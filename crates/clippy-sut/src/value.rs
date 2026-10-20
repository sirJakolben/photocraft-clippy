//! Column values and rows, kept exactly as SQLite stores them.
//!
//! Clip Studio Paint adds and renames columns between versions (a `.todb` of 1.5 has 479 `Variant`
//! columns, an exported `.sut` 184). Rows therefore stay an ordered list of named values with their
//! declared type, so a column this crate does not know survives a read and a write unchanged.

/// One SQLite value with its storage class.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// `NULL`.
    Null,
    /// `INTEGER` storage class.
    Integer(i64),
    /// `REAL` storage class.
    Real(f64),
    /// `TEXT` storage class (decoded to UTF-8; `.todb` files store UTF-16).
    Text(String),
    /// `BLOB` storage class.
    Blob(Vec<u8>),
}

impl Value {
    /// The integer, if this is an `INTEGER` value.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Integer(v) => Some(*v),
            _ => None,
        }
    }

    /// The number as `f64`, if this is a `REAL` or an `INTEGER` value.
    pub fn as_real(&self) -> Option<f64> {
        match self {
            Value::Real(v) => Some(*v),
            #[allow(clippy::cast_precision_loss)]
            Value::Integer(v) => Some(*v as f64),
            _ => None,
        }
    }

    /// The text, if this is a `TEXT` value.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Value::Text(v) => Some(v),
            _ => None,
        }
    }

    /// The bytes, if this is a `BLOB` value.
    pub fn as_blob(&self) -> Option<&[u8]> {
        match self {
            Value::Blob(v) => Some(v),
            _ => None,
        }
    }

    /// Whether this is `NULL`.
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// Equality that treats two NaN reals as equal, so a round trip of a file holding NaN still
    /// compares equal (`PartialEq` follows IEEE and says NaN != NaN).
    pub fn same(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Real(a), Value::Real(b)) => a.to_bits() == b.to_bits() || a == b,
            _ => self == other,
        }
    }
}

/// One column of a row: its name, its declared SQL type and its value.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    /// Column name as the file spells it.
    pub name: String,
    /// Declared type from the table definition (`INTEGER`, `REAL`, `TEXT`, `BLOB` or empty).
    pub decl_type: String,
    /// The stored value.
    pub value: Value,
}

/// An ordered row of named values. Lookups are by exact name first, then ASCII case-insensitive
/// (SQLite column names are case-insensitive).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    cells: Vec<Cell>,
}

impl Row {
    /// An empty row.
    pub fn new() -> Self {
        Self::default()
    }

    /// A row from cells in file order.
    pub fn from_cells(cells: Vec<Cell>) -> Self {
        Self { cells }
    }

    /// All cells in file order.
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    /// Number of columns.
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// Whether the row has no columns.
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// The value of a column, if present.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.position(name).and_then(|i| self.cells.get(i)).map(|c| &c.value)
    }

    /// The cell of a column, if present.
    pub fn cell(&self, name: &str) -> Option<&Cell> {
        self.position(name).and_then(|i| self.cells.get(i))
    }

    /// Whether the column exists.
    pub fn contains(&self, name: &str) -> bool {
        self.position(name).is_some()
    }

    /// Integer value of a column.
    pub fn int(&self, name: &str) -> Option<i64> {
        self.get(name).and_then(Value::as_int)
    }

    /// Numeric value of a column as `f64`.
    pub fn real(&self, name: &str) -> Option<f64> {
        self.get(name).and_then(Value::as_real)
    }

    /// Text value of a column.
    pub fn text(&self, name: &str) -> Option<&str> {
        self.get(name).and_then(Value::as_text)
    }

    /// Blob value of a column.
    pub fn blob(&self, name: &str) -> Option<&[u8]> {
        self.get(name).and_then(Value::as_blob)
    }

    /// Sets a column. An existing column keeps its position and declared type; a new one is appended
    /// with an empty declared type (the writer only accepts columns its template defines).
    pub fn set(&mut self, name: &str, value: Value) {
        match self.position(name).and_then(|i| self.cells.get_mut(i)) {
            Some(cell) => cell.value = value,
            None => self.cells.push(Cell { name: name.to_owned(), decl_type: String::new(), value }),
        }
    }

    /// Removes a column and returns its value.
    pub fn remove(&mut self, name: &str) -> Option<Value> {
        let i = self.position(name)?;
        (i < self.cells.len()).then(|| self.cells.remove(i).value)
    }

    /// Whether both rows hold the same columns with the same values (order and declared types
    /// ignored; NaN equals NaN).
    pub fn same_values(&self, other: &Row) -> bool {
        self.cells.len() == other.cells.len() && self.cells.iter().all(|c| other.get(&c.name).is_some_and(|v| v.same(&c.value)))
    }

    fn position(&self, name: &str) -> Option<usize> {
        self.cells.iter().position(|c| c.name == name).or_else(|| self.cells.iter().position(|c| c.name.eq_ignore_ascii_case(name)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookups_fall_back_to_case_insensitive_names() {
        let mut row = Row::new();
        row.set("BrushSize", Value::Real(12.5));
        assert_eq!(row.real("BrushSize"), Some(12.5));
        assert_eq!(row.real("brushsize"), Some(12.5));
        row.set("brushSIZE", Value::Integer(3));
        assert_eq!(row.len(), 1);
        assert_eq!(row.int("BrushSize"), Some(3));
        assert_eq!(row.remove("BRUSHSIZE"), Some(Value::Integer(3)));
        assert!(row.is_empty());
    }

    #[test]
    fn nan_reals_compare_equal_for_round_trips() {
        assert!(Value::Real(f64::NAN).same(&Value::Real(f64::NAN)));
        assert!(!Value::Real(1.0).same(&Value::Integer(1)));
    }
}
