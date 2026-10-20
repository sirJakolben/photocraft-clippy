# photocraft-clippy-stock

The sub tool stock of photocraft-clippy on disk: Clip Studio Paint–style tools, groups and sub
tools as plain folders and `.sut` files that a file manager can arrange as well as Clippy. Layer 4;
depends on `photocraft-clippy-sut` (the `.sut`/`.todb` format) and `photocraft-paint` (brush
presets of the `.abr` import). File-system code is desktop-only; the web build has the types.

## Layout

```text
<stock>/_clippy.json                    order of the tools
<stock>/<Tool>/_clippy.json             display name, icon, shortcut, order of the groups
<stock>/<Tool>/<Group>/_clippy.json     display name, icon, shortcut, order and shortcuts of the sub tools
<stock>/<Tool>/<Group>/<Sub tool>.sut   one sub tool, Clip Studio Paint's own format
```

`_clippy.json` (every key optional; unknown keys are kept when Clippy rewrites the file):

| Key         | Meaning                                                   | Missing                |
|-------------|-----------------------------------------------------------|------------------------|
| `name`      | Display name of the tool or group                         | Folder name            |
| `icon`      | Icon id (`csp:<NodeIcon>` for icons from Clip Studio Paint) | Default icon          |
| `shortcut`  | Shortcut of the tool or group (`Shift+P`)                 | None                   |
| `order`     | Entry names on disk (`G-Pen.sut`, `Glatt`) in display order | Alphabetical          |
| `shortcuts` | Shortcuts of the sub tools, by file name                  | None                   |

Entries not in `order` follow the listed ones alphabetically, so files a file manager adds come
last. From a `.todb` import the metadata also keeps `cspIconColor`, `cspShortcut` and
`cspShortcuts` (Clip Studio Paint's key codes, for the shortcut mapping).

A sub tool's display name is its file name; Clippy keeps the name inside the file (`NodeName`) in
step when it renames. Names must work as file names on Linux, macOS and Windows (no `/ \ : * ? " <
> |`, control characters, leading dot, trailing dot or space, device names, more than 120 bytes);
names from foreign files are made so (`names::sanitize`), names given to operations are checked
(`names::validate`). Stock paths name entries by folder and file names: `Tool`, `Tool/Group`,
`Tool/Group/Sub tool`; `..` and separators inside a part are errors.

## API

| Item                                   | What it does                                                        |
|----------------------------------------|---------------------------------------------------------------------|
| `Stock::load`, `load_cached`           | The stock in display order; never fails because of its contents     |
| `ops::create_tool`, `create_group`     | New folder, last; a clash gets a number ("Pen 2")                    |
| `ops::rename`                          | Folder or `.sut` (and the name inside it); a clash is an error      |
| `ops::delete`                          | To the trash (`trash::Trash`), the folder's metadata tidied         |
| `ops::move_entry`, `copy_entry`        | Into a folder at an index; copies are independent (fresh UUIDs)     |
| `ops::sort`, `set_meta`                | Order of a folder's entries; display name, icon, shortcut           |
| `import::import_sut`                   | A `.sut` into a group, byte for byte                                |
| `import::import_todb`                  | A whole tool database, with materials from the material folder      |
| `import::import_presets`               | PhotoCraft brush presets (an `.abr`) as a new group of the brush tool |
| `import::MaterialIndex`, `embed`       | Finds a material's `.layer` by UUID and embeds its folder           |
| `export::export`, `save_as`            | Copy of the stock or one entry; "Save Brushes As"                   |
| `watch::StockWatcher`                  | Batches file-system events (200 ms), own changes apart              |
| `live::LiveStock`                      | A stock kept current, with a selection that follows moves           |
| `own`                                  | Paths Clippy changes, so the watcher knows them as its own          |
| `stored_preset`, `PRESET_COLUMN`       | The PhotoCraft preset inside a `.sut` written by the `.abr` import  |

## Behaviour

- **Loading (P10).** Hidden entries, files other than `.sut` and folders with unusable names are
  ignored; symbolic links are not followed (a warning says so). An unreadable `_clippy.json` reads
  as the defaults with a warning and is kept as `_clippy.json.broken` before Clippy writes a new
  one. A `.sut` that doesn't read is an entry with `warning` set; everything else keeps working.
- **Writes** are atomic (hidden temporary file, sync, rename). Operations write the target's
  metadata first, then move the entry, then tidy the source's metadata: a stale name in `order`
  is harmless, so a failure part-way never hides an entry.
- **P8a.** A move or import copies the file byte for byte; a rename changes only `NodeName`; a
  copy only `NodeUuid` (and `NodeName` when it got a number). Every other column, known or not,
  stays as it was.
- **Trash.** Linux and the BSDs: the freedesktop.org home trash (`$XDG_DATA_HOME/Trash`), with
  `.trashinfo` files so file managers can restore; across file systems the entry is copied into
  the trash and then removed. The deletion date is written in UTC. macOS and Windows: the `trash`
  crate.
- **`.todb` import.** Read only, never written (P8). Tools are built in a hidden staging folder
  inside the stock and moved into place one by one; tools, groups and sub tools keep Clip Studio
  Paint's order, exact names (as display names when the folder name had to differ) and icons.
  Sub tools are written with the database's schema (UTF-8). A material reference
  (`.:Paint001:<uuid>:data:material.layer`) is looked up in the material folder given by the
  caller as a `.layer` of that name below a folder named by the UUID; the folder above `data/` is
  packed as the material archive. Missing materials are listed in the report.
- **`.abr` import.** The engine reads the file with `photocraft_io::abr_map` and passes the
  presets. Each becomes a `.sut` with a small schema of its own (`Manager`, `Node`, `Variant`,
  `MaterialFile`): brush size, opacity, the sampled tip as an embedded `.layer` (alpha = coverage)
  and the whole preset as zlib-compressed JSON in `Variant.PhotocraftBrushPreset`. The tool kind
  (`NodeInputOp`, `NodeOutputOp`, `NodeRangeOp`) is copied from a sub tool of the target tool.
- **Watching (P6).** `notify` (inotify, FSEvents, ReadDirectoryChangesW), recursive. Events are
  collected for 200 ms from the first one. Whether an event is Clippy's own is decided on the
  watcher thread as it arrives: its path, or a folder holding it, was noted by an operation in
  the last 3 s. Hidden paths are ignored, except a hidden file renamed to a visible name (an
  atomic save by another program).
- **Selection.** `LiveStock` reloads on foreign events and on any own operation (a process-wide
  revision), reading only `.sut` files whose size or modification time changed. The selected sub
  tool follows moves and renames (Clippy's own and those the watcher saw as rename pairs), and
  otherwise the one sub tool with its node UUID.

## Open questions

- Does Clip Studio Paint open `.sut` files with embedded materials Clippy packed from the material
  folder, and the `.sut` files of the `.abr` import (own schema, extra column)?
- The material folder layout is matched by UUID folder name, not read from Clip Studio Paint's
  `MaterialDB`; the acceptance test on a real installation will tell whether that finds them all.

## Tests

`cargo test -p photocraft-clippy-stock`: names, metadata, trash, every operation with its error
cases (hostile names, `../`, missing entries, read-only folders when not run as root, clashes),
import of generated `.sut`, `.todb` (22 tools, 38 groups, 210 sub tools, with materials) and brush
presets, export, Save As, and the watcher (a file-manager move shows within a second; own changes
are not foreign). With `CLIPPY_CSP_FIXTURES=<folder>` (see `photocraft-clippy-sut`) the acceptance
tests import the owner's `EditImageTool.todb` with its `Material/` folder (22/38/210) and every
exported `.sut`; without it they pass without checking anything.
