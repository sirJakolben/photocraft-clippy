//! Synthetic tool files for tests, examples and fuzz seeds (feature `synth`). Everything here is
//! generated; no Clip Studio Paint data is involved.

use rusqlite::{Connection, params_from_iter};

use crate::archive::{ArchiveEntry, MaterialArchive};
use crate::blobs::{MaterialRef, MaterialRefs, ShowParams};
use crate::effector::{Curve, Effector, Source, Sources};
use crate::layer::{Tip, TipFormat};
use crate::model::{DbInfo, Material, SchemaObject, SubTool, Sut, Variant, col};
use crate::value::{Cell, Row, Value};
use crate::{Result, known};

const MANAGER_COLUMNS: [(&str, &str); 12] = [
    ("_PW_ID", "INTEGER"),
    ("ToolType", "INTEGER"),
    ("Version", "INTEGER"),
    ("RootUuid", "BLOB"),
    ("CurrentNodeUuid", "BLOB"),
    ("MaxVariantID", "INTEGER"),
    ("CommonVariantID", "INTEGER"),
    ("ObjectNodeUuid", "BLOB"),
    ("PressureGraph", ""),
    ("SavedCount", "INTEGER"),
    ("PressureGraphInitialized", "INTEGER"),
    ("UsePressureFile", "INTEGER"),
];
const NODE_COLUMNS: [(&str, &str); 14] = [
    ("_PW_ID", "INTEGER"),
    (col::UUID, "BLOB"),
    (col::NAME, "TEXT"),
    (col::SHORTCUT, "INTEGER"),
    (col::LOCK, "INTEGER"),
    (col::INPUT_OP, "INTEGER"),
    (col::OUTPUT_OP, "INTEGER"),
    (col::RANGE_OP, "INTEGER"),
    (col::ICON, "INTEGER"),
    (col::NEXT, "BLOB"),
    (col::FIRST_CHILD, "BLOB"),
    (col::SELECTED, "BLOB"),
    (col::VARIANT, "INTEGER"),
    (col::INIT_VARIANT, "INTEGER"),
];
const VARIANT_COLUMNS: [(&str, &str); 10] = [
    ("_PW_ID", "INTEGER"),
    (col::VARIANT_ID, "INTEGER"),
    (known::SHOW_PARAM, "BLOB"),
    (known::OPACITY, "INTEGER"),
    (known::BRUSH_SIZE, "REAL"),
    (known::BRUSH_SIZE_EFFECTOR, "BLOB"),
    (known::BRUSH_PATTERN_IMAGES, ""),
    ("SyntheticUnknownText", "TEXT"),
    ("SyntheticUnknownBlob", "BLOB"),
    ("SyntheticUnknownNull", "INTEGER"),
];

/// A deterministic pseudo-random sequence (xorshift), so generated files are reproducible.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

fn create_sql(table: &str, columns: &[(&str, &str)]) -> String {
    let cols: Vec<String> = columns
        .iter()
        .map(|(n, t)| {
            if *n == "_PW_ID" {
                "_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT".to_owned()
            } else if t.is_empty() {
                format!("{n} NULL DEFAULT NULL")
            } else {
                format!("{n} {t} DEFAULT NULL")
            }
        })
        .collect();
    format!("CREATE TABLE {table}({})", cols.join(", "))
}

fn columns(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter().map(|(n, t)| ((*n).to_owned(), (*t).to_owned())).collect()
}

/// A `.sut`-like template: `Manager`, `sqlite_sequence`, `Node`, `Variant`, `MaterialFile`.
pub fn template() -> DbInfo {
    let mut objects = Vec::new();
    let mut cols = Vec::new();
    for (name, list) in [("Manager", &MANAGER_COLUMNS[..]), ("Node", &NODE_COLUMNS[..]), ("Variant", &VARIANT_COLUMNS[..])] {
        objects.push(SchemaObject { kind: "table".into(), name: name.into(), table: name.into(), sql: Some(create_sql(name, list)) });
        cols.push((name.to_owned(), columns(list)));
        if name == "Manager" {
            objects.push(SchemaObject {
                kind: "table".into(),
                name: "sqlite_sequence".into(),
                table: "sqlite_sequence".into(),
                sql: Some("CREATE TABLE sqlite_sequence(name,seq)".into()),
            });
            cols.push(("sqlite_sequence".into(), vec![("name".into(), String::new()), ("seq".into(), String::new())]));
        }
    }
    objects.push(SchemaObject {
        kind: "table".into(),
        name: known::MATERIAL_TABLE.into(),
        table: known::MATERIAL_TABLE.into(),
        sql: Some(known::MATERIAL_FILE_SQL.into()),
    });
    cols.push((known::MATERIAL_TABLE.into(), columns(&known::MATERIAL_FILE_COLUMNS)));
    let mut manager = Row::new();
    for (name, value) in [
        ("ToolType", Value::Integer(0)),
        ("Version", Value::Integer(138)),
        ("RootUuid", Value::Blob(vec![0])),
        ("CurrentNodeUuid", Value::Blob(vec![0])),
        ("MaxVariantID", Value::Integer(1000)),
        ("CommonVariantID", Value::Integer(44)),
        ("ObjectNodeUuid", Value::Blob(vec![0x42; 16])),
        ("PressureGraph", Value::Null),
        ("SavedCount", Value::Integer(1)),
        ("PressureGraphInitialized", Value::Integer(1)),
        ("UsePressureFile", Value::Integer(1)),
    ] {
        push(&mut manager, &MANAGER_COLUMNS, name, value);
    }
    DbInfo { page_size: 4096, encoding: "UTF-8".into(), objects, columns: cols, manager, sequence: Vec::new() }
}

fn push(row: &mut Row, list: &[(&str, &str)], name: &str, value: Value) {
    let decl = list.iter().find(|(n, _)| *n == name).map_or("", |(_, t)| t);
    let mut cells = row.cells().to_vec();
    cells.push(Cell { name: name.into(), decl_type: decl.into(), value });
    *row = Row::from_cells(cells);
}

/// A generated brush tip: a soft disc with some noise, `size`×`size`.
pub fn tip(size: u32, seed: u64) -> Tip {
    let mut rng = Rng(seed | 1);
    let c = f64::from(size) / 2.0;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let d = ((f64::from(x) + 0.5 - c).powi(2) + (f64::from(y) + 0.5 - c).powi(2)).sqrt() / c;
            let a = ((1.0 - d).clamp(0.0, 1.0) * 255.0) as u8;
            let v = (rng.below(64)) as u8;
            rgba.extend_from_slice(&[v, v, v, a]);
        }
    }
    Tip { width: size, height: size, rgba, format: TipFormat::GrayAlpha8 }
}

/// A generated sub tool with current and initial settings and `materials` embedded tips.
pub fn subtool(seed: u64, materials: usize) -> Result<SubTool> {
    let mut rng = Rng(seed | 1);
    let uuid: Vec<u8> = (0..16).map(|_| rng.below(255) as u8 + 1).collect();
    let mut node = Row::new();
    for (name, value) in [
        (col::UUID, Value::Blob(uuid)),
        (col::NAME, Value::Text(format!("Synthetic pen {seed} ✒"))),
        (col::SHORTCUT, Value::Integer(0)),
        (col::LOCK, Value::Integer(0)),
        (col::INPUT_OP, Value::Integer(10)),
        (col::OUTPUT_OP, Value::Integer(10)),
        (col::RANGE_OP, Value::Integer(0)),
        (col::ICON, Value::Integer(110)),
        (col::NEXT, Value::Blob(vec![0])),
        (col::FIRST_CHILD, Value::Blob(vec![0])),
        (col::SELECTED, Value::Blob(vec![0])),
        (col::VARIANT, Value::Integer(977)),
        (col::INIT_VARIANT, Value::Integer(978)),
    ] {
        push(&mut node, &NODE_COLUMNS, name, value);
    }
    let mut mats = Vec::new();
    let mut refs = MaterialRefs::default();
    for i in 0..materials {
        let uuid =
            format!("{:010x}-{:04x}-{:04x}-{:04x}-{:010x}", rng.next() & 0xff_ffff_ffff, i, rng.below(0xffff), rng.below(0xffff), rng.next() & 0xff_ffff_ffff);
        let catalog = format!(".:45:36:{uuid}");
        let layer = crate::layer::encode(&tip(24 + 8 * i as u32, seed + i as u64))?;
        let archive = MaterialArchive {
            entries: vec![
                ArchiveEntry { path: "catalog.zip".into(), data: vec![0x50, 0x4b, 5, 6] },
                ArchiveEntry { path: "data/material_0.layer".into(), data: layer },
            ],
            mtime: 1_700_000_000,
        };
        let mut row = Row::new();
        for (name, value) in [
            ("InstallFolder", Value::Integer(0)),
            ("OriginalPath", Value::Text(format!("{catalog}:data:material_0.layer"))),
            ("OldMaterial", Value::Integer(0)),
            ("FileData", Value::Blob(archive.encode()?)),
            ("CatalogPath", Value::Text(catalog.clone())),
            ("MaterialUuid", Value::Text(String::new())),
        ] {
            push(&mut row, &known::MATERIAL_FILE_COLUMNS, name, value);
        }
        mats.push(Material { row });
        refs.entries.push(MaterialRef { path: format!("{catalog}:data:material_0.layer"), rest: vec![0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 1] });
    }
    let variant = |id: i64, rng: &mut Rng| -> Result<Variant> {
        let mut effector = Effector::new(Sources(0x1f0));
        effector.set_active(Sources(0).with(Source::Pressure, true));
        effector.set_minimum(Source::Pressure, rng.below(100) as i32);
        effector.set_pressure_curve(Some(Curve::new(vec![(0.0, 0.0), (0.3, rng.below(100) as f64 / 100.0), (1.0, 1.0)])));
        let mut row = Row::new();
        for (name, value) in [
            (col::VARIANT_ID, Value::Integer(id)),
            (known::SHOW_PARAM, Value::Blob(ShowParams { ids: vec![2000, 2010, 3231] }.encode())),
            (known::OPACITY, Value::Integer(rng.below(101) as i64)),
            (known::BRUSH_SIZE, Value::Real(rng.below(1000) as f64 / 10.0)),
            (known::BRUSH_SIZE_EFFECTOR, Value::Blob(effector.encode())),
            (known::BRUSH_PATTERN_IMAGES, if refs.entries.is_empty() { Value::Null } else { Value::Blob(refs.encode()?) }),
            ("SyntheticUnknownText", Value::Text("kept as is".into())),
            ("SyntheticUnknownBlob", Value::Blob((0..rng.below(300)).map(|i| i as u8).collect())),
            ("SyntheticUnknownNull", Value::Null),
        ] {
            push(&mut row, &VARIANT_COLUMNS, name, value);
        }
        Ok(Variant { row })
    };
    let current = variant(977, &mut rng)?;
    let init = variant(978, &mut rng)?;
    Ok(SubTool { node, variant: Some(current), init_variant: Some(init), materials: mats })
}

/// A generated `.sut`.
pub fn sut(seed: u64, materials: usize) -> Result<Sut> {
    Ok(Sut::from_subtool(subtool(seed, materials)?, &template()))
}

/// A generated `.todb` (UTF-16) with `shape[t][g]` sub tools in group `g` of tool `t`. Node
/// links can be bent afterwards with `link` = (from node index, column, to node index) to build
/// broken trees; index 0 is the root, then nodes in depth-first order.
pub fn todb(shape: &[&[usize]], links: &[(usize, &str, Option<usize>)]) -> Result<Vec<u8>> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch("PRAGMA page_size = 1024; PRAGMA encoding = 'UTF-16le';")?;
    let info = template();
    for o in info.objects.iter().filter(|o| o.name != "sqlite_sequence" && o.name != known::MATERIAL_TABLE) {
        if let Some(sql) = &o.sql {
            conn.execute(sql, [])?;
        }
    }
    // Nodes in depth-first order with their parent and depth.
    let mut nodes: Vec<(Option<usize>, usize, String)> = vec![(None, 0, String::new())];
    for (t, groups) in shape.iter().enumerate() {
        let tool = nodes.len();
        nodes.push((Some(0), 1, format!("Tool {t}")));
        for (g, &subs) in groups.iter().enumerate() {
            let group = nodes.len();
            nodes.push((Some(tool), 2, format!("Group {t}.{g}")));
            for s in 0..subs {
                nodes.push((Some(group), 3, format!("Sub {t}.{g}.{s}")));
            }
        }
    }
    let uuid = |i: usize| -> Vec<u8> { (0..16).map(|k| (i as u8).wrapping_mul(31).wrapping_add(k as u8 * 7) | 1).collect() };
    let mut link_values: Vec<(Value, Value)> = vec![(Value::Blob(vec![0]), Value::Blob(vec![0])); nodes.len()];
    for i in 0..nodes.len() {
        let parent = nodes[i].0;
        let next = (i + 1..nodes.len()).find(|&j| nodes[j].1 <= nodes[i].1).filter(|&j| nodes[j].0 == parent);
        let child = nodes.get(i + 1).filter(|n| n.0 == Some(i)).map(|_| i + 1);
        link_values[i] = (next.map_or(Value::Blob(vec![0]), |j| Value::Blob(uuid(j))), child.map_or(Value::Blob(vec![0]), |j| Value::Blob(uuid(j))));
    }
    for &(from, column, to) in links {
        if let Some(slot) = link_values.get_mut(from) {
            let v = to.map_or(Value::Blob(vec![0]), |j| Value::Blob(uuid(j)));
            if column == col::NEXT { slot.0 = v } else { slot.1 = v }
        }
    }
    let mut manager = info.manager.clone();
    manager.set(col::ROOT, Value::Blob(uuid(0)));
    insert(&conn, "Manager", 1, &manager)?;
    let mut variant_id = 0;
    for (i, (_, depth, name)) in nodes.iter().enumerate() {
        let mut row = Row::new();
        row.set(col::UUID, Value::Blob(uuid(i)));
        row.set(col::NAME, Value::Text(name.clone()));
        row.set(col::NEXT, link_values[i].0.clone());
        row.set(col::FIRST_CHILD, link_values[i].1.clone());
        if *depth == 3 {
            for column in [col::VARIANT, col::INIT_VARIANT] {
                variant_id += 1;
                row.set(column, Value::Integer(variant_id));
                let mut v = Row::new();
                v.set(col::VARIANT_ID, Value::Integer(variant_id));
                v.set(known::BRUSH_SIZE, Value::Real(variant_id as f64));
                insert(&conn, "Variant", variant_id, &v)?;
            }
        }
        insert(&conn, "Node", i as i64 + 1, &row)?;
    }
    Ok(conn.serialize("main")?.to_vec())
}

fn insert(conn: &Connection, table: &str, id: i64, row: &Row) -> Result<()> {
    let mut names = vec!["_PW_ID".to_owned()];
    let mut values = vec![rusqlite::types::Value::Integer(id)];
    for c in row.cells() {
        names.push(c.name.clone());
        values.push(match &c.value {
            Value::Null => rusqlite::types::Value::Null,
            Value::Integer(i) => rusqlite::types::Value::Integer(*i),
            Value::Real(f) => rusqlite::types::Value::Real(*f),
            Value::Text(t) => rusqlite::types::Value::Text(t.clone()),
            Value::Blob(b) => rusqlite::types::Value::Blob(b.clone()),
        });
    }
    let marks = (1..=values.len()).map(|i| format!("?{i}")).collect::<Vec<_>>().join(", ");
    conn.execute(&format!("INSERT INTO {table} ({}) VALUES ({marks})", names.join(", ")), params_from_iter(values))?;
    Ok(())
}
