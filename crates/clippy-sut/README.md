# photocraft-clippy-sut

Reader and writer for Clip Studio Paint tool files, for Clippy (PhotoCraft Extension): sub tools
(`.sut`), tool databases (`.todb`) and brush tip materials (`.layer`). No workspace dependencies
(layer 0); SQLite (bundled through `rusqlite`) and tar are desktop-only, the blob and `.layer`
decoders also build for wasm.

Clip Studio Paint is a trademark of CELSYS, Inc. This crate is not affiliated with CELSYS. The
layouts below are our own observations of files a user exported or installed, made for
interoperability; no Clip Studio Paint program code was examined and no data is decrypted.

## API

| Item                         | What it does                                                             |
|------------------------------|--------------------------------------------------------------------------|
| `Todb::open`, `from_bytes`   | Tool tree: root → tools → groups → sub tools                             |
| `Sut::open`, `from_bytes`    | One sub tool with its embedded materials                                 |
| `Sut::from_subtool`          | A `.sut` for any sub tool, laid out like a template (`DbInfo`)           |
| `Sut::to_bytes`, `save`      | Writes the `.sut` (atomic save: temporary file, sync, rename)            |
| `Variant`, `Row`, `Value`    | All settings as stored; unknown columns survive a round trip             |
| `known`                      | Names of the columns PhotoCraft maps and their layout (`known::kind`)    |
| `Effector`, `Curve`          | Pen pressure, tilt, velocity and random influence of one setting         |
| `ShowParams`                 | Settings shown in the Tool Property palette                              |
| `MaterialRefs`               | Brush tips and textures a sub tool uses                                  |
| `MaterialArchive`            | The tar archive of an embedded material                                  |
| `layer::decode`, `encode`    | Brush tip ⇄ straight RGBA                                                |
| `Limits`                     | Caps on file size, nodes, rows, tip size and entry size                  |

Examples: `diff` (what differs between two `.sut`, blobs decoded), `export_suts` (every sub tool
of a `.todb` as its own `.sut`), `export_tips` (tips as PNG plus a contact sheet).

## `.todb` and `.sut`

SQLite databases with the tables `Manager` (one row: root node, version 138 for Clip Studio
Paint 1.5, global pressure graph), `Node` (the tree), `Variant` (one row per settings set) and, in
`.sut`, `MaterialFile`. `.todb` is UTF-16, `.sut` UTF-8. Column sets and order differ between
files (479 `Variant` columns in a 1.5 `.todb`, 184 in an exported `.sut`); rows are read by name.

- Tree: `Manager.RootUuid` → node; children are `NodeFirstChildUuid`, then `NodeNextUuid`; "none"
  is the one-byte blob `00`. Levels: root, tool, group, sub tool. Cycles, shared nodes, dangling
  links and a fourth level are errors.
- A sub tool's settings: `NodeVariantID` (current) and `NodeInitVariantID` (initial).
- A `.sut` holds one node (`RootUuid` = its UUID, `CurrentNodeUuid` = `00`, links `00`), its two
  variants and the materials. The writer creates the template's own `CREATE TABLE` statements,
  keeps its page size and `Manager` values, and refuses columns the template lacks.

## Blobs (all big-endian)

**`…Effector`**: `i32` header length (44; 40 in old rows), offered sources, active sources (bits
`0x10` pressure, `0x20` tilt, `0x40` velocity, `0x80` random; `0x100` is offered but never seen
active), minimum in percent for pressure, tilt, velocity and random, an unused slot, byte length
of the pressure curve, byte length of the tilt curve, tilt range (100–500; 44-byte header only);
then the curves. A curve is `i32` 12, point count, 16, then points as two `f64` (input, output).
Checked on all 5 455 effectors of a 1.5 tool database: lengths add up, every pressure-active
effector has a pressure curve and every tilt-active one a tilt curve; velocity and random have
none.

**`VariantShowParam`**: `i32` 12, count, 4, then setting ids. Ids group by area: 10–50 ink and
stabilization, 100–120 texture, 1000–1910 brush, 2000–2101 view tools, 3000–3251 selection,
figures, balloons.

**Material lists** (`BrushPatternImageArray`, `TextureImage`, `Dual…`): `i32` 8, count; per entry
`i32` length (including itself), `i32` path byte length, the path in UTF-16 LE
(`.:Paint001:<uuid>:data:material.layer` for installed materials, `.:<nn>:<nn>:<uuid>:…` for own
ones and those inside a `.sut`), `i32` type, `i32` name byte length, the name in UTF-16 LE, and
more fields kept verbatim.

## `.layer`

`\x89C2F\r\n\x1a\n`, then chunks `u32` LE length, type, data, CRC-32 LE over type and data:
`HEAD`, `dATA`…, `TAIL`. The `dATA` chunks are an SQLite database image, each prefixed by a
`u16` LE flag: 1 = this part is encrypted (always the first 5 120 bytes, plus 8 bytes), 0 = in the
clear. The decoder reads the clear pages directly (page size 1 024 or 4 096, found by trying),
follows overflow chains, and takes the `Offscreen` row (`…, Attribute, BlockData`) with the most
stored tiles.

`Attribute` and `BlockData` have the layout of Clip Studio Paint projects, as documented by
[clipfile-rs](https://github.com/Aodaruma/clipfile-rs) (`docs/format-analysis.md`, MIT): sections
`Parameter` (size, tile grid, 16 packing words; 14 in old tips), `InitColor`, `BlockSize`; blocks
of 256×256 zlib tiles, planar: first the alpha plane, then gray or interleaved B, G, R, unused.
`BlockStatus` and `BlockCheckSum` (Adler-32 of the LE length and the zlib bytes) are missing in
old tips. Tips use five packings: 8-bit alpha, 8-bit gray + alpha, 1-bit alpha, 1-bit gray +
alpha (most significant bit first, 32 bytes per row), 8-bit alpha + BGRX.

`layer::encode` writes a database entirely in the clear, with the tables `Canvas`, `Layer`,
`Mipmap`, `MipmapInfo`, `Offscreen`.

## Open questions

- Does Clip Studio Paint open a `.sut` written by this crate, and a `.layer` stored in the clear?
- Meaning of source bit `0x100` and of the tilt range slot; the id → setting table of
  `VariantShowParam`; the layout of `BrushInOutTarget`.

## Tests

`cargo test -p photocraft-clippy-sut` runs unit tests, generated files (round trips, the tree
guards, every truncation of a `.sut` and a `.layer`, byte flips, proptest on the blob codecs) and,
with `CLIPPY_CSP_FIXTURES=<folder>`, the acceptance tests on a real installation (its
`CLIPStudioPaintVer1_5_0/Tool/EditImageTool.todb`, `Material/` and `Sut-Exporte/*.sut`; never
committed). Fuzz targets: `cargo fuzz run layer|blobs|sut` in `fuzz/`.
