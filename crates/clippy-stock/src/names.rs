//! Names of tool and group folders and sub tool files, and stock paths (`Tool/Group/Sub tool`).
//!
//! A name given to a command is checked ([`validate`]): it must be usable as one file name on
//! Linux, macOS and Windows alike, since a stock folder may be synced between them. A name read
//! from a foreign file (a Clip Studio Paint node name) is made into one ([`sanitize`]). Clashes
//! get a number: "G-Pen", "G-Pen 2", "G-Pen 3" ([`unique`]).

use std::path::{Path, PathBuf};

use crate::{Error, Result};

/// Metadata file of the stock, tool and group folders.
pub const META_FILE: &str = "_clippy.json";
/// File extension of sub tools.
pub const SUT_EXT: &str = "sut";
/// Longest name, in bytes (with room for " 9999" and ".sut" under common 255-byte limits).
pub const MAX_NAME_BYTES: usize = 120;

/// Characters no name may contain: separators and what Windows forbids.
const FORBIDDEN: [char; 9] = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
/// Windows device names, which can't be file names whatever the extension.
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7",
    "LPT8", "LPT9",
];

fn bad(name: &str, reason: &'static str) -> Error {
    Error::BadName { name: name.chars().take(80).collect(), reason }
}

fn is_reserved(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).trim_end();
    RESERVED.iter().any(|r| r.eq_ignore_ascii_case(stem))
}

/// Checks that `name` can be a tool or group folder or a sub tool's file stem.
pub fn validate(name: &str) -> Result<()> {
    if name.is_empty() || name.trim().is_empty() {
        return Err(bad(name, "it is empty"));
    }
    if name == "." || name == ".." {
        return Err(bad(name, "it names a folder itself"));
    }
    if name.starts_with('.') {
        return Err(bad(name, "names starting with a dot are hidden files"));
    }
    if name.chars().any(|c| FORBIDDEN.contains(&c)) {
        return Err(bad(name, r#"it contains one of / \ : * ? " < > |"#));
    }
    if name.chars().any(char::is_control) {
        return Err(bad(name, "it contains a control character"));
    }
    if name.ends_with(' ') || name.ends_with('.') || name.starts_with(' ') {
        return Err(bad(name, "it starts with a space or ends with a space or a dot"));
    }
    if name.len() > MAX_NAME_BYTES {
        return Err(bad(name, "it is longer than 120 bytes"));
    }
    if is_reserved(name) {
        return Err(bad(name, "it is a device name on Windows"));
    }
    if name.eq_ignore_ascii_case(META_FILE) || name.to_ascii_lowercase().starts_with("_clippy.") {
        return Err(bad(name, "it is the name of Clippy's metadata file"));
    }
    Ok(())
}

/// Makes a valid name of `raw` (a name from a foreign file): forbidden and control characters
/// become `_`, surrounding spaces and trailing dots go, long names are cut at a character
/// boundary. An empty result is `fallback` (made valid the same way, then "Unnamed").
pub fn sanitize(raw: &str, fallback: &str) -> String {
    for candidate in [raw, fallback, "Unnamed"] {
        let name = sanitize_once(candidate);
        if validate(&name).is_ok() {
            return name;
        }
    }
    "Unnamed".to_owned()
}

fn sanitize_once(raw: &str) -> String {
    let mut name: String = raw.chars().map(|c| if FORBIDDEN.contains(&c) || c.is_control() { '_' } else { c }).collect();
    let trim = |s: &str| s.trim().trim_end_matches(['.', ' ']).to_owned();
    name = trim(&name);
    if name.starts_with('.') {
        name.replace_range(..1, "_");
    }
    if name.len() > MAX_NAME_BYTES {
        let mut end = MAX_NAME_BYTES;
        while !name.is_char_boundary(end) {
            end -= 1;
        }
        name.truncate(end);
        name = trim(&name);
    }
    if is_reserved(&name) || name.to_ascii_lowercase().starts_with("_clippy.") {
        name.push('_');
    }
    name
}

/// `base` if `taken` says it is free, else "base 2", "base 3" …; a trailing number of `base` is
/// counted on ("G-Pen 2" → "G-Pen 3"). `taken` is asked with the candidate name; callers compare
/// case-insensitively, as macOS and Windows file systems do.
pub fn unique(base: &str, taken: impl Fn(&str) -> bool) -> Result<String> {
    validate(base)?;
    if !taken(base) {
        return Ok(base.to_owned());
    }
    // A trailing counter (2 or more, as this function writes them) counts on; "Tool 0" or
    // "Pen 1" are names of their own and get " 2".
    let counter = base.rsplit_once(' ').and_then(|(stem, n)| {
        let digits = !n.is_empty() && n.len() < 6 && n.bytes().all(|b| b.is_ascii_digit()) && !n.starts_with('0');
        let n = if digits { n.parse::<u32>().ok().filter(|&n| n >= 2)? } else { return None };
        (!stem.trim().is_empty()).then_some((stem, n + 1))
    });
    let (stem, start) = counter.unwrap_or((base, 2));
    for n in start..start.saturating_add(10_000) {
        let suffix = format!(" {n}");
        let mut s = stem.to_owned();
        while s.len() + suffix.len() > MAX_NAME_BYTES {
            s.pop();
        }
        let candidate = format!("{}{suffix}", s.trim_end());
        if validate(&candidate).is_ok() && !taken(&candidate) {
            return Ok(candidate);
        }
    }
    Err(Error::Exists(format!("`{base}` and 10 000 numbered names")))
}

/// The file name of a sub tool: `<stem>.sut`.
pub fn sut_file(stem: &str) -> String {
    format!("{stem}.{SUT_EXT}")
}

/// The stem of a sub tool file name, if it is one (`G-Pen.sut` → `G-Pen`; the extension is
/// matched case-insensitively).
pub fn sut_stem(file: &str) -> Option<&str> {
    let (stem, ext) = file.rsplit_once('.')?;
    (ext.eq_ignore_ascii_case(SUT_EXT) && !stem.is_empty()).then_some(stem)
}

/// An entry of the stock, by folder and file names (not display names).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EntryPath {
    /// A tool folder.
    Tool(String),
    /// A group folder in a tool.
    Group(String, String),
    /// A sub tool: tool, group and the file's stem (without `.sut`).
    SubTool(String, String, String),
}

impl EntryPath {
    /// Parses `Tool`, `Tool/Group` or `Tool/Group/Sub tool` (a trailing `.sut` is optional). Every
    /// part must be a valid name, so `..`, absolute paths and backslashes are errors.
    pub fn parse(path: &str) -> Result<Self> {
        let bad_path = |reason: &str| Error::BadPath { path: path.chars().take(200).collect(), reason: reason.to_owned() };
        if path.is_empty() {
            return Err(bad_path("it is empty"));
        }
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() > 3 {
            return Err(bad_path("a stock path has at most three parts: tool/group/sub tool"));
        }
        let mut owned = Vec::with_capacity(parts.len());
        for (i, part) in parts.iter().enumerate() {
            let part = if i == 2 { sut_stem(part).unwrap_or(part) } else { part };
            validate(part).map_err(|e| bad_path(&e.to_string()))?;
            owned.push(part.to_owned());
        }
        let mut it = owned.into_iter();
        Ok(match (it.next(), it.next(), it.next()) {
            (Some(t), None, None) => EntryPath::Tool(t),
            (Some(t), Some(g), None) => EntryPath::Group(t, g),
            (Some(t), Some(g), Some(s)) => EntryPath::SubTool(t, g, s),
            _ => return Err(bad_path("it is empty")),
        })
    }

    /// The path on disk under `root`.
    pub fn to_path(&self, root: &Path) -> PathBuf {
        match self {
            EntryPath::Tool(t) => root.join(t),
            EntryPath::Group(t, g) => root.join(t).join(g),
            EntryPath::SubTool(t, g, s) => root.join(t).join(g).join(sut_file(s)),
        }
    }

    /// The folder holding the entry, as a stock path (`None` for a tool: the stock itself).
    pub fn parent(&self) -> Option<EntryPath> {
        match self {
            EntryPath::Tool(_) => None,
            EntryPath::Group(t, _) => Some(EntryPath::Tool(t.clone())),
            EntryPath::SubTool(t, g, _) => Some(EntryPath::Group(t.clone(), g.clone())),
        }
    }

    /// The last part: the folder name, or the sub tool's stem.
    pub fn name(&self) -> &str {
        match self {
            EntryPath::Tool(n) | EntryPath::Group(_, n) | EntryPath::SubTool(_, _, n) => n,
        }
    }

    /// The name of the entry in its parent folder (`G-Pen.sut` for a sub tool).
    pub fn file_name(&self) -> String {
        match self {
            EntryPath::SubTool(_, _, s) => sut_file(s),
            other => other.name().to_owned(),
        }
    }

    /// This entry renamed to `name` (same parent).
    pub fn with_name(&self, name: &str) -> EntryPath {
        match self {
            EntryPath::Tool(_) => EntryPath::Tool(name.to_owned()),
            EntryPath::Group(t, _) => EntryPath::Group(t.clone(), name.to_owned()),
            EntryPath::SubTool(t, g, _) => EntryPath::SubTool(t.clone(), g.clone(), name.to_owned()),
        }
    }

    /// A child named `name` of this folder (`None` for a sub tool, which has none).
    pub fn child(&self, name: &str) -> Option<EntryPath> {
        match self {
            EntryPath::Tool(t) => Some(EntryPath::Group(t.clone(), name.to_owned())),
            EntryPath::Group(t, g) => Some(EntryPath::SubTool(t.clone(), g.clone(), name.to_owned())),
            EntryPath::SubTool(..) => None,
        }
    }

    /// `"tool"`, `"group"` or `"subtool"`.
    pub fn kind(&self) -> &'static str {
        match self {
            EntryPath::Tool(_) => "tool",
            EntryPath::Group(..) => "group",
            EntryPath::SubTool(..) => "subtool",
        }
    }

    /// The stock path of a path on disk under `root`, if it is one (sub tools need `.sut`).
    pub fn from_disk(root: &Path, path: &Path) -> Option<EntryPath> {
        let rel = path.strip_prefix(root).ok()?;
        let parts: Vec<&str> = rel.components().map(|c| c.as_os_str().to_str()).collect::<Option<_>>()?;
        match parts.as_slice() {
            [t] => validate(t).ok().map(|()| EntryPath::Tool((*t).to_owned())),
            [t, g] => (validate(t).is_ok() && validate(g).is_ok()).then(|| EntryPath::Group((*t).to_owned(), (*g).to_owned())),
            [t, g, f] => {
                let s = sut_stem(f)?;
                (validate(t).is_ok() && validate(g).is_ok() && validate(s).is_ok()).then(|| EntryPath::SubTool((*t).to_owned(), (*g).to_owned(), s.to_owned()))
            }
            _ => None,
        }
    }
}

impl std::fmt::Display for EntryPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntryPath::Tool(t) => write!(f, "{t}"),
            EntryPath::Group(t, g) => write!(f, "{t}/{g}"),
            EntryPath::SubTool(t, g, s) => write!(f, "{t}/{g}/{s}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostile_names_are_rejected() {
        for name in [
            "",
            " ",
            ".",
            "..",
            ".hidden",
            "a/b",
            "a\\b",
            "../x",
            "x/..",
            "a:b",
            "a*",
            "a?",
            "a\"",
            "a<",
            "a>",
            "a|",
            "a\0b",
            "a\nb",
            "a\u{7f}",
            "trailing.",
            "trailing ",
            " leading",
            "CON",
            "con.txt",
            "Lpt9",
            "_clippy.json",
            "_CLIPPY.JSON",
            "_clippy.json.bak",
        ] {
            assert!(validate(name).is_err(), "{name:?}");
        }
        assert!(validate(&"ä".repeat(61)).is_err());
        for name in ["G-Pen", "Pinsel 2", "ペン", "a.b", "_clippy", "Watercolor (wet)", "ä".repeat(60).as_str(), "CONSOLE"] {
            assert!(validate(name).is_ok(), "{name:?}");
        }
    }

    #[test]
    fn sanitized_names_are_always_valid() {
        let cases = ["", "..", "../../etc/passwd", "a/b\\c", ".x", "x.", " CON ", "\u{0}\u{1}", "_clippy.json", "ペン/鉛筆", &"🖌".repeat(100), "...", "   "];
        for raw in cases {
            let s = sanitize(raw, "fallback");
            assert!(validate(&s).is_ok(), "{raw:?} → {s:?}");
            assert!(!s.contains('/') && !s.contains('\\'), "{s}");
        }
        assert_eq!(sanitize("a/b", "f"), "a_b");
        assert_eq!(sanitize("..", "f"), "f");
        assert_eq!(sanitize("", "../.."), "_._");
        assert_eq!(sanitize("CON", "f"), "CON_");
        assert_eq!(sanitize("G-Pen ✒", "f"), "G-Pen ✒");
    }

    #[test]
    fn clashes_get_numbers() {
        let existing = ["g-pen", "g-pen 2", "x 9"];
        let taken = |n: &str| existing.contains(&n.to_lowercase().as_str());
        assert_eq!(unique("Brush", taken).unwrap(), "Brush");
        assert_eq!(unique("G-Pen", taken).unwrap(), "G-Pen 3");
        assert_eq!(unique("G-Pen 2", taken).unwrap(), "G-Pen 3");
        assert_eq!(unique("x 9", taken).unwrap(), "x 10");
        assert_eq!(unique("Tool 0", |n| n == "Tool 0").unwrap(), "Tool 0 2");
        assert_eq!(unique("Pen 1", |n| n == "Pen 1").unwrap(), "Pen 1 2");
        let long = "a".repeat(MAX_NAME_BYTES);
        let n = unique(&long, |n| n == long).unwrap();
        assert!(n.ends_with(" 2") && n.len() <= MAX_NAME_BYTES);
        assert!(unique("..", |_| false).is_err());
        assert!(unique("x", |_| true).is_err());
    }

    #[test]
    fn stock_paths_parse_and_reject_traversal() {
        assert_eq!(EntryPath::parse("Pen").unwrap(), EntryPath::Tool("Pen".into()));
        assert_eq!(EntryPath::parse("Pen/Glatt").unwrap(), EntryPath::Group("Pen".into(), "Glatt".into()));
        assert_eq!(EntryPath::parse("Pen/Glatt/G-Pen.sut").unwrap(), EntryPath::SubTool("Pen".into(), "Glatt".into(), "G-Pen".into()));
        assert_eq!(EntryPath::parse("Pen/Glatt/G-Pen").unwrap().to_string(), "Pen/Glatt/G-Pen");
        for bad in ["", "/", "/Pen", "Pen/", "../Pen", "Pen/../..", "Pen/./x", "a/b/c/d", "Pen\\x", "Pen//x", "C:", "Pen/Glatt/.sut", "Pen/Glatt/..sut"] {
            assert!(EntryPath::parse(bad).is_err(), "{bad:?}");
        }
        let root = Path::new("/stock");
        let p = EntryPath::parse("Pen/Glatt/G-Pen").unwrap();
        assert_eq!(p.to_path(root), Path::new("/stock/Pen/Glatt/G-Pen.sut"));
        assert_eq!(EntryPath::from_disk(root, &p.to_path(root)), Some(p.clone()));
        assert_eq!(EntryPath::from_disk(root, Path::new("/stock/Pen/Glatt/x.txt")), None);
        assert_eq!(EntryPath::from_disk(root, Path::new("/elsewhere/Pen")), None);
        assert_eq!(p.parent(), Some(EntryPath::Group("Pen".into(), "Glatt".into())));
    }
}
