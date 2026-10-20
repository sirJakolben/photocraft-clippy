//! Reading and writing single columns: numbers kept in their storage class, writes skipped when
//! the column already reads as the value (so a round trip keeps the bytes), and never a column
//! the row doesn't have.

use photocraft_clippy_sut::{Effector, Sources, Value, Variant};

/// A finite number from an `INTEGER` or `REAL` column.
pub(crate) fn num(v: &Variant, column: &str) -> Option<f64> {
    v.real(column).filter(|x| x.is_finite())
}

/// A number as `f32`, clamped into `lo..=hi`.
pub(crate) fn num_in(v: &Variant, column: &str, lo: f64, hi: f64) -> Option<f32> {
    num(v, column).map(|x| x.clamp(lo, hi) as f32)
}

/// A percentage column (0–100) as a 0..1 fraction.
pub(crate) fn percent(v: &Variant, column: &str) -> Option<f32> {
    num_in(v, column, 0.0, 100.0).map(|p| p / 100.0)
}

/// A flag (`!= 0`) from a numeric column.
pub(crate) fn flag(v: &Variant, column: &str) -> Option<bool> {
    num(v, column).map(|x| x != 0.0)
}

/// An integer from a numeric column (reals rounded), when it fits `i64`.
pub(crate) fn int(v: &Variant, column: &str) -> Option<i64> {
    v.int(column).or_else(|| num(v, column).filter(|x| x.abs() < 9.0e15).map(|x| x.round() as i64))
}

/// Write `value` into `column` when the row has it and it doesn't already read within `eps` of
/// it. An `INTEGER` value (or an empty column declared `INTEGER`) is rounded.
pub(crate) fn put_num(v: &mut Variant, column: &str, value: f64, eps: f64) {
    if !value.is_finite() {
        return;
    }
    let Some(cell) = v.row.cell(column) else { return };
    let integer = match &cell.value {
        Value::Integer(_) => true,
        Value::Real(_) => false,
        _ => cell.decl_type.eq_ignore_ascii_case("INTEGER"),
    };
    let new = if integer && value.abs() < 9.0e15 { Value::Integer(value.round() as i64) } else { Value::Real(value) };
    let current = cell.value.as_real();
    let same = match (current, &new) {
        (Some(c), Value::Integer(n)) => c.round() as i64 == *n && (c - value).abs() <= eps.max(0.5),
        (Some(c), _) => (c - value).abs() <= eps,
        (None, _) => false,
    };
    if !same {
        v.set(column, new);
    }
}

/// [`put_num`] for a 0..1 fraction stored as a percentage.
pub(crate) fn put_percent(v: &mut Variant, column: &str, fraction: f32) {
    put_num(v, column, f64::from(fraction) * 100.0, 1e-3);
}

/// Write a flag as 0/1, unless the column already reads as it.
pub(crate) fn put_flag(v: &mut Variant, column: &str, on: bool) {
    if flag(v, column) != Some(on) {
        put_num(v, column, if on { 1.0 } else { 0.0 }, 0.0);
    }
}

/// Write an integer, unless the column already reads as it.
pub(crate) fn put_int(v: &mut Variant, column: &str, value: i64) {
    if int(v, column) != Some(value) {
        #[allow(clippy::cast_precision_loss)]
        put_num(v, column, value as f64, 0.0);
    }
}

/// Write text, unless the column already holds it.
pub(crate) fn put_text(v: &mut Variant, column: &str, text: &str) {
    if v.row.contains(column) && v.text(column) != Some(text) {
        v.set(column, Value::Text(text.to_owned()));
    }
}

/// The effector of `column`, decoded; `None` when the column is missing, `NULL` or undecodable.
pub(crate) fn effector(v: &Variant, column: &str) -> Option<Effector> {
    v.effector(column).and_then(Result::ok)
}

/// The effector to change for `column`: the stored one, or a new one (pressure, tilt, velocity
/// and random offered) for an empty column. `None` when the row has no such column or holds
/// bytes that don't decode (kept as they are).
pub(crate) fn effector_for_write(v: &Variant, column: &str) -> Option<Effector> {
    match v.get(column)? {
        Value::Null => Some(Effector::new(Sources(0xF0))),
        Value::Blob(b) => Effector::decode(b).ok(),
        _ => None,
    }
}
