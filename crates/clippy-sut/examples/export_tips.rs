//! Decodes brush tips and writes them as PNG, for looking at them.
//!
//! `cargo run -p photocraft-clippy-sut --example export_tips -- <folder|file.layer|file.sut> <out>`
//!
//! Every `.layer` under the folder (and the tips embedded in `.sut` files) becomes `<out>/<n>.png`
//! (straight RGBA), and `<out>/sheet.png` shows all of them over white, scaled into 128 px cells.

use std::path::{Path, PathBuf};

use image::{Rgba, RgbaImage, imageops};
use photocraft_clippy_sut::{Limits, Sut, layer};

const CELL: u32 = 128;
const COLUMNS: u32 = 12;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(input), Some(out)) = (args.get(1), args.get(2)) else {
        eprintln!("usage: export_tips <folder|file.layer|file.sut> <out folder>");
        std::process::exit(2);
    };
    let out = PathBuf::from(out);
    if let Err(e) = std::fs::create_dir_all(&out) {
        eprintln!("{}: {e}", out.display());
        std::process::exit(1);
    }
    let mut files = Vec::new();
    collect(Path::new(input), &mut files);
    files.sort();
    let mut tips = Vec::new();
    for path in &files {
        let decoded = if path.extension().is_some_and(|e| e == "sut") {
            Sut::open(path).map(|s| {
                s.subtool
                    .materials
                    .iter()
                    .filter_map(|m| m.archive(&Limits::default()).ok())
                    .filter_map(|a| a.tip_layer().and_then(|l| layer::decode(l, &Limits::default()).ok()))
                    .collect::<Vec<_>>()
            })
        } else {
            std::fs::read(path).map_err(Into::into).and_then(|b| layer::decode(&b, &Limits::default())).map(|t| vec![t])
        };
        match decoded {
            Ok(t) => tips.extend(t.into_iter().map(|t| (path.clone(), t))),
            Err(e) => eprintln!("{}: {e}", path.display()),
        }
    }
    let rows = (tips.len() as u32).div_ceil(COLUMNS).max(1);
    let mut sheet = RgbaImage::from_pixel(COLUMNS * CELL, rows * CELL, Rgba([255, 255, 255, 255]));
    for (i, (path, tip)) in tips.iter().enumerate() {
        let Some(img) = RgbaImage::from_raw(tip.width, tip.height, tip.rgba.clone()) else { continue };
        let name = out.join(format!("{i:03}.png"));
        if let Err(e) = img.save(&name) {
            eprintln!("{}: {e}", name.display());
        }
        println!("{i:03} {:?} {}x{} {}", tip.format, tip.width, tip.height, path.display());
        let scale = f64::from(CELL - 8) / f64::from(tip.width.max(tip.height));
        let (w, h) = (((f64::from(tip.width) * scale) as u32).max(1), ((f64::from(tip.height) * scale) as u32).max(1));
        let small = imageops::resize(&img, w, h, imageops::FilterType::Triangle);
        let (x, y) = ((i as u32 % COLUMNS) * CELL + (CELL - w) / 2, (i as u32 / COLUMNS) * CELL + (CELL - h) / 2);
        imageops::overlay(&mut sheet, &small, i64::from(x), i64::from(y));
    }
    if let Err(e) = sheet.save(out.join("sheet.png")) {
        eprintln!("sheet: {e}");
    }
}

fn collect(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        for entry in std::fs::read_dir(path).into_iter().flatten().flatten() {
            collect(&entry.path(), out);
        }
    } else if path.extension().is_some_and(|e| e == "layer" || e == "sut") {
        out.push(path.to_path_buf());
    }
}
