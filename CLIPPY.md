# photocraft-clippy

Clippy is a fork of [PhotoCraft](https://github.com/storytold/photocraft) that adds a Clip Studio
Paint–style interface: docking palettes on every window edge, tools holding sub-tool groups the
user arranges freely, and Clip Studio Paint sub-tool files (`.sut`) as the native brush format.
PhotoCraft's own interface stays available, and PhotoCraft built from this repository behaves
exactly like upstream.

Read `AGENTS.md` first: every PhotoCraft rule (never crash, everything is a command, layering,
wasm, tests) applies to Clippy code too. This file only adds what is specific to the fork.

## Build and run

```sh
cargo build --release -p photocraft --features clippy --bin photocraft-clippy
./target/release/photocraft-clippy
```

`cargo build -p photocraft` (no feature) still builds PhotoCraft itself. Cargo warns that
`src/main.rs` is in two build targets: both binaries are that one program, and the `clippy` feature
picks the identity.

## Identity

Clippy and an installed PhotoCraft run side by side for one user without sharing state:

| What                     | PhotoCraft                       | photocraft-clippy                         |
|--------------------------|----------------------------------|-------------------------------------------|
| Binary                   | `photocraft`                     | `photocraft-clippy`                       |
| App id, window class     | `ai.storyteller.photocraft`      | `io.github.sirjakolben.PhotocraftClippy`  |
| Settings folder (Linux)  | `~/.config/photocraft`           | `~/.config/photocraft-clippy`             |
| Window title, About      | PhotoCraft                       | photocraft-clippy                         |
| Icon                     | `assets/app-icon/`               | `assets/app-icon-clippy/`                 |
| Desktop entry            | `packaging/linux/ai.storyteller.photocraft.desktop` | `packaging/linux/io.github.sirjakolben.PhotocraftClippy.desktop` |

The settings folder also separates the single-instance lock, logs and crash recovery. On its first
start Clippy copies PhotoCraft's `preferences.json` (shortcuts, tool options), then keeps its own.

Where the identity is decided (the fork's hook points into upstream files; keep them small):

| File                                   | Hook                                                              |
|----------------------------------------|-------------------------------------------------------------------|
| `apps/photocraft/src/identity.rs`      | New: app id, name, binary, settings folder; first-start copy      |
| `apps/photocraft/Cargo.toml`           | `clippy` feature, both `[[bin]]` targets                          |
| `apps/photocraft/src/main.rs`          | `APP_ID`, window title, eframe name, `identity::install()`        |
| `apps/photocraft/src/app_dirs.rs`      | Settings folder names via `identity::CONFIG_DIRS`                 |
| `apps/photocraft/src/app_icon.rs`      | Window icon                                                       |
| `apps/photocraft/src/services.rs`      | XWayland relaunch starts `identity::BIN`                          |
| `crates/ui-egui/src/identity.rs`       | New: UI name, About text and links, hidden Help items, title icon |
| `crates/ui-egui/src/{panels,dialogs,links,menus,brand,canvas}.rs` | Read `identity::get()` instead of fixed PhotoCraft values |
| `crates/ui-egui/src/i18n/de.tsv`       | German strings of the fork, in one block at the end               |

## Interface: Clippy ↔ Photoshop

Clippy shows either its own interface or PhotoCraft's (Window › Interface, or Preferences ›
Interface). The choice is the preference `clippy.mode` (`null` until chosen, which means Clippy),
set by `clippy.mode.set {"mode": "clippy"|"photoshop"}`; it switches at once and survives a
restart. PhotoCraft registers the `clippy.*` commands but shows them nowhere and never reads the
preference. Until the docks (M2), Clippy mode draws PhotoCraft's panels as a placeholder.

`photocraft-ui-egui` never names Clippy: the app binary installs an extension in its `Services`,
and without one every frame, menu and Preferences page is PhotoCraft's own.

| File                                  | Hook                                                                          |
|---------------------------------------|-------------------------------------------------------------------------------|
| `crates/clippy-ui`                    | New (L6): Clippy's interface, an `Extension` of ui-egui; `mode.rs` the switch |
| `crates/clippy-stock`                 | New (L4): the sub tool stock on disk (see Sub tool stock)                     |
| `crates/engine/src/clippy_cmds.rs`    | New: `ClippyPrefs`, commands `clippy.mode.set/clippy/photoshop`               |
| `crates/engine/src/{lib,commands}.rs` | Module and `v.extend(clippy_cmds::specs())`                                   |
| `crates/engine/src/prefs.rs`          | Field `Preferences::clippy`; unused keys in `HIDDEN_UNTIL_IMPLEMENTED`        |
| `crates/ui-egui/src/extension.rs`     | New: `Extension` (frame, menu items, Preferences rows, own menu ids)          |
| `crates/ui-egui/src/lib.rs`           | `Services::extension`; `fn ui` calls it or `photocraft_chrome`                |
| `crates/ui-egui/src/menus.rs`         | `menu_items` adds the extension's items; `invoke` runs its ids                |
| `crates/ui-egui/src/prefs_ui.rs`      | Working copy with the extension's paths; its rows on Interface                |
| `xtask/src/layers.rs`                 | Layers of the new crates; `ui-egui` → `clippy-ui` within L6                   |
| `apps/photocraft`                     | Feature `clippy` depends on `photocraft-clippy-ui`; `install()` in `main.rs`  |

Snapshot of either mode (`--no-extension` renders PhotoCraft itself):

```sh
cargo run -p photocraft-clippy-ui --example snapshot -- --out ui.png --mode photoshop --lang de
```

## Docks

In Clippy mode the window below the menu and option bars is one `craft_ui::docking` layout: the
canvas, protected, in the middle and a dock on any side of it. A dock is a column (left, right)
or a row (top, bottom) of tab groups; groups inside it only stack along it, and each side of the
canvas has one dock at most. Palettes move by dragging their tab, float as windows inside the
main window, and their sizes drag at the dividers.

The layout is the preference `clippy.layouts.clippy`, written only by `clippy.layout.set
{"layout": …}`, `clippy.layout.reset` (Window › Interface › Reset Clippy Workspace) and
`clippy.workspace.load`; null is the default from `crates/clippy-ui/assets/default-layout.json`.
An unreadable saved layout shows the default with a note in the status bar. Palette ids, titles,
icons and bodies are in `crates/clippy-ui/src/dock/panels.rs`; PhotoCraft's panels draw through
`extension::photocraft_panel`. Until Clippy's own palettes exist, the Tool palette shows
PhotoCraft's tool column and the other new palettes (Sub Tool, Tool Property, colour palettes)
show a note.

Around craft_ui's dock area (M3, `crates/clippy-ui/src/dock`):

- Tabs (`header.rs`): an inactive tab is its palette's icon, the active one icon and name; every
  tab has its name as tooltip and accessibility label. craft_ui draws the tab text with room left
  for the icon, which `header::decorate` paints over the tab afterwards on the tab's own layer.
- ☰ palette menu at the left of every group's header (inside its first tab): Close Palette,
  Collapse Area, Float Palette, Reset Clippy Workspace. A tab's context menu gains Collapse Area.
- Icon rail (`rail.rs`): the chevrons at the right of a dock's first header, Collapse Area or a
  double click on a tab strip collapse the whole dock to a 36 pt rail of icons along the window
  edge. An icon opens its palette as an overlay beside the rail (closed by Esc, a click elsewhere
  or the icon); the rail's first button expands the dock. The saved layout lists collapsed sides
  under `collapsed` (`["Left"]`) beside craft_ui's `root` and `floating`, so builds before M3 read
  past it. The frame draws a view without the collapsed docks and maps its split paths back.
- Drag rules (G10): where a drop would break the dock rules, the pointer is a blocked rect for
  craft_ui, so no drop preview shows and the release drops nothing (`dock::drop_refused`). Esc
  cancels a drag (craft_ui). Palettes are at least 160 pt wide (`panels::MIN_WIDTH`).
- Window: one item per palette (`clippy.palette.<id>`, checked while shown, rail included) in
  place of PhotoCraft's panel items. Choosing a shown palette closes it; a closed one comes back
  as a tab beside a palette of its default group, else floating in the middle of the window.
  Disabled while the workspace is locked.
- Workspaces (`workspace.rs`): Window › Workspace lists the saved workspaces (a click loads one,
  menu id `clippy.workspace.load/<name>`), then New Workspace… and Delete Workspace…, which ask
  for a name, then PhotoCraft's Keyboard Shortcuts… and Lock Workspace. PhotoCraft's workspaces
  (Essentials, …) arrange its own panels and are left out in Clippy mode. The engine commands
  `clippy.workspace.save {"name", "overwrite"?}`, `.load {"name"}`, `.delete {"name"}` and
  `.list` keep one `<name>.json` per workspace (`{"format": "photocraft-clippy-workspace",
  "version": 1, "name", "layout"}`) in `<settings folder>/workspaces`
  (`~/.config/photocraft-clippy/workspaces` on Linux); a session without a folder (headless, tests,
  the web) keeps them in memory. Names are 1–64 characters without `/ \ : * ? " < > |`, control
  characters or a dot at either end; they match regardless of case. A loaded workspace is the
  current layout, so it is the one shown after a restart.

Menu ids that aren't engine commands (`clippy.palette.*`, `clippy.workspace.load/*`, and
`clippy.workspace.save` / `.delete` without a name, which open their dialog) run through
`Extension::invoke`, only in Clippy mode; everything they change goes through `clippy.layout.set`
or `clippy.workspace.*`.

| File                                  | Hook                                                                  |
|---------------------------------------|-----------------------------------------------------------------------|
| `crates/ui-egui/src/extension.rs`     | `photocraft_panel`: a PhotoCraft dock panel's body in another dock; `Extension::invoke` |
| `crates/ui-egui/src/menus.rs`         | `invoke_unguarded` asks `Extension::invoke` first                     |
| `crates/engine/src/clippy_cmds.rs`    | `clippy.layout.set`, `clippy.layout.reset`, `clippy.workspace.*`, `ClippyState` |
| `crates/engine/src/lib.rs`            | `Session::clippy` (the workspace folder)                              |
| `crates/engine/src/prefs.rs`          | `clippy.layouts` no longer hidden                                     |
| `apps/photocraft/src/main.rs`         | Feature `clippy` sets `session.clippy.workspace_dir`                  |

## Brush engine additions

`crates/paint` carries the brush options Clip Studio Paint sub tools need. Every one is a serde
field whose default is PhotoCraft's old behaviour and which is left out of JSON until set, so
saved brushes, presets and `.abr` imports paint and save exactly as upstream.

| Clip Studio Paint                         | Field                                                     |
|-------------------------------------------|-----------------------------------------------------------|
| Velocity pen setting                      | `Control::Velocity`                                       |
| Several pen settings on one value         | `Dynamic.controls` (up to three `Influence`s)             |
| Anti-aliasing steps                       | `BrushSettings.aa_level`                                  |
| At least 1 pixel, sync size with view     | `min_one_px`, `size_follows_view`                         |
| Starting and ending                       | `taper`                                                   |
| Several tips and their order              | `tips`, `tip_order`                                       |
| Blend brush strokes with darken           | `blend_darken`                                            |
| Texture rotation, dual brush texture      | `texture.rotation`, `dual_brush.texture`                  |
| Watercolor edge                           | `wet_edge`                                                |
| Stabilization by speed, stickiness        | `smoothing.by_speed`, `smoothing.stickiness`              |

`crates/paint/src/golden_tests.rs` pins a digest of the dabs and pixels of strokes through every
brush section and built-in preset, recorded on the upstream engine: if it moves, an old brush
paints differently. Behaviour tests are in `crates/paint/src/csp_tests.rs`.

## Sub tool stock

Clippy's tools, sub tool groups and sub tools are folders and files (crate
`photocraft-clippy-stock`, its README has the details):

```text
<stock>/<Tool>/<Group>/<Sub tool>.sut     Clip Studio Paint's own sub tool format
<stock>[/<Tool>[/<Group>]]/_clippy.json   display name, icon, shortcut, order of the entries
```

The stock folder is the preference `clippy.stockDir` (an absolute path, chosen freely); empty means
`stock` in the settings folder, which the app names through `clippy_cmds::set_settings_dir`.
What a file manager does there shows in Clippy without a restart: `LiveStock` watches the folder
(200 ms batches) and keeps the selected sub tool selected when it moves. Missing metadata means
folder names, the default icon and alphabetical order (entries not listed come last); a broken
`.sut` is an entry with a warning. Moves and imports copy files byte for byte, a rename changes
only the name inside the `.sut`, a copy only its node UUID, so settings PhotoCraft doesn't use
stay as they were. Deletions go to the trash. A `.todb` is only read, never written.

The engine commands change the stock on disk and keep no state (stock paths are `Tool`,
`Tool/Group`, `Tool/Group/Sub tool`, by folder and file names):

| Command                                   | What it does                                                        |
|-------------------------------------------|---------------------------------------------------------------------|
| `clippy.stock.list`                       | The stock in display order, with warnings                           |
| `clippy.stock.tool.create`, `group.create`| New folder, last; a clash gets a number                             |
| `clippy.stock.rename`, `delete`           | Rename (a clash is an error); move to the trash                     |
| `clippy.stock.move`, `copy`               | Into a folder at an index; copies are independent ("G-Pen 2")       |
| `clippy.stock.sort`, `meta.set`           | Order of a folder; display name, icon, shortcut                     |
| `clippy.stock.import`                     | `.sut` into a group; `.todb` with its material folder; `.abr` as a new group of the brush tool |
| `clippy.stock.export`, `saveAs`           | Copy of the stock or one entry; Save Brushes As (works there after) |

Open: the app does not call `set_settings_dir` yet (without it a stock folder must be chosen),
nothing in the interface shows the stock (M9), and whether Clip Studio Paint opens the `.sut`
files Clippy writes for `.abr` brushes and with packed materials is untested.

| File                                  | Hook                                                                  |
|---------------------------------------|-----------------------------------------------------------------------|
| `crates/clippy-stock`                 | The stock: loading, operations, import, export, watcher               |
| `crates/engine/Cargo.toml`            | Depends on `photocraft-clippy-stock`                                  |
| `crates/engine/src/clippy_cmds.rs`    | `clippy.stock.*`, `stock_dir`, `set_settings_dir`                     |
| `crates/engine/src/prefs.rs`          | `clippy.stockDir` no longer hidden                                    |

## Sub tools: mapping and data flow

A Clip Studio Paint sub tool (`.sut`, read and written by `crates/clippy-sut`) becomes PhotoCraft's
tool, brush and tool options through `crates/clippy-ui/src/mapping` (M8). Photoshop mode never
reads or writes a `.sut`.

**Data flow** (`crates/clippy-ui/src/subtool.rs`):

1. The active sub tool's `.sut` is the source of truth; PhotoCraft's tool, brush and tool
   options are its working copy. Its path is the preference `clippy.subtool`.
2. `clippy.subtool.select {"path"}` reads the `.sut`, selects its tool as the tool column does
   (`extension::select_tool`: `tool_memory::select`, then the tool's own brush), sets the brush
   and the tool options it maps, and calls `brush_changed()`.
3. Every frame in Clippy mode, once the pointer is up, a moved `brush_rev` or a changed tool
   option runs `clippy.subtool.writeBack`: only the rules whose PhotoCraft fields changed are
   written, column by column by name, a column that already reads as the new value keeps its
   bytes, nothing is added, the rest of the file stays as it is; saved atomically (temporary file,
   rename). Only while the sub tool's tool (and its brush) is the current one.
4. So changes through commands, MCP, the control channel and the options bar land in the sub
   tool the same way.
5. In Photoshop mode the `clippy.subtool.*` commands are refused and the frame does nothing;
   switching back reloads the active sub tool from its file.
6. `clippy.subtool.reset` copies the initial settings (`NodeInitVariantID`) over the current
   ones; `clippy.subtool.saveAsDefault` copies the current ones into the initial settings.
7. PhotoCraft's brush presets are hidden in Clippy mode (the Brush Preset picker's list is empty).
8. Columns without a PhotoCraft counterpart stay in the `.sut` and are not shown
   (`clippy.subtool.get` lists them as `kept`).

`clippy.subtool.get` reports the active sub tool, `clippy.subtool.export {"path","name"?}` writes
PhotoCraft's current tool and brush as a new `.sut` (`mapping::reverse`, for building the default
stock). The engine registers every `clippy.subtool.*` command (listed for agents and MCP) but runs
none: without Clippy's interface they report that they aren't available. For automation they
count as file access (`automation::workspace`, a `path` is refused).

**Tools** (`mapping/tool.rs`, In/Out = `NodeInputOp`/`NodeOutputOp`):

| In/Out | PhotoCraft |
|---|---|
| 1/1, 2/2, 3/3 | Zoom, Hand, Rotate View |
| 10/10 | Brush; colour mixing (amount or density of paint below 100 %) → Mixer Brush; named Weichzeichnen/blur → Blur; Verwischen/smudge → Smudge |
| 11/11 | Brush; `AntiAlias` 0 → Pencil |
| 12/12, 13/13 | Eraser (erase all layers: one layer), Clone Stamp |
| 14/14 | Brush with fixed spacing |
| 10/20 | Brush in Quick Mask (selection pen) |
| 30/20, 31/20, 33/20, 50/20 | Rectangular/Elliptical Marquee (by name), Lasso, Polygonal Lasso, Magic Wand |
| 50/50, 52/52, 41/41, 70/70 | Paint Bucket, Gradient, Eyedropper, Type |
| 40/40, 34/42, 45/45 | Move; with Auto-Select (layer); with transform controls |
| 30/30, 32/32 | Rectangle, Ellipse, Polygon (by name), Line |
| everything else | Unsupported: selected with a notice, PhotoCraft's tool stays (lasso fill 31/31 and liquify 120/120 are still to build) |

**Brush columns** (`mapping/brush.rs`, one rule each; units are assumptions until checked with
`CLIPPY_CSP_FIXTURES`): `BrushSize` (+ `BrushSizeUnit`: px, mm, cm, inch, pt, Q at the document's
resolution) and `BrushSizeEffector` → `size`, `shape_dynamics.size`; `BrushAtLeast1Pixel`,
`BrushSizeSyncViewScale` → `min_one_px`, `size_follows_view`; `Opacity` (%) and its effector →
`opacity`, `transfer.opacity`; `CompositeMode` → `mode` (`mapping/blend.rs`; Glow Dodge and Add
(Glow) read as Color Dodge and Linear Dodge and stay as stored); `BrushMix*` → `mixer`
(approximation); `BrushHue/Saturation/ValueChange`, `BrushSubColor`, `BrushChangeStrokeColor` →
`color_dynamics`; `AntiAlias` 0–3 → `aa_level`; `BrushUsePatternImage`, `BrushPatternImageArray`,
`BrushPatternOrderType` → `tip`, `tips`, `tip_order` (tips from the embedded materials, read only);
`BrushHardness`; `BrushThickness` (+ effector) → `roundness`; `BrushRotation`,
`BrushVerticalThicknes` (+90°), `BrushRotationEffector`, `BrushRotationRandomScale` → `angle`,
`shape_dynamics.angle`; `BrushPatternReverseHorizontal/Vertical` → `flip_x/y`; `BrushFlow` (+
effector) → `flow`, `transfer.flow`; `BrushUseSpray`, `BrushSpraySize/Density` (+ effector) →
`scattering` (approximation); `BrushInterval`, `BrushAutoIntervalType` → `spacing`;
`BrushContinuousPlot` → `build_up`; `BrushBlendPatternByDarken` → `blend_darken`; `Texture…` →
`texture` (pattern read only); `UseDualBrush`, `Dual…` → `dual_brush` subset; `BrushUseWaterEdge`,
`BrushWaterEdgeRadius/AlphaPower` → `wet_edges`, `wet_edge`; `FlickerReduction`,
`FlickerReductionBySpeed(Type)`, `Stickness` → `smoothing`; `BrushEnableSnap` → View › Snap;
`BrushUseIn/Out`, `BrushIn/OutLength`, `BrushInOutBySpeed` → `taper`. Pen settings
(`…Effector`, `mapping/effector.rs`): the first active of pressure, tilt and velocity is the
dynamic's control (minimum, curve), the others its further `controls`, random its jitter. Kept in
the `.sut` only: ribbon, vector eraser, erase all layers, correction, sharpen corner, vector
magnet, refer layer, `BrushAdjustVelocity`, `BrushAdjustFlowByInterval`, the start/end ratios and
targets, the interval effector, texture emphasis, spray bias, the other water edge powers.

**Other tools** (`mapping/options.rs`): `BrushBlur` → Blur strength; `Opacity`, `AntiAlias` of the
fill → fill opacity, anti-alias; selection mode, anti-alias, tolerance, contiguous, sample all
layers, font, size, style, alignment and anti-aliasing of text, the eyedropper's sample and the
move tool's auto select map to the existing `ToolOptions`. Their column names are unverified
guesses (a name a file doesn't have maps nothing and is never written).

| File                                  | Hook                                                                  |
|---------------------------------------|-----------------------------------------------------------------------|
| `crates/clippy-ui/src/{mapping,subtool.rs}` | New: the mapping and the data flow                              |
| `crates/engine/src/clippy_cmds.rs`    | `clippy.subtool.*` specs, `ClippyPrefs::subtool`                      |
| `crates/ui-egui/src/extension.rs`     | `Extension::{command, frame, hides_brush_presets}`, `select_tool`, `PhotocraftApp::extension_data` |
| `crates/ui-egui/src/lib.rs`           | Field `extension_data`; `run_command` asks the extension first; the frame calls its `frame` |
| `crates/ui-egui/src/paint_mouse.rs`   | The Brush Preset picker shows no presets when the extension hides them |
| `crates/automation/src/workspace.rs`  | `clippy.subtool.select/export` with a `path` count as file access     |

Tests: `cargo test -p photocraft-clippy-ui --test mapping --test subtool` (synthetic `.sut` only;
with `CLIPPY_CSP_FIXTURES` also every real export under `Sut-Exporte`).

## Trademarks

As `docs/brand/LICENSE-brand.txt` requires of forks, the ArtCraft logo files are removed; only
their terms remain. Clippy never presents itself as an ArtCraft product. Clip Studio Paint material
(sub tools, brush tips, icons, screenshots) is never committed: users import their own.

## Baseline

Upstream base: `d95fdf31` (2026-10-18). Measured on the unmodified base (Fedora 44, 20 threads):

| Step                                   | Result                                                         |
|----------------------------------------|----------------------------------------------------------------|
| `cargo build --release -p photocraft`  | ok, 296 s                                                      |
| `cargo test --workspace --no-fail-fast`| 959 s; 9418 passed, 3 failed, 75 ignored                       |

The three failures are environmental and also fail upstream on this machine:
`crates/gpu/tests/timing.rs` `a_refresh_that_submits_several_times_is_one_measurement` (GPU timing)
and `crates/io/tests/svg.rs` `text_becomes_outline_shapes_or_warns_without_a_font`,
`transformed_text_lands_where_resvg_draws_it` (no `CRAFT_FONTS_DIR`).

## Following upstream

```sh
git fetch upstream
git merge upstream/main          # or a release tag
cargo test --workspace           # compare with the baseline above
```

Conflicts should only appear at the hook points listed above. `upstream` has pushing disabled.
