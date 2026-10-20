//! Generated files: round trips, the tree guards, and hostile input (every truncation, byte flips,
//! arbitrary blobs) never panicking.
#![cfg(not(target_arch = "wasm32"))]

use photocraft_clippy_sut::{Effector, Error, Limits, MaterialRefs, ShowParams, Sut, Todb, Value, known, layer, synth};
use proptest::prelude::*;

#[test]
fn generated_suts_round_trip_with_materials() {
    for (seed, materials) in [(1, 0), (2, 1), (3, 3)] {
        let sut = synth::sut(seed, materials).unwrap();
        let bytes = sut.to_bytes().unwrap();
        let back = Sut::from_bytes(&bytes, &Limits::default()).unwrap();
        assert_eq!(back.subtool, sut.subtool);
        assert!(back.info.manager.same_values(&sut.info.manager));
        assert_eq!(back.info.sequence, vec![("Manager".into(), 1), ("Node".into(), 1), ("Variant".into(), 2), ("MaterialFile".into(), materials as i64)].into_iter().filter(|(_, n)| *n > 0).collect::<Vec<_>>());
        assert_eq!(back.to_bytes().unwrap(), bytes);
        let v = back.subtool.variant.as_ref().unwrap();
        assert!(v.effector(known::BRUSH_SIZE_EFFECTOR).unwrap().is_ok());
        assert_eq!(v.show_params().unwrap().unwrap().ids, [2000, 2010, 3231]);
        assert_eq!(v.all_material_refs().len(), materials);
        for m in &back.subtool.materials {
            let tip = layer::decode(m.archive(&Limits::default()).unwrap().tip_layer().unwrap(), &Limits::default()).unwrap();
            assert!(tip.width >= 24);
            assert!(v.all_material_refs().iter().any(|r| r.uuid() == m.uuid()));
        }
    }
}

#[test]
fn saving_is_atomic_and_readable() {
    let dir = std::env::temp_dir().join(format!("clippy-sut-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("pen.sut");
    let sut = synth::sut(9, 1).unwrap();
    sut.save(&path).unwrap();
    sut.save(&path).unwrap();
    assert_eq!(Sut::open(&path).unwrap().subtool, sut.subtool);
    let names: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name()).collect();
    assert_eq!(names, ["pen.sut"]);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn unknown_template_columns_are_refused() {
    let mut sut = synth::sut(4, 0).unwrap();
    sut.subtool.variant.as_mut().unwrap().set("NotInTheTemplate", Value::Integer(1));
    assert!(matches!(sut.to_bytes(), Err(Error::UnknownColumn { .. })));
}

#[test]
fn generated_tool_databases_build_their_tree() {
    let bytes = synth::todb(&[&[2, 1], &[3]], &[]).unwrap();
    let db = Todb::from_bytes(&bytes, &Limits::default()).unwrap();
    assert_eq!(db.info.encoding, "UTF-16le");
    assert_eq!(db.tree.tools.len(), 2);
    assert_eq!(db.tree.group_count(), 3);
    let names: Vec<_> = db.tree.subtools().map(|(_, _, s)| s.name().to_owned()).collect();
    assert_eq!(names, ["Sub 0.0.0", "Sub 0.0.1", "Sub 0.1.0", "Sub 1.0.0", "Sub 1.0.1", "Sub 1.0.2"]);
    for sut in db.subtool_suts() {
        let back = Sut::from_bytes(&sut.to_bytes().unwrap(), &Limits::default()).unwrap();
        assert_eq!(back.subtool.variant, sut.subtool.variant);
    }
}

#[test]
fn broken_trees_are_errors() {
    // Node order: 0 root, 1 tool, 2 group, 3 and 4 sub tools.
    for links in [
        vec![(4, "NodeNextUuid", Some(3))],      // sibling cycle
        vec![(3, "NodeFirstChildUuid", Some(1))], // child points back up
        vec![(2, "NodeFirstChildUuid", Some(99))], // dangling link
        vec![(3, "NodeFirstChildUuid", Some(4))], // a fourth level
    ] {
        let bytes = synth::todb(&[&[2]], &links).unwrap();
        assert!(Todb::from_bytes(&bytes, &Limits::default()).is_err(), "{links:?}");
    }
}

#[test]
fn every_truncated_sut_is_an_error() {
    let bytes = synth::sut(5, 1).unwrap().to_bytes().unwrap();
    for cut in 0..bytes.len() {
        assert!(Sut::from_bytes(&bytes[..cut], &Limits::default()).is_err(), "cut {cut}");
    }
}

#[test]
fn every_truncated_layer_is_an_error() {
    let bytes = layer::encode(&synth::tip(40, 7)).unwrap();
    for cut in 0..bytes.len() {
        assert!(layer::decode(&bytes[..cut], &Limits::default()).is_err(), "cut {cut}");
    }
}

#[test]
fn flipped_bytes_never_panic() {
    let sut = synth::sut(6, 1).unwrap().to_bytes().unwrap();
    let tip = layer::encode(&synth::tip(300, 8)).unwrap();
    let todb = synth::todb(&[&[2, 2]], &[]).unwrap();
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    for _ in 0..1500 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        for file in [&sut, &tip, &todb] {
            let mut bad = file.clone();
            let i = (state % bad.len() as u64) as usize;
            bad[i] ^= (state >> 32) as u8 | 1;
            let _ = Sut::from_bytes(&bad, &Limits::default()).map(|s| s.to_bytes());
            let _ = Todb::from_bytes(&bad, &Limits::default());
            let _ = layer::decode(&bad, &Limits::default());
        }
    }
}

#[test]
fn small_limits_are_enforced() {
    let tip = layer::encode(&synth::tip(300, 8)).unwrap();
    let limits = Limits { max_tip_pixels: 1000, ..Limits::default() };
    assert!(matches!(layer::decode(&tip, &limits), Err(Error::Limit { .. })));
    let todb = synth::todb(&[&[2, 2]], &[]).unwrap();
    let limits = Limits { max_nodes: 3, ..Limits::default() };
    assert!(matches!(Todb::from_bytes(&todb, &limits), Err(Error::Limit { .. })));
}

proptest! {
    #[test]
    fn arbitrary_blobs_decode_to_errors_or_round_trip(bytes in proptest::collection::vec(any::<u8>(), 0..400)) {
        if let Ok(e) = Effector::decode(&bytes) {
            prop_assert_eq!(e.encode(), bytes.clone());
        }
        if let Ok(p) = ShowParams::decode(&bytes) {
            prop_assert_eq!(p.encode(), bytes.clone());
        }
        if let Ok(r) = MaterialRefs::decode(&bytes) {
            let _ = r.encode();
        }
        let _ = layer::decode(&bytes, &Limits::default());
    }

    #[test]
    fn plausible_effectors_round_trip(slots in proptest::collection::vec(any::<i32>(), 9..11), points in proptest::collection::vec((0.0f64..1.0, 0.0f64..1.0), 0..8)) {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&((slots.len() as i32 + 1) * 4).to_be_bytes());
        let curve_len = if points.is_empty() { 0 } else { 12 + 16 * points.len() as i32 };
        for (i, s) in slots.iter().enumerate() {
            let v = match i { 7 => curve_len, 8 => 0, _ => *s };
            bytes.extend_from_slice(&v.to_be_bytes());
        }
        if !points.is_empty() {
            for v in [12, points.len() as i32, 16] {
                bytes.extend_from_slice(&v.to_be_bytes());
            }
            for (x, y) in &points {
                bytes.extend_from_slice(&x.to_be_bytes());
                bytes.extend_from_slice(&y.to_be_bytes());
            }
        }
        let e = Effector::decode(&bytes).unwrap();
        prop_assert_eq!(e.pressure_curve().map(|c| c.points.len()).unwrap_or(0), points.len());
        prop_assert_eq!(e.encode(), bytes);
    }
}
