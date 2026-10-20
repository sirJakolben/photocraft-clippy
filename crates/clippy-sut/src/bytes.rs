//! Bounds-checked byte reading and writing for the blob and `.layer` codecs.

use crate::{Error, Result};

/// A cursor over a byte slice whose every read is checked; running past the end is an error naming
/// `what`.
pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
    what: &'static str,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8], what: &'static str) -> Self {
        Self { bytes, pos: 0, what }
    }

    pub(crate) fn at(bytes: &'a [u8], pos: usize, what: &'static str) -> Self {
        Self { bytes, pos, what }
    }

    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    pub(crate) fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.pos)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    pub(crate) fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).filter(|&end| end <= self.bytes.len()).ok_or_else(|| self.truncated(n))?;
        let slice = self.bytes.get(self.pos..end).ok_or_else(|| self.truncated(n))?;
        self.pos = end;
        Ok(slice)
    }

    pub(crate) fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let slice = self.take(N)?;
        let mut out = [0u8; N];
        out.copy_from_slice(slice);
        Ok(out)
    }

    pub(crate) fn u32_be(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.array()?))
    }

    pub(crate) fn u32_le(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    pub(crate) fn i32_be(&mut self) -> Result<i32> {
        Ok(i32::from_be_bytes(self.array()?))
    }

    pub(crate) fn f64_be(&mut self) -> Result<f64> {
        Ok(f64::from_be_bytes(self.array()?))
    }

    /// A `u32` length or count that must fit `usize`.
    pub(crate) fn len_be(&mut self) -> Result<usize> {
        let v = self.u32_be()?;
        usize::try_from(v).map_err(|_| self.error(format!("length {v} does not fit memory")))
    }

    /// A label as Clip Studio Paint writes them: `u32` BE character count, then UTF-16 BE.
    pub(crate) fn label(&mut self) -> Result<String> {
        let chars = self.len_be()?;
        let bytes = self.take(chars.checked_mul(2).ok_or_else(|| self.error("label length overflows"))?)?;
        let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes(*c)).collect();
        Ok(String::from_utf16_lossy(&units))
    }

    /// Reads a label and requires it to be `expected`.
    pub(crate) fn expect_label(&mut self, expected: &str) -> Result<()> {
        let at = self.pos;
        let label = self.label()?;
        if label == expected { Ok(()) } else { Err(self.error(format!("expected {expected:?} at {at}, found {label:?}"))) }
    }

    /// Whether the next bytes are the label `expected`, without consuming anything.
    pub(crate) fn peek_label(&self, expected: &str) -> bool {
        let mut probe = Reader::at(self.bytes, self.pos, self.what);
        probe.label().is_ok_and(|l| l == expected)
    }

    pub(crate) fn error(&self, reason: impl Into<String>) -> Error {
        Error::Malformed { what: self.what, reason: reason.into() }
    }

    fn truncated(&self, wanted: usize) -> Error {
        self.error(format!("truncated: {wanted} bytes wanted at offset {}, {} left", self.pos, self.remaining()))
    }
}

/// Appends `v` as `u32` BE; lengths above `u32::MAX` are an error.
pub(crate) fn put_len(out: &mut Vec<u8>, v: usize, what: &'static str) -> Result<()> {
    let v = u32::try_from(v).map_err(|_| Error::Limit { what, value: v as u64, limit: u64::from(u32::MAX) })?;
    out.extend_from_slice(&v.to_be_bytes());
    Ok(())
}

pub(crate) fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}

pub(crate) fn put_i32(out: &mut Vec<u8>, v: i32) {
    out.extend_from_slice(&v.to_be_bytes());
}

pub(crate) fn put_label(out: &mut Vec<u8>, label: &str) {
    let units: Vec<u16> = label.encode_utf16().collect();
    put_u32(out, u32::try_from(units.len()).unwrap_or(u32::MAX));
    for u in units {
        out.extend_from_slice(&u.to_be_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_round_trip_and_truncation_is_an_error() {
        let mut out = Vec::new();
        put_label(&mut out, "BlockStatus");
        let mut r = Reader::new(&out, "test");
        assert!(r.peek_label("BlockStatus"));
        r.expect_label("BlockStatus").unwrap();
        assert!(r.is_empty());
        for cut in 0..out.len() {
            assert!(Reader::new(&out[..cut], "test").label().is_err() || cut == 0 && out.is_empty());
        }
        let huge = [0xff, 0xff, 0xff, 0xff];
        assert!(Reader::new(&huge, "test").label().is_err());
    }
}
