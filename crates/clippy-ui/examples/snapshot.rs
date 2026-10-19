//! Render Clippy offscreen (no window, no focus stealing) and save a PNG: PhotoCraft's snapshot
//! example with Clippy's extension installed.
//!
//! ```sh
//! cargo run --release -p photocraft-clippy-ui --example snapshot -- \
//!     --out clippy.png --size 1440x900 --mode photoshop --lang de --menu Window/Interface \
//!     --script '[["ui.menu.invoke", {"id": "file.new"}]]'
//! ```
//!
//! `--mode clippy|photoshop` sets the interface first (default: the user's default, Clippy).
//! `--lang` sets the UI language (default English). `--script` is a JSON array of `[method,
//! params]` control-protocol calls (docs/control-protocol.md). `--menu A/B/…` then clicks the
//! menu title `A` and points at the submenu rows `B`, … so the open menu shows in the image.
//! `--no-extension` renders PhotoCraft itself, the baseline to compare the Photoshop mode with.

use egui_kittest::kittest::Queryable;
use photocraft_ui_egui::control::{ControlRequest, Outcome, handle};
use photocraft_ui_egui::{PhotocraftApp, Services};
use serde_json::{Value, json};

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = arg(&args, "--out").unwrap_or_else(|| "clippy.png".into());
    let (w, h) = arg(&args, "--size")
        .and_then(|s| s.split_once('x').and_then(|(a, b)| Some((a.parse::<f32>().ok()?, b.parse::<f32>().ok()?))))
        .unwrap_or((1440.0, 900.0));
    let scale: f32 = arg(&args, "--scale").and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let mode = arg(&args, "--mode");
    let lang = arg(&args, "--lang").unwrap_or_else(|| "en".into());
    let menu = arg(&args, "--menu");
    let extension = !args.iter().any(|a| a == "--no-extension");
    let script: Vec<(String, Value)> =
        arg(&args, "--script").map(|s| serde_json::from_str::<Vec<(String, Value)>>(&s).expect("--script must be [[method, params], …]")).unwrap_or_default();

    let mut harness =
        egui_kittest::Harness::builder().with_size(egui::vec2(w, h)).with_pixels_per_point(scale).with_max_steps(64).wgpu().build_eframe(move |cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
            let mut services = Services::default();
            if extension {
                photocraft_clippy_ui::install(&mut services);
            }
            let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), services);
            if let Some(rs) = cc.wgpu_render_state.as_ref() {
                app.set_wgpu(rs.clone());
            }
            app
        });
    let ctx = harness.ctx.clone();
    let mut calls = vec![("engine.execute".to_string(), json!({"id": "prefs.set", "params": {"path": "interface.language", "value": lang}}))];
    if let Some(mode) = mode {
        calls.push(("engine.execute".into(), json!({"id": "clippy.mode.set", "params": {"mode": mode}})));
    }
    calls.extend(script);
    harness.run_steps(4);
    for (method, params) in calls {
        let (req, _rx) = ControlRequest::new(&method, params);
        if let Outcome::Done(v) = handle(harness.state_mut(), &ctx, &req)
            && v.get("ok") == Some(&Value::Bool(false))
        {
            eprintln!("{method}: {v}");
        }
        harness.run_steps(4);
    }
    if let Some(menu) = menu {
        let mut path = menu.split('/');
        if let Some(title) = path.next() {
            harness.get_by_label(title).click();
            harness.run_steps(4);
        }
        for row in path {
            harness.get_all_by_label(row).last().expect("the submenu row").hover();
            harness.run_steps(6);
        }
    }
    // Let fade animations settle.
    for _ in 0..12 {
        harness.step();
    }
    let img = harness.render().expect("render");
    img.save(&out).expect("save png");
    println!("wrote {out} ({}×{})", img.width(), img.height());
}
