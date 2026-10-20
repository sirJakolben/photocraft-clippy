//! The stock on disk: loading, every operation with its error cases, import, export, watching.
//! All files are generated (`photocraft_clippy_sut::synth`); no Clip Studio Paint data.
#![cfg(not(target_arch = "wasm32"))]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use photocraft_clippy_stock::import::{self, MaterialIndex};
use photocraft_clippy_stock::live::LiveStock;
use photocraft_clippy_stock::ops::{self, MetaChange};
use photocraft_clippy_stock::trash::Trash;
use photocraft_clippy_stock::{EntryPath, Error, FolderMeta, Stock, export, meta, stored_preset};
use photocraft_clippy_sut::{Limits, MaterialRefs, Sut, known, layer, synth};
use photocraft_paint::{BrushPreset, BrushSettings, GrayTile, TipShape};

fn p(s: &str) -> EntryPath {
    EntryPath::parse(s).unwrap()
}

fn write_sut(path: &Path, seed: u64, name: &str) {
    let mut sut = synth::sut(seed, 1).unwrap();
    sut.subtool.set_name(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    sut.save(path).unwrap();
}

/// `root/Pen/{Glatt/{G-Pen, Fineliner}, Rau/{Kreide}}`, `root/Brush/Wasser/{Nass}`.
fn stock() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("stock");
    write_sut(&root.join("Pen/Glatt/G-Pen.sut"), 1, "G-Pen");
    write_sut(&root.join("Pen/Glatt/Fineliner.sut"), 2, "Fineliner");
    write_sut(&root.join("Pen/Rau/Kreide.sut"), 3, "Kreide");
    write_sut(&root.join("Brush/Wasser/Nass.sut"), 4, "Nass");
    (dir, root)
}

fn names(stock: &Stock) -> Vec<String> {
    stock.subtools().map(|(p, _)| p.to_string()).collect()
}

fn sut_at(root: &Path, path: &str) -> Sut {
    Sut::open(&p(path).to_path(root)).unwrap()
}

fn trash(dir: &tempfile::TempDir) -> Trash {
    Trash::Freedesktop(dir.path().join("Trash"))
}

/// Whether permissions bind this process (not root): read-only tests check nothing otherwise.
fn permissions_apply(dir: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let probe = dir.join("probe");
        fs::create_dir(&probe).unwrap();
        fs::set_permissions(&probe, fs::Permissions::from_mode(0o555)).unwrap();
        let applies = fs::write(probe.join("x"), b"x").is_err();
        fs::set_permissions(&probe, fs::Permissions::from_mode(0o755)).unwrap();
        fs::remove_dir_all(&probe).unwrap();
        applies
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
        false
    }
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

#[test]
fn loads_folders_alphabetically_without_metadata() {
    let (_d, root) = stock();
    let s = Stock::load(&root).unwrap();
    assert_eq!(names(&s), ["Brush/Wasser/Nass", "Pen/Glatt/Fineliner", "Pen/Glatt/G-Pen", "Pen/Rau/Kreide"]);
    assert_eq!(s.tools[1].name, "Pen");
    assert!(s.warnings.is_empty(), "{:?}", s.warnings);
    let pen = &s.tools[1].groups[0].subtools[1];
    assert_eq!((pen.name.as_str(), pen.file.as_str(), pen.node_name.as_str()), ("G-Pen", "G-Pen.sut", "G-Pen"));
    assert_eq!(pen.uuid.len(), 32);
    assert!(pen.warning.is_none());
}

#[test]
fn metadata_names_orders_and_ignores_unknown_entries() {
    let (_d, root) = stock();
    meta::write(&root, &FolderMeta { order: vec!["Pen".into(), "Gone".into()], ..Default::default() }).unwrap();
    meta::write(&root.join("Pen"), &FolderMeta { name: Some("Stift".into()), icon: Some("csp:110".into()), ..Default::default() }).unwrap();
    meta::write(
        &root.join("Pen/Glatt"),
        &FolderMeta { order: vec!["G-Pen.sut".into()], shortcuts: [("G-Pen.sut".to_owned(), "P".to_owned())].into(), ..Default::default() },
    )
    .unwrap();
    // Things the stock doesn't show.
    fs::write(root.join("Pen/Glatt/readme.txt"), b"x").unwrap();
    fs::write(root.join("Pen/Glatt/.G-Pen.sut.tmp1"), b"x").unwrap();
    fs::create_dir_all(root.join(".hidden/Group")).unwrap();
    fs::write(root.join("loose.sut"), b"x").unwrap();
    fs::create_dir_all(root.join("Pen/Glatt/folder.sut")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.join("Pen"), root.join("Link")).unwrap();
    let s = Stock::load(&root).unwrap();
    assert_eq!(names(&s), ["Pen/Glatt/G-Pen", "Pen/Glatt/Fineliner", "Pen/Rau/Kreide", "Brush/Wasser/Nass"]);
    let pen = s.tool("Pen").unwrap();
    assert_eq!((pen.name.as_str(), pen.icon.as_deref()), ("Stift", Some("csp:110")));
    assert_eq!(pen.groups[0].subtools[0].shortcut.as_deref(), Some("P"));
    assert!(s.tool("Brush").unwrap().icon.is_none());
    #[cfg(unix)]
    assert!(s.warnings.iter().any(|w| w.contains("symbolic link")), "{:?}", s.warnings);
}

#[test]
fn broken_files_are_entries_with_warnings() {
    let (_d, root) = stock();
    fs::write(root.join("Pen/Glatt/Kaputt.sut"), b"SQLite format 3\0 but not really").unwrap();
    fs::write(root.join("Pen/Glatt/Leer.sut"), b"").unwrap();
    fs::write(root.join("Pen/_clippy.json"), b"{ not json").unwrap();
    fs::write(root.join("Brush/_clippy.json"), vec![b' '; 2 << 20]).unwrap();
    let s = Stock::load(&root).unwrap();
    let glatt = &s.tool("Pen").unwrap().groups[0];
    let broken: Vec<_> = glatt.subtools.iter().filter(|e| e.warning.is_some()).map(|e| e.name.as_str()).collect();
    assert_eq!(broken, ["Kaputt", "Leer"]);
    assert!(glatt.subtools.iter().any(|e| e.name == "G-Pen" && e.warning.is_none()));
    assert!(s.tool("Pen").unwrap().warning.is_some());
    assert!(s.tool("Brush").unwrap().warning.is_some());
    assert_eq!(s.warnings.len(), 4, "{:?}", s.warnings);
    // A broken metadata file is kept aside when Clippy writes a new one.
    ops::sort(&root, Some(&p("Pen")), Some(&["Rau".into()])).unwrap();
    assert_eq!(fs::read(root.join("Pen").join(meta::BROKEN_FILE)).unwrap(), b"{ not json");
    assert_eq!(meta::read(&root.join("Pen")).0.order, ["Rau"]);
    assert!(Stock::load(&root.join("missing")).is_err());
    assert!(Stock::load(&root.join("Pen/Glatt/G-Pen.sut")).is_err());
}

#[test]
fn tools_and_groups_are_created_with_numbers_on_clashes() {
    let (_d, root) = stock();
    assert_eq!(ops::create_tool(&root, "Eraser").unwrap(), p("Eraser"));
    assert_eq!(ops::create_tool(&root, "pen").unwrap(), p("pen 2"));
    assert_eq!(ops::create_group(&root, "Pen", "Glatt").unwrap(), p("Pen/Glatt 2"));
    assert_eq!(meta::read(&root).0.order.last().map(String::as_str), Some("pen 2"));
    let s = Stock::load(&root).unwrap();
    assert_eq!(s.tools.last().unwrap().dir, "pen 2", "new entries come last");
    for bad in ["", "..", "../x", "a/b", ".x", "CON", "x\0"] {
        assert!(matches!(ops::create_tool(&root, bad), Err(Error::BadName { .. })), "{bad:?}");
        assert!(ops::create_group(&root, "Pen", bad).is_err(), "{bad:?}");
    }
    assert!(matches!(ops::create_group(&root, "Missing", "G"), Err(Error::NotFound(_))));
    assert!(ops::create_tool(&root.join("missing"), "T").is_err());
    assert!(!root.join("..").join("x").exists());
}

#[test]
fn renaming_a_sub_tool_renames_the_file_and_the_name_inside_only() {
    let (_d, root) = stock();
    meta::write(
        &root.join("Pen/Glatt"),
        &FolderMeta { order: vec!["G-Pen.sut".into(), "Fineliner.sut".into()], shortcuts: [("G-Pen.sut".into(), "P".into())].into(), ..Default::default() },
    )
    .unwrap();
    let before = sut_at(&root, "Pen/Glatt/G-Pen");
    let new = ops::rename(&root, &p("Pen/Glatt/G-Pen"), "Zeichenstift ✒").unwrap();
    assert_eq!(new, p("Pen/Glatt/Zeichenstift ✒"));
    assert!(!root.join("Pen/Glatt/G-Pen.sut").exists());
    let after = sut_at(&root, "Pen/Glatt/Zeichenstift ✒");
    assert_eq!(after.subtool.name(), "Zeichenstift ✒");
    // P8a: everything else unchanged.
    let mut node = after.subtool.node.clone();
    node.set("NodeName", photocraft_clippy_sut::Value::Text("G-Pen".into()));
    assert!(node.same_values(&before.subtool.node));
    assert_eq!(after.subtool.variant, before.subtool.variant);
    assert_eq!(after.subtool.init_variant, before.subtool.init_variant);
    assert_eq!(after.subtool.materials, before.subtool.materials);
    let m = meta::read(&root.join("Pen/Glatt")).0;
    assert_eq!(m.order, ["Zeichenstift ✒.sut", "Fineliner.sut"]);
    assert_eq!(m.shortcuts.get("Zeichenstift ✒.sut").map(String::as_str), Some("P"));
    // Only the case changes: allowed.
    assert_eq!(ops::rename(&root, &p("Pen/Glatt/Fineliner"), "fineliner").unwrap(), p("Pen/Glatt/fineliner"));
}

#[test]
fn rename_errors_change_nothing() {
    let (_d, root) = stock();
    for bad in ["", "..", "../../evil", "a/b", ".hidden", "x:y", "_clippy.json"] {
        assert!(matches!(ops::rename(&root, &p("Pen/Glatt/G-Pen"), bad), Err(Error::BadName { .. })), "{bad:?}");
    }
    assert!(matches!(ops::rename(&root, &p("Pen/Glatt/G-Pen"), "FINELINER"), Err(Error::Exists(_))));
    assert!(matches!(ops::rename(&root, &p("Pen/Glatt"), "rau"), Err(Error::Exists(_))));
    assert!(matches!(ops::rename(&root, &p("Pen/Glatt/Missing"), "x"), Err(Error::NotFound(_))));
    assert!(matches!(ops::rename(&root, &p("Missing"), "x"), Err(Error::NotFound(_))));
    let s = Stock::load(&root).unwrap();
    assert_eq!(names(&s), ["Brush/Wasser/Nass", "Pen/Glatt/Fineliner", "Pen/Glatt/G-Pen", "Pen/Rau/Kreide"]);
}

#[test]
fn renaming_folders_keeps_their_place_and_drops_the_old_display_name() {
    let (_d, root) = stock();
    meta::write(&root, &FolderMeta { order: vec!["Pen".into(), "Brush".into()], ..Default::default() }).unwrap();
    meta::write(&root.join("Pen"), &FolderMeta { name: Some("Old name".into()), icon: Some("csp:1".into()), ..Default::default() }).unwrap();
    assert_eq!(ops::rename(&root, &p("Pen"), "Stift").unwrap(), p("Stift"));
    assert_eq!(ops::rename(&root, &p("Stift/Rau"), "Rough").unwrap(), p("Stift/Rough"));
    let s = Stock::load(&root).unwrap();
    assert_eq!(s.tools[0].dir, "Stift");
    assert_eq!(s.tools[0].name, "Stift");
    assert_eq!(s.tools[0].icon.as_deref(), Some("csp:1"));
    assert!(s.contains(&p("Stift/Rough/Kreide")));
}

#[test]
#[cfg(unix)]
fn a_read_only_sub_tool_can_be_renamed_but_a_read_only_folder_refuses() {
    let (d, root) = stock();
    let file = root.join("Pen/Glatt/G-Pen.sut");
    set_mode(&file, 0o444);
    ops::rename(&root, &p("Pen/Glatt/G-Pen"), "Stift").unwrap();
    assert_eq!(sut_at(&root, "Pen/Glatt/Stift").subtool.name(), "Stift");
    if permissions_apply(d.path()) {
        set_mode(&root.join("Pen/Rau"), 0o555);
        assert!(matches!(ops::move_entry(&root, &p("Pen/Glatt/Stift"), Some(&p("Pen/Rau")), None), Err(Error::Io { .. })));
        assert!(root.join("Pen/Glatt/Stift.sut").exists(), "the source stays");
        assert!(ops::rename(&root, &p("Pen/Rau/Kreide"), "x").is_err());
        assert!(ops::delete(&root, &p("Pen/Rau/Kreide"), &trash(&d)).is_err());
        assert!(root.join("Pen/Rau/Kreide.sut").exists());
        set_mode(&root.join("Pen/Rau"), 0o755);
    }
}

#[test]
fn deleting_goes_to_the_trash() {
    let (d, root) = stock();
    meta::write(
        &root.join("Pen/Glatt"),
        &FolderMeta { order: vec!["G-Pen.sut".into(), "Fineliner.sut".into()], shortcuts: [("G-Pen.sut".into(), "P".into())].into(), ..Default::default() },
    )
    .unwrap();
    let bytes = fs::read(root.join("Pen/Glatt/G-Pen.sut")).unwrap();
    ops::delete(&root, &p("Pen/Glatt/G-Pen"), &trash(&d)).unwrap();
    assert_eq!(fs::read(d.path().join("Trash/files/G-Pen.sut")).unwrap(), bytes);
    assert!(d.path().join("Trash/info/G-Pen.sut.trashinfo").is_file());
    let m = meta::read(&root.join("Pen/Glatt")).0;
    assert_eq!(m.order, ["Fineliner.sut"]);
    assert!(m.shortcuts.is_empty());
    ops::delete(&root, &p("Pen/Rau"), &trash(&d)).unwrap();
    ops::delete(&root, &p("Brush"), &trash(&d)).unwrap();
    assert!(d.path().join("Trash/files/Brush/Wasser/Nass.sut").is_file());
    assert_eq!(names(&Stock::load(&root).unwrap()), ["Pen/Glatt/Fineliner"]);
    assert!(matches!(ops::delete(&root, &p("Brush"), &trash(&d)), Err(Error::NotFound(_))));
}

#[test]
fn moving_a_sub_tool_keeps_its_file_and_numbers_clashes() {
    let (_d, root) = stock();
    meta::write(&root.join("Pen/Glatt"), &FolderMeta { shortcuts: [("G-Pen.sut".into(), "P".into())].into(), ..Default::default() }).unwrap();
    let bytes = fs::read(root.join("Pen/Glatt/G-Pen.sut")).unwrap();
    let moved = ops::move_entry(&root, &p("Pen/Glatt/G-Pen"), Some(&p("Pen/Rau")), Some(0)).unwrap();
    assert_eq!(moved, p("Pen/Rau/G-Pen"));
    assert_eq!(fs::read(moved.to_path(&root)).unwrap(), bytes, "a move doesn't touch the file");
    let rau = meta::read(&root.join("Pen/Rau")).0;
    assert_eq!(rau.order, ["G-Pen.sut", "Kreide.sut"]);
    assert_eq!(rau.shortcuts.get("G-Pen.sut").map(String::as_str), Some("P"), "the shortcut moves along");
    assert!(meta::read(&root.join("Pen/Glatt")).0.shortcuts.is_empty());
    // A clash: "Name 2", and the name inside follows the file.
    write_sut(&root.join("Pen/Glatt/G-Pen.sut"), 9, "G-Pen");
    let second = ops::move_entry(&root, &p("Pen/Glatt/G-Pen"), Some(&p("Pen/Rau")), None).unwrap();
    assert_eq!(second, p("Pen/Rau/G-Pen 2"));
    assert_eq!(sut_at(&root, "Pen/Rau/G-Pen 2").subtool.name(), "G-Pen 2");
    let s = Stock::load(&root).unwrap();
    assert_eq!(s.tool("Pen").unwrap().groups[1].subtools.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["G-Pen", "Kreide", "G-Pen 2"]);
    // Within its group: a reorder.
    assert_eq!(ops::move_entry(&root, &p("Pen/Rau/G-Pen 2"), Some(&p("Pen/Rau")), Some(0)).unwrap(), p("Pen/Rau/G-Pen 2"));
    assert_eq!(meta::read(&root.join("Pen/Rau")).0.order, ["G-Pen 2.sut", "G-Pen.sut", "Kreide.sut"]);
    // Past the end is the end.
    ops::move_entry(&root, &p("Pen/Rau/G-Pen 2"), Some(&p("Pen/Rau")), Some(99)).unwrap();
    assert_eq!(meta::read(&root.join("Pen/Rau")).0.order.last().map(String::as_str), Some("G-Pen 2.sut"));
}

#[test]
fn groups_move_between_tools_and_tools_reorder() {
    let (_d, root) = stock();
    assert_eq!(ops::move_entry(&root, &p("Pen/Rau"), Some(&p("Brush")), None).unwrap(), p("Brush/Rau"));
    assert!(root.join("Brush/Rau/Kreide.sut").is_file());
    assert_eq!(ops::move_entry(&root, &p("Pen"), None, Some(0)).unwrap(), p("Pen"));
    let s = Stock::load(&root).unwrap();
    assert_eq!(names(&s), ["Pen/Glatt/Fineliner", "Pen/Glatt/G-Pen", "Brush/Wasser/Nass", "Brush/Rau/Kreide"], "a moved group comes last");
}

#[test]
fn moves_and_copies_into_the_wrong_kind_of_folder_are_errors() {
    let (_d, root) = stock();
    let cases: [(&str, Option<&str>); 6] = [
        ("Pen/Glatt/G-Pen", Some("Pen")),
        ("Pen/Glatt/G-Pen", None),
        ("Pen/Glatt", Some("Pen/Rau")),
        ("Pen/Glatt", None),
        ("Pen", Some("Brush")),
        ("Pen/Glatt/G-Pen", Some("Pen/Glatt/Fineliner")),
    ];
    for (from, to) in cases {
        let to = to.map(p);
        assert!(matches!(ops::move_entry(&root, &p(from), to.as_ref(), None), Err(Error::Invalid(_))), "{from} → {to:?}");
        assert!(matches!(ops::copy_entry(&root, &p(from), to.as_ref(), None), Err(Error::Invalid(_))), "{from} → {to:?}");
    }
    assert!(matches!(ops::move_entry(&root, &p("Pen/Glatt/G-Pen"), Some(&p("Pen/Missing")), None), Err(Error::NotFound(_))));
    assert!(matches!(ops::move_entry(&root, &p("Pen/Glatt/Missing"), Some(&p("Pen/Rau")), None), Err(Error::NotFound(_))));
    assert!(matches!(ops::copy_entry(&root, &p("Pen/Glatt/Missing"), Some(&p("Pen/Rau")), None), Err(Error::NotFound(_))));
    assert_eq!(names(&Stock::load(&root).unwrap()).len(), 4);
}

#[test]
fn a_copied_sub_tool_is_independent() {
    let (_d, root) = stock();
    let copy = ops::copy_entry(&root, &p("Pen/Glatt/G-Pen"), Some(&p("Pen/Glatt")), None).unwrap();
    assert_eq!(copy, p("Pen/Glatt/G-Pen 2"));
    let original = sut_at(&root, "Pen/Glatt/G-Pen");
    let copied = sut_at(&root, "Pen/Glatt/G-Pen 2");
    assert_ne!(original.subtool.uuid(), copied.subtool.uuid(), "a copy has its own node UUID");
    assert_eq!(copied.subtool.name(), "G-Pen 2");
    assert_eq!(copied.subtool.variant, original.subtool.variant);
    assert_eq!(copied.subtool.materials, original.subtool.materials);
    // Changing the copy leaves the original alone.
    let mut changed = copied.clone();
    changed.subtool.variant.as_mut().unwrap().set(known::BRUSH_SIZE, photocraft_clippy_sut::Value::Real(123.0));
    changed.save(&copy.to_path(&root)).unwrap();
    assert_eq!(sut_at(&root, "Pen/Glatt/G-Pen").subtool.variant, original.subtool.variant);
    // Into another group under its own name, at the front.
    assert_eq!(ops::copy_entry(&root, &p("Pen/Glatt/G-Pen"), Some(&p("Brush/Wasser")), Some(0)).unwrap(), p("Brush/Wasser/G-Pen"));
    assert_eq!(sut_at(&root, "Brush/Wasser/G-Pen").subtool.name(), "G-Pen");
    assert_eq!(meta::read(&root.join("Brush/Wasser")).0.order, ["G-Pen.sut", "Nass.sut"]);
    // A broken file is copied as it is.
    fs::write(root.join("Pen/Rau/Kaputt.sut"), b"junk").unwrap();
    ops::copy_entry(&root, &p("Pen/Rau/Kaputt"), Some(&p("Pen/Rau")), None).unwrap();
    assert_eq!(fs::read(root.join("Pen/Rau/Kaputt 2.sut")).unwrap(), b"junk");
}

#[test]
fn copied_folders_get_fresh_uuids_and_their_folder_name() {
    let (_d, root) = stock();
    meta::write(&root.join("Pen"), &FolderMeta { name: Some("Stift".into()), icon: Some("csp:7".into()), ..Default::default() }).unwrap();
    assert_eq!(ops::copy_entry(&root, &p("Pen"), None, None).unwrap(), p("Pen 2"));
    assert_eq!(ops::copy_entry(&root, &p("Pen/Glatt"), Some(&p("Brush")), None).unwrap(), p("Brush/Glatt"));
    let s = Stock::load(&root).unwrap();
    let copy = s.tool("Pen 2").unwrap();
    assert_eq!((copy.name.as_str(), copy.icon.as_deref()), ("Pen 2", Some("csp:7")));
    let uuid = |path: &str| sut_at(&root, path).subtool.uuid().to_vec();
    assert_ne!(uuid("Pen/Glatt/G-Pen"), uuid("Pen 2/Glatt/G-Pen"));
    assert_ne!(uuid("Pen/Glatt/G-Pen"), uuid("Brush/Glatt/G-Pen"));
    assert_eq!(sut_at(&root, "Pen 2/Glatt/G-Pen").subtool.variant, sut_at(&root, "Pen/Glatt/G-Pen").subtool.variant);
}

#[test]
fn sorting_sets_and_clears_the_order() {
    let (_d, root) = stock();
    assert_eq!(ops::sort(&root, Some(&p("Pen/Glatt")), Some(&["G-Pen".into()])).unwrap(), ["G-Pen.sut", "Fineliner.sut"]);
    assert_eq!(ops::sort(&root, None, Some(&["Pen".into(), "Brush".into()])).unwrap(), ["Pen", "Brush"]);
    assert_eq!(ops::sort(&root, Some(&p("Pen/Glatt")), None).unwrap(), ["Fineliner.sut", "G-Pen.sut"]);
    assert!(!root.join("Pen/Glatt/_clippy.json").exists(), "an empty metadata file goes");
    assert!(matches!(ops::sort(&root, Some(&p("Pen/Glatt")), Some(&["Nope".into()])), Err(Error::NotFound(_))));
    assert!(matches!(ops::sort(&root, None, Some(&["../etc".into()])), Err(Error::NotFound(_))));
    assert!(ops::sort(&root, Some(&p("Pen/Glatt/G-Pen")), None).is_err());
    assert!(ops::sort(&root, Some(&p("Missing")), None).is_err());
}

#[test]
fn metadata_is_set_and_cleared() {
    let (_d, root) = stock();
    let change = MetaChange { name: Some(Some("Stift\u{7}".into())), icon: Some(Some("csp:3".into())), shortcut: Some(Some("P".into())) };
    ops::set_meta(&root, &p("Pen"), &change).unwrap();
    ops::set_meta(&root, &p("Pen/Glatt/G-Pen"), &MetaChange { shortcut: Some(Some("Shift+P".into())), ..Default::default() }).unwrap();
    let s = Stock::load(&root).unwrap();
    let pen = s.tool("Pen").unwrap();
    assert_eq!((pen.name.as_str(), pen.icon.as_deref(), pen.shortcut.as_deref()), ("Stift", Some("csp:3"), Some("P")));
    assert_eq!(pen.groups[0].subtools[1].shortcut.as_deref(), Some("Shift+P"));
    ops::set_meta(&root, &p("Pen"), &MetaChange { name: Some(None), icon: Some(None), shortcut: Some(None) }).unwrap();
    ops::set_meta(&root, &p("Pen/Glatt/G-Pen"), &MetaChange { shortcut: Some(None), ..Default::default() }).unwrap();
    assert!(!root.join("Pen/_clippy.json").exists() && !root.join("Pen/Glatt/_clippy.json").exists());
    assert!(ops::set_meta(&root, &p("Pen/Glatt/G-Pen"), &MetaChange { name: Some(Some("x".into())), ..Default::default() }).is_err());
    assert!(ops::set_meta(&root, &p("Missing"), &MetaChange::default()).is_err());
}

#[test]
fn a_sut_is_imported_byte_for_byte_into_a_group() {
    let (d, root) = stock();
    let file = d.path().join("export/Weird ／ name.sut");
    let mut sut = synth::sut(42, 2).unwrap();
    sut.subtool.set_name("../../Evil/Name");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    sut.save(&file).unwrap();
    let path = import::import_sut(&root, &file, "Pen", "Glatt").unwrap();
    assert_eq!(path, EntryPath::SubTool("Pen".into(), "Glatt".into(), "_._.._Evil_Name".into()));
    assert_eq!(fs::read(path.to_path(&root)).unwrap(), fs::read(&file).unwrap());
    assert_eq!(meta::read(&root.join("Pen/Glatt")).0.order.last().map(String::as_str), Some("_._.._Evil_Name.sut"));
    // Twice: numbered.
    assert_eq!(import::import_sut(&root, &file, "Pen", "Glatt").unwrap().name(), "_._.._Evil_Name 2");
    // Broken and missing files and groups are errors.
    fs::write(d.path().join("bad.sut"), b"nope").unwrap();
    assert!(matches!(import::import_sut(&root, &d.path().join("bad.sut"), "Pen", "Glatt"), Err(Error::Sut { .. })));
    assert!(import::import_sut(&root, &d.path().join("missing.sut"), "Pen", "Glatt").is_err());
    assert!(matches!(import::import_sut(&root, &file, "Pen", "Missing"), Err(Error::NotFound(_))));
    assert!(import::import_sut(&root, d.path(), "Pen", "Glatt").is_err());
}

/// 22 tools, 38 groups, 210 sub tools: the shape of the owner's Clip Studio Paint 1.5 database.
fn owner_shape() -> Vec<Vec<usize>> {
    let mut shape: Vec<Vec<usize>> = (0..22).map(|t| if t < 16 { vec![6, 5] } else { vec![1] }).collect();
    // 16 × 2 + 6 × 1 = 38 groups; 16 × 11 + 6 = 182 sub tools; 28 more in the first groups.
    for t in shape.iter_mut().take(14) {
        t[0] += 2;
    }
    assert_eq!(shape.iter().map(Vec::len).sum::<usize>(), 38);
    assert_eq!(shape.iter().flatten().sum::<usize>(), 210);
    shape
}

/// A UTF-16 `.todb` of `shape` laid out like `synth::todb`, but for any number of nodes. Tools 1
/// and 2 are both named "Pen/Marker"; the first sub tool of every group uses the material `refs`.
fn todb(shape: &[Vec<usize>], refs: Option<&MaterialRefs>) -> Vec<u8> {
    use rusqlite::types::Value as Sql;
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA page_size = 1024; PRAGMA encoding = 'UTF-16le';").unwrap();
    let info = synth::template();
    for o in info.objects.iter().filter(|o| o.name != "sqlite_sequence" && o.name != known::MATERIAL_TABLE) {
        conn.execute(o.sql.as_deref().unwrap(), []).unwrap();
    }
    let insert = |table: &str, cells: Vec<(&str, Sql)>| {
        let names: Vec<&str> = cells.iter().map(|(n, _)| *n).collect();
        let marks: Vec<String> = (1..=cells.len()).map(|i| format!("?{i}")).collect();
        let sql = format!("INSERT INTO {table} ({}) VALUES ({})", names.join(", "), marks.join(", "));
        conn.execute(&sql, rusqlite::params_from_iter(cells.into_iter().map(|(_, v)| v))).unwrap();
    };
    // Depth-first: (parent, depth, name).
    let mut nodes: Vec<(Option<usize>, usize, String)> = vec![(None, 0, String::new())];
    for (t, groups) in shape.iter().enumerate() {
        let tool = nodes.len();
        nodes.push((Some(0), 1, if t == 1 || t == 2 { "Pen/Marker".into() } else { format!("Tool {t}") }));
        for (g, &subs) in groups.iter().enumerate() {
            let group = nodes.len();
            nodes.push((Some(tool), 2, format!("Group {t}.{g}")));
            for s in 0..subs {
                nodes.push((Some(group), 3, format!("Sub {t}.{g}.{s}")));
            }
        }
    }
    let uuid = |i: usize| -> Sql {
        Sql::Blob(
            (0..16u8)
                .map(|k| match k {
                    0 => 1,
                    1 => (i >> 8) as u8,
                    2 => i as u8,
                    _ => k.wrapping_mul(17),
                })
                .collect(),
        )
    };
    let none = || Sql::Blob(vec![0]);
    let mut manager = vec![("ToolType", Sql::Integer(0)), ("Version", Sql::Integer(138)), ("RootUuid", uuid(0))];
    manager.push(("CurrentNodeUuid", none()));
    insert("Manager", manager);
    let mut variant = 0;
    for (i, (parent, depth, name)) in nodes.iter().enumerate() {
        let next = (i + 1..nodes.len()).find(|&j| nodes[j].1 <= *depth).filter(|&j| nodes[j].0 == *parent).map_or_else(none, uuid);
        let child = nodes.get(i + 1).filter(|n| n.0 == Some(i)).map_or_else(none, |_| uuid(i + 1));
        let mut cells = vec![("NodeUuid", uuid(i)), ("NodeName", Sql::Text(name.clone())), ("NodeNextUuid", next), ("NodeFirstChildUuid", child)];
        cells.push(("NodeIcon", Sql::Integer(100 + *depth as i64)));
        if *depth == 3 {
            let first_in_group = nodes.get(i - 1).is_some_and(|n| n.1 == 2);
            for column in ["NodeVariantID", "NodeInitVariantID"] {
                variant += 1;
                cells.push((column, Sql::Integer(variant)));
                let mut v = vec![("VariantID", Sql::Integer(variant)), (known::BRUSH_SIZE, Sql::Real(variant as f64))];
                if let (Some(r), true) = (refs, first_in_group) {
                    v.push((known::BRUSH_PATTERN_IMAGES, Sql::Blob(r.encode().unwrap())));
                }
                insert("Variant", v);
            }
        }
        insert("Node", cells);
    }
    conn.serialize("main").unwrap().to_vec()
}

#[test]
fn a_tool_database_becomes_tools_groups_and_sut_files() {
    let (d, root) = stock();
    // One material in Clip Studio Paint's material folder, one missing.
    let full = synth::sut(11, 2).unwrap();
    let refs = MaterialRefs::decode(full.subtool.variant.as_ref().unwrap().blob(known::BRUSH_PATTERN_IMAGES).unwrap()).unwrap();
    let r = &refs.entries[0];
    let folder = d.path().join("Material/Paint001").join(r.uuid().unwrap()).join("data");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join(r.file_name().unwrap()), full.subtool.materials[0].archive(&Limits::default()).unwrap().tip_layer().unwrap()).unwrap();
    let file = d.path().join("EditImageTool.todb");
    fs::write(&file, todb(&owner_shape(), Some(&refs))).unwrap();
    let report = import::import_todb(&root, &file, Some(&d.path().join("Material"))).unwrap();
    assert_eq!((report.tools.len(), report.groups.len(), report.subtools.len()), (22, 38, 210));
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(report.materials, 38, "the first sub tool of every group embeds the one material found");
    assert_eq!(report.missing_materials, [refs.entries[1].path.clone()]);
    let s = Stock::load(&root).unwrap();
    assert!(s.warnings.is_empty(), "{:?}", s.warnings);
    assert_eq!(s.tools.len(), 24, "two tools were there before");
    let imported = &s.tools[2..];
    assert_eq!(imported.len(), 22);
    assert_eq!(imported.iter().map(|t| t.groups.len()).sum::<usize>(), 38);
    assert_eq!(imported.iter().flat_map(|t| &t.groups).map(|g| g.subtools.len()).sum::<usize>(), 210);
    assert!(imported.iter().flat_map(|t| &t.groups).flat_map(|g| &g.subtools).all(|e| e.warning.is_none()));
    // Clip Studio Paint's order, not alphabetical; hostile and repeated names made into folders.
    let order: Vec<_> = imported.iter().map(|t| t.dir.as_str()).collect();
    assert_eq!(order.get(..4), Some(&["Tool 0", "Pen_Marker", "Pen_Marker 2", "Tool 3"][..]));
    assert_eq!(order.get(9..11), Some(&["Tool 9", "Tool 10"][..]));
    assert_eq!((imported[1].name.as_str(), imported[2].name.as_str()), ("Pen/Marker", "Pen/Marker"), "the exact name is the display name");
    assert_eq!(imported[0].icon.as_deref(), Some("csp:101"));
    // The sub tools are the database's, setting for setting, plus their materials.
    let db = photocraft_clippy_sut::Todb::open(&file).unwrap();
    let (_, _, first) = db.tree.subtools().next().unwrap();
    let sut = sut_at(&root, "Tool 0/Group 0.0/Sub 0.0.0");
    let (mut a, mut b) = (sut.subtool.node.clone(), first.node.clone());
    for link in ["NodeNextUuid", "NodeFirstChildUuid"] {
        a.remove(link);
        b.remove(link);
    }
    assert!(a.same_values(&b));
    assert_eq!(sut.subtool.variant, first.variant);
    assert_eq!(sut.subtool.materials.len(), 1);
    assert_eq!(sut.subtool.materials[0].original_path(), Some(r.path.as_str()));
    assert!(sut_at(&root, "Tool 0/Group 0.0/Sub 0.0.1").subtool.materials.is_empty());
    // No staging folder is left behind.
    assert!(fs::read_dir(&root).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().starts_with('.')));
    // Importing again: numbered tools.
    let again = import::import_todb(&root, &file, None).unwrap();
    assert_eq!(again.tools.first().map(String::as_str), Some("Tool 0 2"));
    assert_eq!(again.materials, 0);
}

#[test]
fn broken_tool_databases_are_refused_without_leftovers() {
    let (d, root) = stock();
    for (name, bytes) in [
        ("empty.todb", Vec::new()),
        ("junk.todb", b"SQLite format 3\0".repeat(20)),
        ("cycle.todb", synth::todb(&[&[2]], &[(3, "NodeNextUuid", Some(2))]).unwrap()),
    ] {
        let file = d.path().join(name);
        fs::write(&file, bytes).unwrap();
        assert!(matches!(import::import_todb(&root, &file, None), Err(Error::Sut { .. })), "{name}");
    }
    assert!(import::import_todb(&root, &d.path().join("missing.todb"), None).is_err());
    let file = d.path().join("ok.todb");
    fs::write(&file, synth::todb(&[&[1]], &[]).unwrap()).unwrap();
    assert!(matches!(import::import_todb(&root, &file, Some(&d.path().join("no-materials"))), Err(Error::NotFound(_))));
    assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
}

#[test]
fn materials_are_embedded_from_the_material_folder() {
    let d = tempfile::tempdir().unwrap();
    // A sub tool as it comes out of a .todb: material references, nothing embedded.
    let full = synth::sut(7, 2).unwrap();
    let mut sut = full.clone();
    sut.subtool.materials.clear();
    let refs = MaterialRefs::decode(full.subtool.variant.as_ref().unwrap().blob(known::BRUSH_PATTERN_IMAGES).unwrap()).unwrap();
    // Lay the first material out as Clip Studio Paint's material folder does: <...>/<uuid>/data/<file>.
    let r = &refs.entries[0];
    let tip = full.subtool.materials[0].archive(&Limits::default()).unwrap();
    let folder = d.path().join("Material/Paint/sub").join(r.uuid().unwrap());
    fs::create_dir_all(folder.join("data")).unwrap();
    fs::write(folder.join("data").join(r.file_name().unwrap()), tip.tip_layer().unwrap()).unwrap();
    fs::write(folder.join("catalog.xml"), b"<catalog/>").unwrap();
    let index = MaterialIndex::scan(&d.path().join("Material")).unwrap();
    assert!(index.find(r).is_some());
    let (n, missing) = import::embed(&mut sut, &index);
    assert_eq!(n, 1);
    assert_eq!(missing, [refs.entries[1].path.clone()]);
    let m = &sut.subtool.materials[0];
    assert_eq!(m.original_path(), Some(r.path.as_str()));
    let archive = m.archive(&Limits::default()).unwrap();
    assert_eq!(archive.tip_layer(), tip.tip_layer());
    assert_eq!(archive.get("catalog.xml"), Some(&b"<catalog/>"[..]));
    // The file with the embedded material reads back and its tip decodes.
    let back = Sut::from_bytes(&sut.to_bytes().unwrap(), &Limits::default()).unwrap();
    let decoded = layer::decode(back.subtool.materials[0].archive(&Limits::default()).unwrap().tip_layer().unwrap(), &Limits::default()).unwrap();
    assert!(decoded.width > 0);
    // Already embedded: nothing to do.
    assert_eq!(import::embed(&mut sut, &index).0, 0);
}

fn preset(name: &str, sampled: bool) -> BrushPreset {
    let tip = if sampled { TipShape::Sampled(GrayTile::from_fn(9, 7, |x, y| ((x + y) % 3) as f32 / 2.0)) } else { TipShape::Round };
    BrushPreset {
        name: name.into(),
        brush: BrushSettings { size: 33.0, opacity: 0.5, tip, ..Default::default() },
        builtin: false,
        group: "Imported".into(),
        folder: Vec::new(),
        captures: Default::default(),
    }
}

#[test]
fn brush_presets_become_a_group_of_the_brush_tool() {
    let (_d, root) = stock();
    let presets = [preset("Soft/Round", false), preset("Chalk", true), preset("Chalk", true)];
    let (group, report) = import::import_presets(&root, None, "My Brushes.abr", &presets).unwrap();
    assert_eq!(group, p("Brush/My Brushes.abr"));
    assert_eq!(report.subtools, ["Brush/My Brushes.abr/Soft_Round", "Brush/My Brushes.abr/Chalk", "Brush/My Brushes.abr/Chalk 2"]);
    let s = Stock::load(&root).unwrap();
    assert!(s.warnings.is_empty(), "{:?}", s.warnings);
    let names: Vec<_> = s.tool("Brush").unwrap().groups.iter().find(|g| g.dir == "My Brushes.abr").unwrap().subtools.iter().map(|e| e.name.clone()).collect();
    assert_eq!(names, ["Soft_Round", "Chalk", "Chalk 2"], "in the file's order");
    let chalk = sut_at(&root, "Brush/My Brushes.abr/Chalk");
    let variant = chalk.subtool.variant.as_ref().unwrap();
    assert_eq!(variant.real(known::BRUSH_SIZE), Some(33.0));
    assert_eq!(variant.int(known::OPACITY), Some(50));
    assert_eq!(stored_preset(variant).unwrap().unwrap(), presets[1], "the whole preset is kept");
    // Tool kind from the brush tool's own sub tools.
    let neighbour = sut_at(&root, "Brush/Wasser/Nass");
    assert_eq!(chalk.subtool.input_op(), neighbour.subtool.input_op());
    // The tip is an embedded material Clip Studio Paint style.
    let tip = layer::decode(chalk.subtool.materials[0].archive(&Limits::default()).unwrap().tip_layer().unwrap(), &Limits::default()).unwrap();
    assert_eq!((tip.width, tip.height), (9, 7));
    let refs = variant.material_refs(known::BRUSH_PATTERN_IMAGES).unwrap().unwrap();
    assert_eq!(refs.entries[0].uuid(), chalk.subtool.materials[0].uuid());
    // Into a named tool; a second import of the same file name is numbered.
    let (g2, _) = import::import_presets(&root, Some("Airbrush"), "My Brushes.abr", &presets[..1]).unwrap();
    assert_eq!(g2, p("Airbrush/My Brushes.abr"));
    let (g3, _) = import::import_presets(&root, None, "My Brushes.abr", &[]).unwrap();
    assert_eq!(g3, p("Brush/My Brushes.abr 2"));
    assert!(import::import_presets(&root, Some("../x"), "g", &[]).is_err());
}

#[test]
fn without_a_brush_tool_one_is_made() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("stock");
    fs::create_dir(&root).unwrap();
    let (group, _) = import::import_presets(&root, None, "", &[preset("x", false)]).unwrap();
    assert_eq!(group, p("Brush/Brushes"));
    // A tool whose display name is Pinsel counts as the brush tool.
    let root2 = d.path().join("stock2");
    fs::create_dir_all(root2.join("T1")).unwrap();
    meta::write(&root2.join("T1"), &FolderMeta { name: Some("Pinsel".into()), ..Default::default() }).unwrap();
    assert_eq!(import::import_presets(&root2, None, "g", &[]).unwrap().0, p("T1/g"));
}

#[test]
fn exports_are_verbatim_copies() {
    let (d, root) = stock();
    let out = d.path().join("out");
    fs::create_dir(&out).unwrap();
    let whole = export::export(&root, None, &out).unwrap();
    assert_eq!(whole, out.join("stock"));
    assert_eq!(fs::read(whole.join("Pen/Glatt/G-Pen.sut")).unwrap(), fs::read(root.join("Pen/Glatt/G-Pen.sut")).unwrap());
    assert_eq!(export::export(&root, None, &out).unwrap(), out.join("stock 2"));
    assert_eq!(export::export(&root, Some(&p("Pen")), &out).unwrap(), out.join("Pen"));
    assert_eq!(export::export(&root, Some(&p("Pen/Glatt")), &out).unwrap(), out.join("Glatt"));
    assert_eq!(export::export(&root, Some(&p("Pen/Glatt/G-Pen")), &out).unwrap(), out.join("G-Pen.sut"));
    assert!(matches!(export::export(&root, None, &root.join("Pen")), Err(Error::Invalid(_))), "not into itself");
    assert!(matches!(export::export(&root, Some(&p("Pen")), &root.join("Pen/Glatt")), Err(Error::Invalid(_))));
    assert!(matches!(export::export(&root, None, &d.path().join("missing")), Err(Error::NotFound(_))));
    assert!(matches!(export::export(&root, Some(&p("Missing")), &out), Err(Error::NotFound(_))));
}

#[test]
fn save_as_copies_the_stock_and_leaves_it_alone() {
    let (d, root) = stock();
    let before = names(&Stock::load(&root).unwrap());
    let dest = d.path().join("new/place");
    assert_eq!(export::save_as(&root, &dest).unwrap(), dest);
    assert_eq!(names(&Stock::load(&dest).unwrap()), before);
    assert_eq!(names(&Stock::load(&root).unwrap()), before);
    // Working on the copy leaves the old place unchanged.
    ops::delete(&dest, &p("Pen"), &trash(&d)).unwrap();
    assert_eq!(names(&Stock::load(&root).unwrap()), before);
    assert!(matches!(export::save_as(&root, &dest), Err(Error::Exists(_))), "not into a folder with files");
    assert!(matches!(export::save_as(&root, &root.join("Pen/inside")), Err(Error::Invalid(_))));
    fs::write(d.path().join("file"), b"x").unwrap();
    assert!(export::save_as(&root, &d.path().join("file")).is_err());
    let empty = d.path().join("empty");
    fs::create_dir(&empty).unwrap();
    export::save_as(&root, &empty).unwrap();
    // A stock never used saves as an empty folder.
    export::save_as(&d.path().join("never"), &d.path().join("fresh")).unwrap();
    assert!(d.path().join("fresh").is_dir());
}

/// P6 acceptance: a move in a file manager shows in the model in under a second, and the
/// selection follows it.
#[test]
fn a_file_manager_move_shows_within_a_second() {
    let (d, root) = stock();
    let mut live = LiveStock::open(&root).unwrap();
    assert!(live.watch_error().is_none(), "{:?}", live.watch_error());
    live.select(Some(p("Pen/Glatt/G-Pen")));
    let start = Instant::now();
    fs::rename(root.join("Pen/Glatt/G-Pen.sut"), root.join("Pen/Rau/Moved.sut")).unwrap();
    wait_for(&mut live, |s| s.contains(&p("Pen/Rau/Moved")));
    let elapsed = start.elapsed();
    assert!(elapsed < Duration::from_secs(1), "{elapsed:?}");
    assert!(!live.stock().contains(&p("Pen/Glatt/G-Pen")));
    assert_eq!(live.selected(), Some(&p("Pen/Rau/Moved")));
    // A folder renamed: the selection inside it follows too.
    fs::rename(root.join("Pen"), root.join("Stift")).unwrap();
    wait_for(&mut live, |s| s.contains(&p("Stift/Rau/Moved")));
    assert_eq!(live.selected(), Some(&p("Stift/Rau/Moved")));
    // Files copied in, a sub tool deleted, metadata edited by hand.
    fs::copy(root.join("Stift/Rau/Kreide.sut"), root.join("Brush/Wasser/Kreide.sut")).unwrap();
    wait_for(&mut live, |s| s.contains(&p("Brush/Wasser/Kreide")));
    fs::remove_file(root.join("Stift/Rau/Moved.sut")).unwrap();
    wait_for(&mut live, |s| !s.contains(&p("Stift/Rau/Moved")));
    assert_eq!(live.selected(), None);
    fs::write(root.join("Brush/_clippy.json"), br#"{"name":"Pinsel"}"#).unwrap();
    wait_for(&mut live, |s| s.tool("Brush").is_some_and(|t| t.name == "Pinsel"));
    drop(d);
}

fn wait_for(live: &mut LiveStock, done: impl Fn(&Stock) -> bool) -> Vec<String> {
    let end = Instant::now() + Duration::from_secs(5);
    let mut seen = Vec::new();
    while !done(live.stock()) {
        assert!(Instant::now() < end, "the change never showed; saw {seen:?}");
        if let Some(u) = live.wait(Duration::from_millis(100)).unwrap() {
            seen.extend(u.foreign.iter().map(|c| format!("{c:?}")));
        }
    }
    seen
}

#[test]
fn clippys_own_changes_are_not_foreign_and_keep_the_selection() {
    let (_d, root) = stock();
    let mut live = LiveStock::open(&root).unwrap();
    live.select(Some(p("Pen/Glatt/G-Pen")));
    let moved = ops::move_entry(&root, &p("Pen/Glatt/G-Pen"), Some(&p("Brush/Wasser")), None).unwrap();
    let update = live.wait(Duration::from_secs(2)).unwrap().unwrap();
    assert!(update.own);
    assert!(live.stock().contains(&moved));
    assert_eq!(live.selected(), Some(&moved));
    // Let the watcher's batch of that move arrive: none of it is foreign.
    let end = Instant::now() + Duration::from_millis(600);
    while Instant::now() < end {
        if let Some(u) = live.wait(Duration::from_millis(100)).unwrap() {
            assert!(u.foreign.is_empty(), "own changes reported as foreign: {:?}", u.foreign);
        }
    }
    ops::rename(&root, &p("Brush"), "Pinsel").unwrap();
    live.wait(Duration::from_secs(2)).unwrap();
    assert_eq!(live.selected(), Some(&p("Pinsel/Wasser/G-Pen")));
}
