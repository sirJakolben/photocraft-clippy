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
| `crates/clippy-stock`                 | New (L4): the sub tool stock on disk (empty until M7)                         |
| `crates/engine/src/clippy_cmds.rs`    | New: `ClippyPrefs`, commands `clippy.mode.set/clippy/photoshop`               |
| `crates/engine/src/{lib,commands}.rs` | Module and `v.extend(clippy_cmds::specs())`                                   |
| `crates/engine/src/prefs.rs`          | Field `Preferences::clippy`; unused keys in `HIDDEN_UNTIL_IMPLEMENTED`        |
| `crates/ui-egui/src/extension.rs`     | New: `Extension` (frame, menu items, Preferences rows)                        |
| `crates/ui-egui/src/lib.rs`           | `Services::extension`; `fn ui` calls it or `photocraft_chrome`                |
| `crates/ui-egui/src/menus.rs`         | `menu_items` adds the extension's items                                       |
| `crates/ui-egui/src/prefs_ui.rs`      | Working copy with the extension's paths; its rows on Interface                |
| `xtask/src/layers.rs`                 | Layers of the new crates; `ui-egui` → `clippy-ui` within L6                   |
| `apps/photocraft`                     | Feature `clippy` depends on `photocraft-clippy-ui`; `install()` in `main.rs`  |

Snapshot of either mode (`--no-extension` renders PhotoCraft itself):

```sh
cargo run -p photocraft-clippy-ui --example snapshot -- --out ui.png --mode photoshop --lang de
```

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
