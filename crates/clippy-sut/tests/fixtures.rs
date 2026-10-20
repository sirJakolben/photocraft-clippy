//! Acceptance tests against a real Clip Studio Paint 1.5 installation (M6).
//!
//! Set `CLIPPY_CSP_FIXTURES` to a folder holding `CLIPStudioPaintVer1_5_0/Tool/EditImageTool.todb`,
//! `Material/` and `Sut-Exporte/*.sut`. Without it the tests pass without checking anything. The
//! files are private user data and never part of the repository.
#![cfg(not(target_arch = "wasm32"))]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use photocraft_clippy_sut::{Effector, Limits, MaterialRefs, ShowParams, SubTool, Sut, Todb, Value, known, layer};

fn fixtures() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("CLIPPY_CSP_FIXTURES")?);
    dir.is_dir().then_some(dir)
}

fn todb(dir: &Path) -> Todb {
    Todb::open(&dir.join("CLIPStudioPaintVer1_5_0/Tool/EditImageTool.todb")).unwrap()
}

fn suts(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir.join("Sut-Exporte"))
        .map(|d| d.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "sut")).collect())
        .unwrap_or_default();
    out.sort();
    out
}

fn layers(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            layers(&path, out);
        } else if path.extension().is_some_and(|e| e == "layer") {
            out.push(path);
        }
    }
}

/// Compares two sub tools column by column; tree links of the node are left out (a `.sut` has
/// none).
fn assert_same(a: &SubTool, b: &SubTool, what: &str) {
    let mut na = a.node.clone();
    let mut nb = b.node.clone();
    for link in ["NodeNextUuid", "NodeFirstChildUuid"] {
        na.remove(link);
        nb.remove(link);
    }
    assert!(na.same_values(&nb), "{what}: node differs");
    for (va, vb) in [(&a.variant, &b.variant), (&a.init_variant, &b.init_variant)] {
        let (va, vb) = (va.as_ref().unwrap(), vb.as_ref().unwrap());
        assert_eq!(va.row.len(), vb.row.len(), "{what}: column count");
        for cell in va.row.cells() {
            let other = vb.get(&cell.name).unwrap_or_else(|| panic!("{what}: {} missing", cell.name));
            assert!(cell.value.same(other), "{what}: {} differs", cell.name);
        }
    }
    assert_eq!(a.materials, b.materials, "{what}: materials differ");
}

#[test]
fn the_tool_database_reads_completely() {
    let Some(dir) = fixtures() else { return };
    let db = todb(&dir);
    assert_eq!(db.tree.tools.len(), 22);
    assert_eq!(db.tree.group_count(), 38);
    assert_eq!(db.tree.subtools().count(), 210);
    assert_eq!(db.info.version(), Some(138));
    for (tool, group, sub) in db.tree.subtools() {
        assert!(sub.variant.is_some() && sub.init_variant.is_some(), "{}/{}/{}", tool.name(), group.name(), sub.name());
    }
}

#[test]
fn every_known_blob_decodes_and_encodes_to_the_same_bytes() {
    let Some(dir) = fixtures() else { return };
    let db = todb(&dir);
    let mut counts = BTreeMap::new();
    let mut variants: Vec<_> = db.tree.subtools().flat_map(|(_, _, s)| [s.variant.clone(), s.init_variant.clone()]).flatten().collect();
    for path in suts(&dir) {
        let sut = Sut::open(&path).unwrap();
        variants.extend([sut.subtool.variant, sut.subtool.init_variant].into_iter().flatten());
    }
    for v in &variants {
        for cell in v.row.cells() {
            let Value::Blob(bytes) = &cell.value else { continue };
            match known::kind(&cell.name, &cell.decl_type) {
                known::Kind::Effector => {
                    assert_eq!(Effector::decode(bytes).unwrap().encode(), *bytes, "{}", cell.name);
                    *counts.entry("effector").or_insert(0) += 1;
                }
                known::Kind::ShowParams => {
                    assert_eq!(ShowParams::decode(bytes).unwrap().encode(), *bytes);
                    *counts.entry("show params").or_insert(0) += 1;
                }
                known::Kind::MaterialRefs => {
                    let refs = MaterialRefs::decode(bytes).unwrap();
                    assert_eq!(refs.encode().unwrap(), *bytes, "{}", cell.name);
                    assert!(refs.entries.iter().all(|e| e.uuid().is_some()));
                    *counts.entry("material refs").or_insert(0) += refs.entries.len();
                }
                _ => {}
            }
        }
    }
    eprintln!("decoded and re-encoded: {counts:?}");
    assert!(counts["effector"] > 5000);
}

#[test]
fn every_sub_tool_survives_a_sut_round_trip() {
    let Some(dir) = fixtures() else { return };
    let db = todb(&dir);
    let mut n = 0;
    for sut in db.subtool_suts() {
        let bytes = sut.to_bytes().unwrap();
        let back = Sut::from_bytes(&bytes, &Limits::default()).unwrap();
        assert_same(&sut.subtool, &back.subtool, sut.subtool.name());
        assert_eq!(back.info.encoding, "UTF-8");
        // Written once more, the file is byte-identical: the writer is deterministic.
        assert_eq!(back.to_bytes().unwrap(), bytes, "{}", sut.subtool.name());
        n += 1;
    }
    assert_eq!(n, 210);
}

#[test]
fn exported_suts_round_trip_with_their_schema_and_materials() {
    let Some(dir) = fixtures() else { return };
    let paths = suts(&dir);
    assert!(!paths.is_empty());
    for path in paths {
        let sut = Sut::open(&path).unwrap();
        let back = Sut::from_bytes(&sut.to_bytes().unwrap(), &Limits::default()).unwrap();
        assert_same(&sut.subtool, &back.subtool, &path.display().to_string());
        assert!(sut.info.manager.same_values(&back.info.manager));
        assert_eq!(sut.info.sequence, back.info.sequence);
        assert_eq!(sut.info.page_size, back.info.page_size);
        let sql = |s: &Sut| s.info.objects.iter().map(|o| o.sql.clone()).collect::<Vec<_>>();
        assert_eq!(sql(&sut), sql(&back));
        assert_eq!(sut.info.columns, back.info.columns);
    }
}

#[test]
fn every_brush_tip_decodes() {
    let Some(dir) = fixtures() else { return };
    let mut paths = Vec::new();
    layers(&dir.join("Material"), &mut paths);
    let mut tips = Vec::new();
    for path in &paths {
        let bytes = std::fs::read(path).unwrap();
        tips.push(layer::decode(&bytes, &Limits::default()).unwrap_or_else(|e| panic!("{}: {e}", path.display())));
    }
    for path in suts(&dir) {
        for m in Sut::open(&path).unwrap().subtool.materials {
            let archive = m.archive(&Limits::default()).unwrap();
            tips.push(layer::decode(archive.tip_layer().unwrap(), &Limits::default()).unwrap());
        }
    }
    let mut formats = BTreeMap::new();
    for t in &tips {
        assert_eq!(t.rgba.len(), t.width as usize * t.height as usize * 4);
        assert!(t.rgba.chunks_exact(4).any(|p| p[3] != 0), "a tip is fully transparent");
        *formats.entry(format!("{:?}", t.format)).or_insert(0) += 1;
    }
    eprintln!("{} tips: {formats:?}", tips.len());
    assert_eq!(tips.len(), 177);
    // Re-encoding a real tip and decoding it again gives the same pixels.
    for t in tips.iter().step_by(11) {
        let back = layer::decode(&layer::encode(t).unwrap(), &Limits::default()).unwrap();
        assert_eq!(back.rgba, t.rgba);
    }
}
