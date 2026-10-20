//! New `.sut` files for brushes that don't come from Clip Studio Paint (PhotoCraft's `.abr`
//! import): a small schema with the tables of an exported sub tool, the settings Clippy can state
//! without the mapping of M8 (size, opacity, the tip), and the whole PhotoCraft brush preset as
//! deflated JSON in the column [`PRESET_COLUMN`], so nothing of the brush is lost.

use std::io::{Read, Write};

use photocraft_clippy_sut::{
    Cell, DbInfo, Material, MaterialArchive, MaterialRef, MaterialRefs, Row, SchemaObject, SubTool, Sut, Value, Variant, known, layer,
};
use photocraft_paint::{BrushPreset, TipShape};

use crate::{Error, Result};

/// `Variant` column holding the PhotoCraft brush preset (zlib-compressed JSON).
pub const PRESET_COLUMN: &str = "PhotocraftBrushPreset";
/// Largest preset JSON read back.
pub const MAX_PRESET_JSON: u64 = 256 << 20;

const MANAGER: [(&str, &str); 6] =
    [("_PW_ID", "INTEGER"), ("ToolType", "INTEGER"), ("Version", "INTEGER"), ("RootUuid", "BLOB"), ("CurrentNodeUuid", "BLOB"), ("MaxVariantID", "INTEGER")];
const NODE: [(&str, &str); 14] = [
    ("_PW_ID", "INTEGER"),
    ("NodeUuid", "BLOB"),
    ("NodeName", "TEXT"),
    ("NodeShortCutKey", "INTEGER"),
    ("NodeLock", "INTEGER"),
    ("NodeInputOp", "INTEGER"),
    ("NodeOutputOp", "INTEGER"),
    ("NodeRangeOp", "INTEGER"),
    ("NodeIcon", "INTEGER"),
    ("NodeNextUuid", "BLOB"),
    ("NodeFirstChildUuid", "BLOB"),
    ("NodeSelectedUuid", "BLOB"),
    ("NodeVariantID", "INTEGER"),
    ("NodeInitVariantID", "INTEGER"),
];
const VARIANT: [(&str, &str); 7] = [
    ("_PW_ID", "INTEGER"),
    ("VariantID", "INTEGER"),
    (known::BRUSH_SIZE, "REAL"),
    (known::OPACITY, "INTEGER"),
    (known::BRUSH_USE_PATTERN_IMAGE, "INTEGER"),
    (known::BRUSH_PATTERN_IMAGES, "BLOB"),
    (PRESET_COLUMN, "BLOB"),
];
/// Node columns copied from a sub tool of the target tool, so the new one behaves like its
/// neighbours (what kind of tool it is).
pub const TOOL_KIND_COLUMNS: [&str; 3] = ["NodeInputOp", "NodeOutputOp", "NodeRangeOp"];

fn create_sql(table: &str, columns: &[(&str, &str)]) -> String {
    let cols: Vec<String> = columns
        .iter()
        .map(|(n, t)| if *n == "_PW_ID" { "_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT".to_owned() } else { format!("{n} {t} DEFAULT NULL") })
        .collect();
    format!("CREATE TABLE {table}({})", cols.join(", "))
}

fn row(list: &[(&str, &str)], values: Vec<(&str, Value)>) -> Row {
    let cells = values
        .into_iter()
        .map(|(name, value)| Cell { name: name.to_owned(), decl_type: list.iter().find(|(n, _)| *n == name).map_or("", |(_, t)| t).to_owned(), value })
        .collect();
    Row::from_cells(cells)
}

/// The schema of new sub tools.
pub fn template() -> DbInfo {
    let mut objects = Vec::new();
    let mut columns = Vec::new();
    for (name, list) in [("Manager", &MANAGER[..]), ("Node", &NODE[..]), ("Variant", &VARIANT[..])] {
        objects.push(SchemaObject { kind: "table".into(), name: name.into(), table: name.into(), sql: Some(create_sql(name, list)) });
        columns.push((name.to_owned(), list.iter().map(|(n, t)| ((*n).to_owned(), (*t).to_owned())).collect()));
    }
    objects.push(SchemaObject {
        kind: "table".into(),
        name: known::MATERIAL_TABLE.into(),
        table: known::MATERIAL_TABLE.into(),
        sql: Some(known::MATERIAL_FILE_SQL.into()),
    });
    columns.push((known::MATERIAL_TABLE.into(), known::MATERIAL_FILE_COLUMNS.iter().map(|(n, t)| ((*n).to_owned(), (*t).to_owned())).collect()));
    let manager = row(
        &MANAGER,
        vec![
            ("ToolType", Value::Integer(0)),
            ("Version", Value::Integer(138)),
            ("RootUuid", Value::Blob(vec![0])),
            ("CurrentNodeUuid", Value::Blob(vec![0])),
            ("MaxVariantID", Value::Integer(2)),
        ],
    );
    DbInfo { page_size: 4096, encoding: "UTF-8".into(), objects, columns, manager, sequence: Vec::new() }
}

/// A sub tool for a PhotoCraft brush preset. `kind` holds [`TOOL_KIND_COLUMNS`] values to copy.
pub fn preset_sut(preset: &BrushPreset, uuid: Vec<u8>, kind: &[(String, Value)]) -> Result<Sut> {
    let catalog = format!(".:photocraft:{}", crate::fsutil::hex(&uuid));
    let mut materials = Vec::new();
    let mut refs = MaterialRefs::default();
    if let TipShape::Sampled(tile) = &preset.brush.tip {
        // The tip as a mask (alpha = coverage), the way Clip Studio Paint stores most tips.
        let mut rgba = Vec::with_capacity(tile.data.len().saturating_mul(4));
        for &v in tile.data.iter() {
            rgba.extend_from_slice(&[0, 0, 0, (u32::from(v) * 255 / 65_535) as u8]);
        }
        let tip = layer::Tip { width: tile.width, height: tile.height, rgba, format: layer::TipFormat::Alpha8 };
        let bytes = layer::encode(&tip).map_err(|e| Error::Invalid(format!("brush {}: its tip can't be stored: {e}", preset.name)))?;
        let file = "material_0.layer";
        let archive = MaterialArchive { entries: vec![photocraft_clippy_sut::ArchiveEntry { path: format!("data/{file}"), data: bytes }], mtime: 0 };
        let path = format!("{catalog}:data:{file}");
        materials.push(Material {
            row: row(
                &known::MATERIAL_FILE_COLUMNS,
                vec![
                    ("InstallFolder", Value::Integer(0)),
                    ("OriginalPath", Value::Text(path.clone())),
                    ("OldMaterial", Value::Integer(0)),
                    ("FileData", Value::Blob(archive.encode().map_err(|e| Error::Invalid(e.to_string()))?)),
                    ("CatalogPath", Value::Text(catalog.clone())),
                    ("MaterialUuid", Value::Text(String::new())),
                ],
            ),
        });
        // Type 3, no display name, one more word: as in the generated test files of clippy-sut.
        refs.entries.push(MaterialRef { path, rest: vec![0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 1] });
    }
    let json = serde_json::to_vec(preset).map_err(|e| Error::Invalid(format!("brush {}: {e}", preset.name)))?;
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(&json).and_then(|()| z.flush()).map_err(|e| Error::Invalid(e.to_string()))?;
    let packed = z.finish().map_err(|e| Error::Invalid(e.to_string()))?;
    let patterns = if refs.entries.is_empty() { Value::Null } else { Value::Blob(refs.encode().map_err(|e| Error::Invalid(e.to_string()))?) };
    let variant = |id: i64| Variant {
        row: row(
            &VARIANT,
            vec![
                ("VariantID", Value::Integer(id)),
                (known::BRUSH_SIZE, Value::Real(f64::from(preset.brush.size))),
                (known::OPACITY, Value::Integer((f64::from(preset.brush.opacity.clamp(0.0, 1.0)) * 100.0).round() as i64)),
                (known::BRUSH_USE_PATTERN_IMAGE, Value::Integer(i64::from(!refs.entries.is_empty()))),
                (known::BRUSH_PATTERN_IMAGES, patterns.clone()),
                (PRESET_COLUMN, Value::Blob(packed.clone())),
            ],
        ),
    };
    let mut node_values = vec![
        ("NodeUuid", Value::Blob(uuid)),
        ("NodeName", Value::Text(preset.name.clone())),
        ("NodeShortCutKey", Value::Integer(0)),
        ("NodeLock", Value::Integer(0)),
        ("NodeNextUuid", Value::Blob(vec![0])),
        ("NodeFirstChildUuid", Value::Blob(vec![0])),
        ("NodeSelectedUuid", Value::Blob(vec![0])),
        ("NodeVariantID", Value::Integer(1)),
        ("NodeInitVariantID", Value::Integer(2)),
    ];
    for (name, value) in kind {
        if let Some(column) = TOOL_KIND_COLUMNS.iter().find(|c| *c == name) {
            node_values.push((column, value.clone()));
        }
    }
    let subtool = SubTool { node: row(&NODE, node_values), variant: Some(variant(1)), init_variant: Some(variant(2)), materials };
    Ok(Sut::from_subtool(subtool, &template()))
}

/// The PhotoCraft brush preset stored in a sub tool written by [`preset_sut`], if it has one.
pub fn stored_preset(variant: &Variant) -> Option<std::result::Result<BrushPreset, String>> {
    let packed = variant.blob(PRESET_COLUMN)?;
    let mut json = Vec::new();
    let read = flate2::read::ZlibDecoder::new(packed).take(MAX_PRESET_JSON + 1).read_to_end(&mut json);
    Some(match read {
        Err(e) => Err(e.to_string()),
        Ok(n) if n as u64 > MAX_PRESET_JSON => Err("the stored brush is too large".into()),
        Ok(_) => serde_json::from_slice(&json).map_err(|e| e.to_string()),
    })
}
