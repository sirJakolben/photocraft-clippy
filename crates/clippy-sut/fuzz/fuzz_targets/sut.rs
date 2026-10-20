#![no_main]

use libfuzzer_sys::fuzz_target;
use photocraft_clippy_sut::{Limits, Sut, Todb};

// Whole tool databases (SQLite images) as .sut and .todb, and writing back what was read.
fuzz_target!(|data: &[u8]| {
    let limits = Limits::default();
    if let Ok(sut) = Sut::from_bytes(data, &limits) {
        let _ = sut.to_bytes();
    }
    let _ = Todb::from_bytes(data, &limits);
});
