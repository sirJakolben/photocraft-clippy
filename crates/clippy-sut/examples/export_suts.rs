//! Writes every sub tool of a `.todb` as its own `.sut`, laid out like the database.
//!
//! `cargo run -p photocraft-clippy-sut --example export_suts -- EditImageTool.todb <out> [name]`
//!
//! With `name`, only sub tools whose name contains it are written. Materials are not embedded
//! (that is M7's import); sub tools using brush tip materials refer to them by path.

use std::path::{Path, PathBuf};

use photocraft_clippy_sut::Todb;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(input), Some(out)) = (args.get(1), args.get(2)) else {
        eprintln!("usage: export_suts <EditImageTool.todb> <out folder> [name filter]");
        std::process::exit(2);
    };
    let filter = args.get(3).map(String::as_str).unwrap_or("");
    let db = match Todb::open(Path::new(input)) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("{input}: {e}");
            std::process::exit(1);
        }
    };
    let out = PathBuf::from(out);
    if let Err(e) = std::fs::create_dir_all(&out) {
        eprintln!("{}: {e}", out.display());
        std::process::exit(1);
    }
    for sut in db.subtool_suts().filter(|s| s.subtool.name().contains(filter)) {
        let name: String = sut.subtool.name().chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect();
        let path = out.join(format!("{}.sut", name.trim()));
        match sut.save(&path) {
            Ok(()) => println!("{}", path.display()),
            Err(e) => eprintln!("{}: {e}", path.display()),
        }
    }
}
