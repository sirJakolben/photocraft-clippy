# Clippy (PhotoCraft Extension)

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

| What                     | PhotoCraft                       | Clippy                                    |
|--------------------------|----------------------------------|-------------------------------------------|
| Binary                   | `photocraft`                     | `photocraft-clippy`                       |
| App id, window class     | `ai.storyteller.photocraft`      | `io.github.sirjakolben.PhotocraftClippy`  |
| Settings folder (Linux)  | `~/.config/photocraft`           | `~/.config/photocraft-clippy`             |
| Window title, About      | PhotoCraft                       | Clippy (PhotoCraft Extension)             |
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
