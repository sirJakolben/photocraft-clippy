//! The interface switch, Clippy ↔ Photoshop: the menu, the Preferences dialog, the frame and the
//! saved preference. The render tests draw the real app offscreen and compare pixels; they skip
//! when no GPU adapter is available.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use egui_kittest::kittest::Queryable;
use photocraft_engine::clippy_cmds::ClippyMode;
use photocraft_ui_egui::extension::Extension;
use photocraft_ui_egui::{PhotocraftApp, Services, menus};
use serde_json::{Value, json};

type Harness = egui_kittest::Harness<'static, PhotocraftApp>;

const SIZE: egui::Vec2 = egui::vec2(1280.0, 800.0);

#[test]
fn clippy_is_the_default_and_the_window_menu_switches() {
    let mut app = app(Some(photocraft_clippy_ui::extension()));
    assert_eq!(app.session.prefs().clippy.mode, None);
    assert_eq!(photocraft_clippy_ui::mode::current(app.session.prefs()), ClippyMode::Clippy);
    assert_eq!(checked(&app), [Some(true), Some(false)]);
    let items = interface_items(&app);
    assert!(items.iter().all(|i| i.path == ["Window", "Interface"] && i.enabled), "{items:?}");
    assert_eq!(items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(), ["Clippy", "Photoshop", "Reset Clippy Workspace"]);

    // A click on a menu item runs its id, as here.
    invoke(&mut app, "clippy.mode.photoshop");
    assert_eq!(app.session.prefs().clippy.mode, Some(ClippyMode::Photoshop));
    assert_eq!(checked(&app), [Some(false), Some(true)]);
    invoke(&mut app, "clippy.mode.clippy");
    assert_eq!(app.session.prefs().clippy.mode, Some(ClippyMode::Clippy));
    assert_eq!(checked(&app), [Some(true), Some(false)]);
}

#[test]
fn the_interface_submenu_follows_workspace() {
    let app = app(Some(photocraft_clippy_ui::extension()));
    let items = menus::menu_items(&app);
    let at = items.iter().position(|i| i.id == "clippy.mode.clippy").unwrap();
    assert_eq!(items[at - 1].path, ["Window", "Workspace"]);
    assert_eq!(items[at + 1].id, "clippy.mode.photoshop");
}

#[test]
fn photocraft_without_the_extension_has_no_clippy_items() {
    let app = app(None);
    assert!(interface_items(&app).is_empty());
    assert!(menus::menu_items(&app).iter().all(|i| !i.id.starts_with("clippy.")));
}

/// The frame calls the extension only in Clippy mode, and the switch shows in the next frame.
#[test]
fn the_frame_draws_the_extension_only_in_clippy_mode() {
    static FRAMES: AtomicUsize = AtomicUsize::new(0);
    let counting = Extension {
        ui: |app, ui, chrome| {
            FRAMES.fetch_add(1, Ordering::SeqCst);
            app.photocraft_chrome(ui, chrome);
        },
        ..photocraft_clippy_ui::extension()
    };
    let mut h = egui_kittest::Harness::builder().with_size(SIZE).with_max_steps(64).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        app(Some(counting))
    });
    h.run_steps(3);
    let drawn = FRAMES.load(Ordering::SeqCst);
    assert!(drawn > 0, "Clippy mode draws the extension");
    h.state_mut().run("clippy.mode.set", json!({"mode": "photoshop"})).unwrap();
    h.run_steps(3);
    let after = FRAMES.load(Ordering::SeqCst);
    h.run_steps(3);
    assert_eq!(FRAMES.load(Ordering::SeqCst), after, "Photoshop mode draws PhotoCraft's own interface");
    h.state_mut().run("clippy.mode.set", json!({"mode": "clippy"})).unwrap();
    h.run_steps(2);
    assert!(FRAMES.load(Ordering::SeqCst) > after, "back without a restart");
}

/// Starting in Photoshop mode shows PhotoCraft's interface pixel for pixel; Clippy mode shows its
/// docks instead. (Switching at run time can leave egui's font atlas packed in another order by
/// the strings Clippy drew, which moves single anti-aliased pixels; nothing else carries over.)
#[test]
fn photoshop_mode_renders_exactly_as_photocraft() {
    let Some(photocraft) = render(None, None) else { return };
    let photoshop = render(Some(photocraft_clippy_ui::extension()), Some("photoshop")).unwrap();
    assert!(photocraft == photoshop, "Photoshop mode differs from PhotoCraft");
    let clippy = render(Some(photocraft_clippy_ui::extension()), Some("clippy")).unwrap();
    assert!(photocraft != clippy, "Clippy mode draws its docks");
}

/// Preferences › Interface: the mode applies with OK, not before, and Cancel keeps it.
#[test]
fn preferences_switch_the_mode_on_ok() {
    for confirm in ["OK", "Cancel"] {
        let mut h = egui_kittest::Harness::builder().with_size(SIZE).with_max_steps(64).build_eframe(|cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
            app(Some(photocraft_clippy_ui::extension()))
        });
        let ctx = h.ctx.clone();
        menus::invoke(h.state_mut(), &ctx, "edit.preferences.interface", json!({})).unwrap();
        h.run_steps(4);
        h.get_by_label("photocraft-clippy");
        h.get_by_value("Clippy").click();
        h.run_steps(3);
        h.get_by_label("Photoshop").click();
        h.run_steps(3);
        assert_eq!(h.state().session.prefs().clippy.mode, None, "{confirm}: nothing applies before OK");
        h.get_by_label(confirm).click();
        h.run_steps(4);
        let expected = (confirm == "OK").then_some(ClippyMode::Photoshop);
        assert_eq!(h.state().session.prefs().clippy.mode, expected, "{confirm}");
    }
}

/// The Clippy group's explanation is translated (de.tsv).
#[test]
fn the_preferences_hint_is_translated() {
    let mut h = egui_kittest::Harness::builder().with_size(SIZE).with_max_steps(64).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = app(Some(photocraft_clippy_ui::extension()));
        app.run("prefs.set", json!({"path": "interface.language", "value": "de"})).unwrap();
        app
    });
    let ctx = h.ctx.clone();
    menus::invoke(h.state_mut(), &ctx, "edit.preferences.interface", json!({})).unwrap();
    h.run_steps(4);
    assert!(h.query_by_label_contains("Wechselt ohne Neustart").is_some());
    assert!(h.query_by_label_contains("Switches without a restart").is_none());
}

/// PhotoCraft's Preferences dialog has no Clippy rows.
#[test]
fn photocraft_preferences_have_no_clippy_rows() {
    let mut h = egui_kittest::Harness::builder().with_size(SIZE).with_max_steps(64).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        app(None)
    });
    let ctx = h.ctx.clone();
    menus::invoke(h.state_mut(), &ctx, "edit.preferences.interface", json!({})).unwrap();
    h.run_steps(4);
    assert!(h.query_by_label("photocraft-clippy").is_none());
    let values = h.state().ui.dialogs.iter().find_map(|d| d.fields.get("values").cloned()).unwrap();
    assert!(values.get("clippy").is_none(), "{values}");
}

/// The chosen mode is saved with the preferences and comes back at the next start.
#[test]
fn the_mode_survives_a_restart() {
    let store: Arc<Mutex<Option<String>>> = Arc::default();
    let running = store.clone();
    let mut h = egui_kittest::Harness::builder().with_size(SIZE).with_max_steps(64).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        app_on(&running)
    });
    invoke(h.state_mut(), "clippy.mode.photoshop");
    // The frame loop saves changed preferences.
    h.run_steps(3);
    drop(h);
    let saved: Value = serde_json::from_str(store.lock().unwrap().as_deref().unwrap()).unwrap();
    assert_eq!(saved["clippy"]["mode"], "photoshop");

    let restarted = app_on(&store);
    assert_eq!(restarted.session.prefs().clippy.mode, Some(ClippyMode::Photoshop));
    assert_eq!(checked(&restarted), [Some(false), Some(true)]);
}

/// The app in English, whatever the system's language.
fn app(extension: Option<Extension>) -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Services { extension, ..Default::default() });
    app.run("prefs.set", json!({"path": "interface.language", "value": "en"})).unwrap();
    app
}

/// Clippy whose preferences live in `store`.
fn app_on(store: &Arc<Mutex<Option<String>>>) -> PhotocraftApp {
    let (load, save) = (store.clone(), store.clone());
    let mut services = Services {
        load_prefs: Some(Box::new(move || load.lock().unwrap().clone())),
        save_prefs: Some(Box::new(move |text: &str| {
            *save.lock().unwrap() = Some(text.to_string());
            Ok(())
        })),
        ..Default::default()
    };
    photocraft_clippy_ui::install(&mut services);
    PhotocraftApp::new(photocraft_engine::Session::new(), services)
}

fn invoke(app: &mut PhotocraftApp, id: &str) {
    menus::invoke(app, &egui::Context::default(), id, json!({})).unwrap();
}

fn interface_items(app: &PhotocraftApp) -> Vec<menus::MenuItem> {
    menus::menu_items(app).into_iter().filter(|i| i.path == ["Window", "Interface"]).collect()
}

/// The checkmarks of Window › Interface › Clippy and Photoshop.
fn checked(app: &PhotocraftApp) -> [Option<bool>; 2] {
    let items = interface_items(app);
    ["clippy.mode.clippy", "clippy.mode.photoshop"].map(|id| items.iter().find(|i| i.id == id).and_then(|i| i.checked))
}

/// The window with a document open, in `mode`; `None` without a GPU adapter.
fn render(extension: Option<Extension>, mode: Option<&'static str>) -> Option<Vec<u8>> {
    let built = std::panic::catch_unwind(move || {
        egui_kittest::Harness::builder().with_size(SIZE).with_pixels_per_point(1.0).with_max_steps(64).wgpu().build_eframe(move |cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
            let mut app = app(extension);
            // Before the first frame, as when the app starts in that mode.
            if let Some(mode) = mode {
                app.run("clippy.mode.set", json!({"mode": mode})).unwrap();
            }
            if let Some(rs) = cc.wgpu_render_state.as_ref() {
                app.set_wgpu(rs.clone());
            }
            app
        })
    });
    let mut h: Harness = match built {
        Ok(h) => h,
        Err(_) => {
            eprintln!("skipping: no GPU adapter");
            return None;
        }
    };
    h.state_mut().run("file.new", json!({"width": 640, "height": 400})).unwrap();
    h.run_steps(8);
    Some(h.render().expect("render").into_raw())
}
