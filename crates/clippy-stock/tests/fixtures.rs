//! Acceptance tests against a real Clip Studio Paint 1.5 installation (M7).
//!
//! Set `CLIPPY_CSP_FIXTURES` to a folder holding `CLIPStudioPaintVer1_5_0/Tool/EditImageTool.todb`,
//! `Material/` and `Sut-Exporte/*.sut` (as for `photocraft-clippy-sut`). Without it the tests pass
//! without checking anything. The files are private user data and never part of the repository.
#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};

use photocraft_clippy_stock::{EntryPath, Stock, import, ops};
use photocraft_clippy_sut::Sut;

fn fixtures() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("CLIPPY_CSP_FIXTURES")?);
    dir.is_dir().then_some(dir)
}

fn import_all(dir: &Path, root: &Path) -> import::ImportReport {
    std::fs::create_dir_all(root).unwrap();
    import::import_todb(root, &dir.join("CLIPStudioPaintVer1_5_0/Tool/EditImageTool.todb"), Some(&dir.join("Material"))).unwrap()
}

#[test]
fn the_owners_tool_database_imports_completely() {
    let Some(dir) = fixtures() else { return };
    let out = tempfile::tempdir().unwrap();
    let root = out.path().join("stock");
    let report = import_all(&dir, &root);
    eprintln!("{} materials embedded; missing: {:?}; warnings: {:?}", report.materials, report.missing_materials, report.warnings);
    assert_eq!((report.tools.len(), report.groups.len(), report.subtools.len()), (22, 38, 210));
    assert!(report.warnings.is_empty());
    let stock = Stock::load(&root).unwrap();
    assert_eq!(stock.tools.len(), 22);
    assert_eq!(stock.group_count(), 38);
    assert_eq!(stock.subtools().count(), 210);
    let broken: Vec<_> = stock.subtools().filter(|(_, e)| e.warning.is_some()).map(|(p, e)| format!("{p}: {:?}", e.warning)).collect();
    assert!(broken.is_empty(), "{broken:#?}");
    // Every sub tool keeps the database's settings.
    let db = photocraft_clippy_sut::Todb::open(&dir.join("CLIPStudioPaintVer1_5_0/Tool/EditImageTool.todb")).unwrap();
    for ((path, _), (_, _, original)) in stock.subtools().zip(db.tree.subtools()) {
        let sut = Sut::open(&path.to_path(&root)).unwrap();
        assert_eq!(sut.subtool.variant, original.variant, "{path}");
        assert_eq!(sut.subtool.init_variant, original.init_variant, "{path}");
    }
}

#[test]
fn exported_suts_import_byte_for_byte_and_survive_operations() {
    let Some(dir) = fixtures() else { return };
    let out = tempfile::tempdir().unwrap();
    let root = out.path().join("stock");
    std::fs::create_dir_all(&root).unwrap();
    ops::create_tool(&root, "Imported").unwrap();
    ops::create_group(&root, "Imported", "Sut").unwrap();
    let mut files: Vec<_> =
        std::fs::read_dir(dir.join("Sut-Exporte")).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "sut")).collect();
    files.sort();
    assert!(!files.is_empty());
    for file in &files {
        let path = import::import_sut(&root, file, "Imported", "Sut").unwrap();
        assert_eq!(std::fs::read(path.to_path(&root)).unwrap(), std::fs::read(file).unwrap(), "{}", file.display());
        // Renamed and copied, the settings and materials stay as they were (P8a).
        let original = Sut::open(file).unwrap();
        let renamed = ops::rename(&root, &path, &format!("{} renamed", path.name())).unwrap();
        let copy = ops::copy_entry(&root, &renamed, Some(&EntryPath::Group("Imported".into(), "Sut".into())), None).unwrap();
        for p in [&renamed, &copy] {
            let sut = Sut::open(&p.to_path(&root)).unwrap();
            assert_eq!(sut.subtool.variant, original.subtool.variant, "{p}");
            assert_eq!(sut.subtool.init_variant, original.subtool.init_variant, "{p}");
            assert_eq!(sut.subtool.materials, original.subtool.materials, "{p}");
        }
    }
}
