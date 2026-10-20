//! PhotoCraft → a new sub tool (the reverse direction): a `.sut` holding PhotoCraft's tool and
//! working copy, laid out like Clip Studio Paint 1.5's exports, with every column the mapping
//! knows. Meant for building sub tools from PhotoCraft settings (the default stock, M7).
//!
//! Brush tips and textures aren't written (embedding materials is the stock's job): a sampled
//! tip leaves the sub tool round.

use photocraft_clippy_sut::{Cell, DbInfo, Row, SchemaObject, SubTool, Sut, Value, Variant, known};
use photocraft_ui_egui::state::Tool;

use super::{Ctx, Working, tool, write_all};

/// Declared type of a column in the template.
fn decl_type(column: &str) -> &'static str {
    const REAL: [&str; 11] = [
        known::BRUSH_SIZE,
        known::BRUSH_ROTATION,
        known::BRUSH_INTERVAL,
        known::BRUSH_IN_LENGTH,
        known::BRUSH_OUT_LENGTH,
        known::TEXTURE_SCALE,
        known::TEXTURE_ROTATE,
        known::DUAL_SIZE,
        known::BRUSH_WATER_EDGE_RADIUS,
        known::BRUSH_SPRAY_SIZE,
        super::options::col::FONT_SIZE,
    ];
    const TEXT: [&str; 2] = [super::options::col::FONT_NAME, super::options::col::FONT_STYLE];
    if known::MATERIAL_COLUMNS.contains(&column) {
        ""
    } else if column.ends_with("Effector") && column != known::BRUSH_ROTATION_EFFECTOR || column == known::SHOW_PARAM {
        "BLOB"
    } else if REAL.contains(&column) {
        "REAL"
    } else if TEXT.contains(&column) {
        "TEXT"
    } else {
        "INTEGER"
    }
}

const MANAGER: [(&str, &str); 7] = [
    ("_PW_ID", "INTEGER"),
    ("ToolType", "INTEGER"),
    ("Version", "INTEGER"),
    ("RootUuid", "BLOB"),
    ("CurrentNodeUuid", "BLOB"),
    ("MaxVariantID", "INTEGER"),
    ("SavedCount", "INTEGER"),
];
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

fn create_sql(table: &str, columns: &[(String, String)]) -> String {
    let cols: Vec<String> = columns
        .iter()
        .map(|(n, t)| match (n.as_str(), t.as_str()) {
            ("_PW_ID", _) => "_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT".to_owned(),
            (n, "") => format!("{n} NULL DEFAULT NULL"),
            (n, t) => format!("{n} {t} DEFAULT NULL"),
        })
        .collect();
    format!("CREATE TABLE {table}({})", cols.join(", "))
}

fn owned(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter().map(|(n, t)| ((*n).to_owned(), (*t).to_owned())).collect()
}

/// The `Variant` columns of a new sub tool: its id, the palette state and every mapped column.
pub fn variant_columns() -> Vec<(String, String)> {
    let mut cols = vec![("_PW_ID".to_owned(), "INTEGER".to_owned()), ("VariantID".to_owned(), "INTEGER".to_owned())];
    cols.push((known::SHOW_PARAM.to_owned(), "BLOB".to_owned()));
    for rule in super::rules() {
        for c in rule.columns {
            if !cols.iter().any(|(n, _)| n == c) {
                cols.push(((*c).to_owned(), decl_type(c).to_owned()));
            }
        }
    }
    cols
}

/// A `.sut` template: `Manager`, `sqlite_sequence`, `Node`, `Variant` (every mapped column),
/// `MaterialFile`.
pub fn template() -> DbInfo {
    let tables = [("Manager", owned(&MANAGER)), ("Node", owned(&NODE)), ("Variant", variant_columns())];
    let mut objects = Vec::new();
    let mut columns = Vec::new();
    for (name, cols) in tables {
        objects.push(SchemaObject { kind: "table".into(), name: name.into(), table: name.into(), sql: Some(create_sql(name, &cols)) });
        columns.push((name.to_owned(), cols));
        if name == "Manager" {
            objects.push(SchemaObject {
                kind: "table".into(),
                name: "sqlite_sequence".into(),
                table: "sqlite_sequence".into(),
                sql: Some("CREATE TABLE sqlite_sequence(name,seq)".into()),
            });
            columns.push(("sqlite_sequence".into(), vec![("name".into(), String::new()), ("seq".into(), String::new())]));
        }
    }
    objects.push(SchemaObject {
        kind: "table".into(),
        name: known::MATERIAL_TABLE.into(),
        table: known::MATERIAL_TABLE.into(),
        sql: Some(known::MATERIAL_FILE_SQL.into()),
    });
    columns.push((known::MATERIAL_TABLE.into(), owned(&known::MATERIAL_FILE_COLUMNS)));
    let manager = row(
        &MANAGER,
        &[
            ("ToolType", Value::Integer(0)),
            ("Version", Value::Integer(138)),
            ("RootUuid", Value::Blob(vec![0])),
            ("CurrentNodeUuid", Value::Blob(vec![0])),
            ("MaxVariantID", Value::Integer(2)),
            ("SavedCount", Value::Integer(1)),
        ],
    );
    DbInfo { page_size: 4096, encoding: "UTF-8".into(), objects, columns, manager, sequence: Vec::new() }
}

/// A row with `values` in the order of `list`, typed as `list` declares.
fn row(list: &[(&str, &str)], values: &[(&str, Value)]) -> Row {
    Row::from_cells(
        values
            .iter()
            .map(|(name, value)| Cell {
                name: (*name).to_owned(),
                decl_type: list.iter().find(|(n, _)| n == name).map_or("", |(_, t)| t).to_owned(),
                value: value.clone(),
            })
            .collect(),
    )
}

/// An empty settings row (every mapped column `NULL`) with id `id`.
fn blank_variant(id: i64) -> Variant {
    let cells = variant_columns()
        .into_iter()
        .filter(|(n, _)| n != "_PW_ID")
        .map(|(name, decl_type)| {
            let value = if name == "VariantID" { Value::Integer(id) } else { Value::Null };
            Cell { name, decl_type, value }
        })
        .collect();
    Variant { row: Row::from_cells(cells) }
}

/// A 16-byte node UUID from `seed` (never all zeros, which means "none").
fn uuid(seed: u64, name: &str) -> Vec<u8> {
    // FNV-1a over the seed and the name, stretched with splitmix64.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in seed.to_le_bytes().iter().chain(name.as_bytes()) {
        h = (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3);
    }
    let mut out = Vec::with_capacity(16);
    for _ in 0..2 {
        h = h.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = h;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        out.extend_from_slice(&(z ^ (z >> 31)).to_be_bytes());
    }
    if out.iter().all(|&b| b == 0)
        && let Some(b) = out.first_mut()
    {
        *b = 1;
    }
    out
}

/// A new sub tool named `name` for `tool` with the settings of `working`; only the columns of the
/// tool's own rules carry values, the others stay `NULL`. `seed` makes its UUID (callers pass the
/// time). Tools that share an In/Out pair are told apart by name when read back (a blur or
/// smudge brush, an ellipse or polygon), so `name` should say which. An error when Clip Studio
/// Paint has no such tool.
pub fn new_sut(name: &str, t: Tool, working: &Working, ctx: Ctx, seed: u64) -> Result<Sut, String> {
    let (input, output) = tool::ops_of(t).ok_or_else(|| format!("Clip Studio Paint has no sub tool for PhotoCraft's {t:?} tool"))?;
    let node = row(
        &NODE,
        &[
            ("NodeUuid", Value::Blob(uuid(seed, name))),
            ("NodeName", Value::Text(name.to_owned())),
            ("NodeShortCutKey", Value::Integer(0)),
            ("NodeLock", Value::Integer(0)),
            ("NodeInputOp", Value::Integer(input)),
            ("NodeOutputOp", Value::Integer(output)),
            ("NodeRangeOp", Value::Integer(0)),
            ("NodeIcon", Value::Integer(0)),
            ("NodeNextUuid", Value::Blob(vec![0])),
            ("NodeFirstChildUuid", Value::Blob(vec![0])),
            ("NodeSelectedUuid", Value::Blob(vec![0])),
            ("NodeVariantID", Value::Integer(1)),
            ("NodeInitVariantID", Value::Integer(2)),
        ],
    );
    let mut variant = blank_variant(1);
    // The Pencil is a pen without anti-aliasing; the blending tools mix colour.
    let mut w = working.clone();
    if t == Tool::Pencil {
        w.brush.aa_level = Some(0);
        w.brush.aliased = true;
    }
    write_all(t, &w, &mut variant, ctx);
    if t == Tool::MixerBrush && !tool::mixes_colour(&variant) {
        variant.set(known::BRUSH_MIX_COLOR, Value::Integer(50));
    }
    let mut init = variant.clone();
    init.set("VariantID", Value::Integer(2));
    let subtool = SubTool { node, variant: Some(variant), init_variant: Some(init), materials: Vec::new() };
    Ok(Sut::from_subtool(subtool, &template()))
}
