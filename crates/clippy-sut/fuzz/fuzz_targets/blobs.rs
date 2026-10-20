#![no_main]

use libfuzzer_sys::fuzz_target;
use photocraft_clippy_sut::{Effector, MaterialRefs, ShowParams};

// Variant blob codecs: whatever decodes must encode back to the same bytes.
fuzz_target!(|data: &[u8]| {
    if let Ok(e) = Effector::decode(data) {
        assert_eq!(e.encode(), data);
    }
    if let Ok(p) = ShowParams::decode(data) {
        assert_eq!(p.encode(), data);
    }
    if let Ok(r) = MaterialRefs::decode(data) {
        assert_eq!(r.encode().ok().as_deref(), Some(data));
    }
});
