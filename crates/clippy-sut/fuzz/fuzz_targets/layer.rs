#![no_main]

use libfuzzer_sys::fuzz_target;

// The .layer container, the clear-page SQLite reader and the tile decoder on arbitrary bytes.
// Seeds: `cargo run -p photocraft-clippy-sut --example export_tips` inputs, or `synth::tip` encoded.
fuzz_target!(|data: &[u8]| {
    let limits = photocraft_clippy_sut::Limits { max_tip_pixels: 1 << 22, max_entry_bytes: 16 << 20, ..Default::default() };
    if let Ok(tip) = photocraft_clippy_sut::layer::decode(data, &limits) {
        assert_eq!(tip.rgba.len(), tip.width as usize * tip.height as usize * 4);
    }
});
