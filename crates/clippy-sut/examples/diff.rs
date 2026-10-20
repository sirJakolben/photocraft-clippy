//! Shows what differs between two sub tools, column by column, with known blobs decoded.
//!
//! `cargo run -p photocraft-clippy-sut --example diff -- before.sut after.sut`
//!
//! Made for finding out what a setting is stored as: export a sub tool from Clip Studio Paint,
//! change one setting, export again, and diff the two files.

use std::path::Path;

use photocraft_clippy_sut::{Effector, MaterialRefs, Row, ShowParams, Sut, Value, known};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(a), Some(b)) = (args.get(1), args.get(2)) else {
        eprintln!("usage: diff <before.sut> <after.sut>");
        std::process::exit(2);
    };
    let (a, b) = match (Sut::open(Path::new(a)), Sut::open(Path::new(b))) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let mut n = 0;
    n += diff_rows("Manager", &a.info.manager, &b.info.manager);
    n += diff_rows("Node", &a.subtool.node, &b.subtool.node);
    for (label, va, vb) in
        [("Variant (current)", &a.subtool.variant, &b.subtool.variant), ("Variant (initial)", &a.subtool.init_variant, &b.subtool.init_variant)]
    {
        match (va, vb) {
            (Some(va), Some(vb)) => n += diff_rows(label, &va.row, &vb.row),
            (None, None) => {}
            _ => {
                println!("{label}: present in only one file");
                n += 1;
            }
        }
    }
    if a.subtool.materials != b.subtool.materials {
        println!("MaterialFile: {} vs {} embedded materials, contents differ", a.subtool.materials.len(), b.subtool.materials.len());
        n += 1;
    }
    println!("{n} difference(s)");
}

fn diff_rows(table: &str, a: &Row, b: &Row) -> usize {
    let mut n = 0;
    for cell in a.cells() {
        match b.cell(&cell.name) {
            None => {
                println!("{table}.{}: only in the first file ({})", cell.name, show(&cell.value));
                n += 1;
            }
            Some(other) if !cell.value.same(&other.value) => {
                println!("{table}.{}:", cell.name);
                describe(&cell.name, &cell.decl_type, &cell.value, &other.value);
                n += 1;
            }
            Some(_) => {}
        }
    }
    for cell in b.cells().iter().filter(|c| !a.contains(&c.name)) {
        println!("{table}.{}: only in the second file ({})", cell.name, show(&cell.value));
        n += 1;
    }
    n
}

fn describe(name: &str, decl: &str, a: &Value, b: &Value) {
    let (Value::Blob(x), Value::Blob(y)) = (a, b) else {
        println!("    {} -> {}", show(a), show(b));
        return;
    };
    match known::kind(name, decl) {
        known::Kind::Effector => match (Effector::decode(x), Effector::decode(y)) {
            (Ok(x), Ok(y)) => {
                for (i, (s, t)) in x.slots().iter().zip(y.slots()).enumerate() {
                    if s != t {
                        println!("    slot {i}: {s} (0x{s:x}) -> {t} (0x{t:x})");
                    }
                }
                if x.slots().len() != y.slots().len() {
                    println!("    header: {} -> {} slots", x.slots().len(), y.slots().len());
                }
                for (label, cx, cy) in [("pressure curve", x.pressure_curve(), y.pressure_curve()), ("tilt curve", x.tilt_curve(), y.tilt_curve())] {
                    if cx != cy {
                        println!("    {label}: {:?} -> {:?}", cx.map(|c| &c.points), cy.map(|c| &c.points));
                    }
                }
            }
            _ => hex_diff(x, y),
        },
        known::Kind::ShowParams => match (ShowParams::decode(x), ShowParams::decode(y)) {
            (Ok(x), Ok(y)) => {
                let added: Vec<_> = y.ids.iter().filter(|i| !x.contains(**i)).collect();
                let removed: Vec<_> = x.ids.iter().filter(|i| !y.contains(**i)).collect();
                println!("    ids added {added:?}, removed {removed:?}");
            }
            _ => hex_diff(x, y),
        },
        known::Kind::MaterialRefs => match (MaterialRefs::decode(x), MaterialRefs::decode(y)) {
            (Ok(x), Ok(y)) => {
                let paths = |r: &MaterialRefs| r.entries.iter().map(|e| format!("{} ({})", e.path, e.name().unwrap_or_default())).collect::<Vec<_>>();
                println!("    {:?}\n -> {:?}", paths(&x), paths(&y));
            }
            _ => hex_diff(x, y),
        },
        _ => hex_diff(x, y),
    }
}

/// Prints the differing 4-byte words of two blobs as big-endian `i32`, the unit most blobs use.
fn hex_diff(a: &[u8], b: &[u8]) {
    if a.len() != b.len() {
        println!("    length {} -> {}", a.len(), b.len());
    }
    let mut shown = 0;
    for (i, (x, y)) in a.chunks(4).zip(b.chunks(4)).enumerate() {
        if x != y && shown < 40 {
            let word = |w: &[u8]| if w.len() == 4 { i32::from_be_bytes([w[0], w[1], w[2], w[3]]).to_string() } else { format!("{w:02x?}") };
            println!("    @{:5}: {:>11} -> {:>11}   ({:02x?} -> {:02x?})", i * 4, word(x), word(y), x, y);
            shown += 1;
        }
    }
}

fn show(v: &Value) -> String {
    match v {
        Value::Blob(b) => format!("blob, {} bytes", b.len()),
        Value::Text(t) => format!("{t:?}"),
        other => format!("{other:?}"),
    }
}
