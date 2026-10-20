//! SQLite access: reading `.todb` and `.sut`, writing `.sut`. Desktop only.
//!
//! Databases are opened from memory (`sqlite3_deserialize`, read-only), so a hostile file never
//! touches the disk and nothing is written back. Writing builds the `.sut` in an in-memory database
//! from the template's own `CREATE TABLE` statements and serializes it.

use std::fs;
use std::io::Write;
use std::path::Path;

use rusqlite::types::{Value as SqlValue, ValueRef};
use rusqlite::{Connection, OpenFlags, params_from_iter};

use crate::model::{DbInfo, NULL_UUID, RawDb, SchemaObject, Sut, Todb, Variant, col};
use crate::value::{Cell, Row, Value};
use crate::{Error, Limits, Result, check_limit, known, malformed};

const PW_ID: &str = "_PW_ID";
const MANAGER: &str = "Manager";
const NODE: &str = "Node";
const VARIANT: &str = "Variant";

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::Sqlite(e.to_string())
    }
}

impl Todb {
    /// Reads a `.todb` file.
    pub fn open(path: &Path) -> Result<Self> {
        Self::from_bytes(&read_capped(path, &Limits::default())?, &Limits::default())
    }

    /// Reads a `.todb` from its bytes.
    pub fn from_bytes(bytes: &[u8], limits: &Limits) -> Result<Self> {
        read_raw(bytes, limits, false)?.into_todb(limits)
    }

    /// Every sub tool as a stand-alone `.sut`, laid out like this database (the `.todb`
    /// columns, UTF-8). Embedded materials are empty; M7 adds them from the material folder.
    pub fn subtool_suts(&self) -> impl Iterator<Item = Sut> + '_ {
        self.tree.subtools().map(|(_, _, s)| Sut::from_subtool(s.clone(), &self.info))
    }
}

impl Sut {
    /// Reads a `.sut` file.
    pub fn open(path: &Path) -> Result<Self> {
        Self::from_bytes(&read_capped(path, &Limits::default())?, &Limits::default())
    }

    /// Reads a `.sut` from its bytes.
    pub fn from_bytes(bytes: &[u8], limits: &Limits) -> Result<Self> {
        read_raw(bytes, limits, true)?.into_sut(limits)
    }

    /// The `.sut` file bytes: the template's tables and columns, UTF-8, one node.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        write_sut(self)
    }

    /// Writes the `.sut` atomically: a temporary file next to `path`, synced, then renamed over it.
    pub fn save(&self, path: &Path) -> Result<()> {
        write_atomic(path, &self.to_bytes()?)
    }
}

/// Writes `bytes` to `path` through a temporary file in the same folder, so a crash or a full disk
/// never leaves a half-written file under the final name.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let name = path.file_name().and_then(|n| n.to_str()).ok_or_else(|| Error::Io(std::io::Error::other("path has no file name")))?;
    let tmp = path.with_file_name(format!(".{name}.tmp{}", std::process::id()));
    let result = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    Ok(result?)
}

fn read_capped(path: &Path, limits: &Limits) -> Result<Vec<u8>> {
    let len = fs::metadata(path)?.len();
    check_limit("file size", len, limits.max_file_bytes)?;
    Ok(fs::read(path)?)
}

fn open_bytes(bytes: &[u8]) -> Result<Connection> {
    if bytes.len() < 100 || !bytes.starts_with(b"SQLite format 3\0") {
        return malformed("tool database", "not an SQLite database");
    }
    // A cut-off file must not read as a shorter valid one: SQLite fills missing pages of an
    // in-memory image with zeros. The header's page count is valid when the change counter
    // (offset 24) equals the version-valid-for number (offset 92).
    let be = |at: usize| bytes.get(at..at + 4).and_then(|b| <[u8; 4]>::try_from(b).ok()).map(u32::from_be_bytes);
    let page_size = bytes.get(16..18).map_or(0, |b| match u16::from_be_bytes([b[0], b[1]]) {
        1 => 65_536,
        v => usize::from(v),
    });
    if let (Some(pages), Some(counter), Some(valid_for)) = (be(28), be(24), be(92)) {
        if counter == valid_for && pages != 0 && (pages as usize).checked_mul(page_size) != Some(bytes.len()) {
            return malformed("tool database", format!("{} bytes, the header says {pages} pages of {page_size}", bytes.len()));
        }
    }
    let mut conn = Connection::open_in_memory_with_flags(OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    conn.deserialize_read_exact("main", bytes, bytes.len(), true)?;
    // Hostile schemas must not run code (views, triggers) and corrupt pages must fail early.
    conn.execute_batch("PRAGMA trusted_schema = OFF; PRAGMA cell_size_check = ON; PRAGMA query_only = ON;")?;
    let check: String = conn.query_row("PRAGMA quick_check(1)", [], |r| r.get(0))?;
    if check != "ok" {
        return malformed("tool database", check);
    }
    Ok(conn)
}

fn read_raw(bytes: &[u8], limits: &Limits, with_materials: bool) -> Result<RawDb> {
    check_limit("file size", bytes.len() as u64, limits.max_file_bytes)?;
    let conn = open_bytes(bytes)?;
    let page_size: i64 = conn.query_row("PRAGMA page_size", [], |r| r.get(0))?;
    let encoding: String = conn.query_row("PRAGMA encoding", [], |r| r.get(0))?;
    let mut objects = Vec::new();
    {
        let mut stmt = conn.prepare("SELECT type, name, tbl_name, sql FROM sqlite_master WHERE type IN ('table', 'index') ORDER BY rowid")?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            objects.push(SchemaObject { kind: r.get(0)?, name: r.get(1)?, table: r.get(2)?, sql: r.get(3)? });
            check_limit("schema objects", objects.len() as u64, 10_000)?;
        }
    }
    let mut columns = Vec::new();
    for o in objects.iter().filter(|o| o.kind == "table") {
        columns.push((o.name.clone(), table_columns(&conn, &o.name)?));
    }
    let has = |t: &str| objects.iter().any(|o| o.kind == "table" && o.name.eq_ignore_ascii_case(t));
    for required in [MANAGER, NODE, VARIANT] {
        if !has(required) {
            return malformed("tool database", format!("no {required} table"));
        }
    }
    let mut managers = read_table(&conn, MANAGER, 2)?;
    if managers.len() != 1 {
        return malformed("tool database", format!("{} Manager rows", managers.len()));
    }
    let manager = managers.pop().unwrap_or_default();
    let sequence = if has("sqlite_sequence") {
        let mut stmt = conn.prepare("SELECT name, seq FROM sqlite_sequence ORDER BY rowid")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        rows.take(10_000).collect::<std::result::Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };
    let nodes = read_table(&conn, NODE, limits.max_nodes)?;
    let variants = read_table(&conn, VARIANT, limits.max_rows)?;
    let materials = if with_materials && has(known::MATERIAL_TABLE) { read_table(&conn, known::MATERIAL_TABLE, limits.max_rows)? } else { Vec::new() };
    let page_size = u32::try_from(page_size).unwrap_or(4096);
    Ok(RawDb { info: DbInfo { page_size, encoding, objects, columns, manager, sequence }, nodes, variants, materials })
}

fn table_columns(conn: &Connection, table: &str) -> Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", quote(table)))?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?.unwrap_or_default())))?;
    Ok(rows.take(100_000).collect::<std::result::Result<Vec<_>, _>>()?)
}

/// All rows of `table` in rowid order, without `_PW_ID`.
fn read_table(conn: &Connection, table: &str, max_rows: u64) -> Result<Vec<Row>> {
    let decl = table_columns(conn, table)?;
    let mut stmt = conn.prepare(&format!("SELECT * FROM {} ORDER BY rowid", quote(table)))?;
    let names: Vec<String> = stmt.column_names().into_iter().map(str::to_owned).collect();
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(r) = rows.next()? {
        check_limit("rows", out.len() as u64 + 1, max_rows)?;
        let mut cells = Vec::with_capacity(names.len());
        for (i, name) in names.iter().enumerate() {
            if name == PW_ID {
                continue;
            }
            let decl_type = decl.iter().find(|(n, _)| n == name).map(|(_, t)| t.clone()).unwrap_or_default();
            cells.push(Cell { name: name.clone(), decl_type, value: from_sql(r.get_ref(i)?) });
        }
        out.push(Row::from_cells(cells));
    }
    Ok(out)
}

fn from_sql(v: ValueRef<'_>) -> Value {
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => Value::Integer(i),
        ValueRef::Real(f) => Value::Real(f),
        ValueRef::Text(t) => Value::Text(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(b) => Value::Blob(b.to_vec()),
    }
}

fn to_sql(v: &Value) -> SqlValue {
    match v {
        Value::Null => SqlValue::Null,
        Value::Integer(i) => SqlValue::Integer(*i),
        Value::Real(f) => SqlValue::Real(*f),
        Value::Text(t) => SqlValue::Text(t.clone()),
        Value::Blob(b) => SqlValue::Blob(b.clone()),
    }
}

fn quote(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn write_sut(sut: &Sut) -> Result<Vec<u8>> {
    let info = &sut.info;
    let conn = Connection::open_in_memory()?;
    let page_size = if info.page_size.is_power_of_two() && (512..=65_536).contains(&info.page_size) { info.page_size } else { 4096 };
    conn.execute_batch(&format!("PRAGMA page_size = {page_size}; PRAGMA encoding = 'UTF-8';"))?;
    for object in &info.objects {
        let Some(sql) = &object.sql else { continue };
        if object.name.eq_ignore_ascii_case("sqlite_sequence") {
            continue; // created by SQLite with the first AUTOINCREMENT table
        }
        let head = sql.trim_start().to_ascii_uppercase();
        if !(head.starts_with("CREATE TABLE") || head.starts_with("CREATE INDEX") || head.starts_with("CREATE UNIQUE INDEX")) {
            return Err(Error::Unsupported { what: "template schema", reason: format!("statement for {}", object.name) });
        }
        conn.execute(sql, [])?;
    }
    for required in [MANAGER, NODE, VARIANT] {
        if !info.has_table(required) {
            return malformed("template", format!("no {required} table"));
        }
    }

    insert(&conn, info, MANAGER, 1, &info.manager)?;
    let mut node = sut.subtool.node.clone();
    node.set(col::NEXT, Value::Blob(NULL_UUID.to_vec()));
    node.set(col::FIRST_CHILD, Value::Blob(NULL_UUID.to_vec()));
    insert(&conn, info, NODE, 1, &node)?;
    let mut variants: Vec<&Variant> = sut.subtool.variant.iter().collect();
    if let Some(init) = &sut.subtool.init_variant {
        if variants.iter().all(|v| v.id() != init.id()) {
            variants.push(init);
        }
    }
    for (i, v) in variants.iter().enumerate() {
        insert(&conn, info, VARIANT, i as i64 + 1, &v.row)?;
    }
    if !sut.subtool.materials.is_empty() && !info.has_table(known::MATERIAL_TABLE) {
        return malformed("template", "no MaterialFile table for the embedded materials");
    }
    for (i, m) in sut.subtool.materials.iter().enumerate() {
        insert(&conn, info, known::MATERIAL_TABLE, i as i64 + 1, &m.row)?;
    }
    Ok(conn.serialize("main")?.to_vec())
}

fn insert(conn: &Connection, info: &DbInfo, table: &str, pw_id: i64, row: &Row) -> Result<()> {
    let columns = info.table_columns(table).ok_or_else(|| Error::UnknownColumn { table: table.into(), column: String::new() })?;
    let mut names = vec![quote(PW_ID)];
    let mut values = vec![SqlValue::Integer(pw_id)];
    for cell in row.cells() {
        if !columns.iter().any(|(n, _)| n.eq_ignore_ascii_case(&cell.name)) {
            return Err(Error::UnknownColumn { table: table.into(), column: cell.name.clone() });
        }
        names.push(quote(&cell.name));
        values.push(to_sql(&cell.value));
    }
    let placeholders = (1..=values.len()).map(|i| format!("?{i}")).collect::<Vec<_>>().join(", ");
    let sql = format!("INSERT INTO {} ({}) VALUES ({placeholders})", quote(table), names.join(", "));
    conn.execute(&sql, params_from_iter(values))?;
    Ok(())
}
