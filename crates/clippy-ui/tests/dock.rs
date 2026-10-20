//! Clippy's docks (M2): the default layout on a first start, dragging a palette, saving the layout
//! across a restart, an unreadable saved layout, the workspace reset, and painting on the canvas
//! inside the dock. The app runs offscreen without a GPU (the canvas falls back to the CPU).

use std::sync::{Arc, Mutex};

use craft_ui::docking::{Action, Placement, Zone};
use egui_kittest::kittest::Queryable;
use photocraft_clippy_ui::dock::{self, layout, panels::PanelId};
use photocraft_ui_egui::control::{ControlRequest, handle};
use photocraft_ui_egui::{PhotocraftApp, Services, menus};
use serde_json::{Value, json};

type Harness = egui_kittest::Harness<'static, PhotocraftApp>;

const SIZE: egui::Vec2 = egui::vec2(1280.0, 800.0);

#[test]
fn a_first_start_shows_the_default_layout() {
    let mut h = harness(Arc::default());
    h.run_steps(4);
    for title in ["Sub Tool", "Tool Property", "Swatches", "Navigator", "Layers", "Properties", "Tool"] {
        assert!(shown(&h, title), "tab {title}");
    }
    // Brushes is PhotoCraft's preset panel; Clippy's sub tools replace it (docs: architektur G2 rule 7).
    assert!(!shown(&h, "Brushes"));
    assert_eq!(dock::saved(h.state()), Value::Null, "nothing saved until the layout changes");
    assert_eq!(dock::current(h.state()), layout::default_layout());
}

#[test]
fn a_tab_dragged_below_the_canvas_makes_a_bottom_dock_and_is_saved() {
    let mut h = harness(Arc::default());
    h.run_steps(4);
    let tab = top(&h, "Info").center();
    let canvas = canvas_rect(&h);
    let target = egui::pos2(canvas.center().x, canvas.bottom() - 12.0);
    drag(&mut h, tab, target);
    let layout = dock::current(h.state());
    assert!(layout::sides(&layout).contains(&Zone::Bottom), "{layout:?}");
    assert!(layout.panels().contains(&&PanelId::Info));
    assert_ne!(dock::saved(h.state()), Value::Null, "saved on release");
}

#[test]
fn a_tab_dropped_beside_a_side_dock_group_is_refused() {
    let mut l = layout::default_layout();
    let before = l.clone();
    let beside = Action::Move { panel: PanelId::Info, anchor: PanelId::Layer, placement: Placement::Split(Zone::Left) };
    assert!(!dock::apply(&mut l, beside, false));
    assert_eq!(l, before);
    let below = Action::Move { panel: PanelId::Info, anchor: PanelId::Layer, placement: Placement::Split(Zone::Bottom) };
    assert!(dock::apply(&mut l, below, false));
    // A locked workspace moves nothing; the canvas never moves.
    let mut locked = layout::default_layout();
    assert!(!dock::apply(&mut locked, Action::Move { panel: PanelId::Info, anchor: PanelId::Layer, placement: Placement::Tab { before: None } }, true));
    assert!(!dock::apply(&mut locked, Action::Float { panel: PanelId::Canvas, rect: [10.0, 10.0, 300.0, 300.0] }, false));
    assert!(!dock::apply(&mut locked, Action::Close { panel: PanelId::Canvas }, false));
}

#[test]
fn a_floating_palette_and_the_layout_survive_a_restart() {
    let store: Arc<Mutex<Option<String>>> = Arc::default();
    let mut h = harness(store.clone());
    h.run_steps(2);
    let mut l = layout::default_layout();
    assert!(dock::apply(&mut l, Action::Float { panel: PanelId::Navigator, rect: [400.0, 200.0, 260.0, 220.0] }, false));
    h.state_mut().run("clippy.layout.set", json!({"layout": serde_json::to_value(&l).unwrap()})).unwrap();
    h.run_steps(4);
    assert!(store.lock().unwrap().as_deref().is_some_and(|t| t.contains("\"navigator\"")), "preferences written");
    drop(h);

    let mut again = harness(store);
    again.run_steps(4);
    assert_eq!(dock::current(again.state()), l);
    assert_eq!(again.state().session.prefs().clippy.layouts["clippy"]["floating"][0]["panels"], json!(["navigator"]));
    assert!(shown(&again, "Navigator"), "the floating window shows");
}

#[test]
fn an_unreadable_layout_shows_the_default_with_a_note() {
    let mut h = harness(Arc::default());
    h.state_mut().run("clippy.layout.set", json!({"layout": {"root": {"Tabs": {"panels": ["layer"], "active": 0}}, "floating": []}})).unwrap();
    h.run_steps(3);
    assert!(h.state().ui.status.contains("default layout"), "{}", h.state().ui.status);
    assert!(shown(&h, "Layers"));
    assert_eq!(dock::current(h.state()), layout::default_layout());
}

#[test]
fn reset_clippy_workspace_restores_the_default() {
    let mut h = harness(Arc::default());
    let mut l = layout::default_layout();
    assert!(dock::apply(&mut l, Action::Close { panel: PanelId::Layer }, false));
    h.state_mut().run("clippy.layout.set", json!({"layout": serde_json::to_value(&l).unwrap()})).unwrap();
    h.run_steps(3);
    assert!(!shown(&h, "Layers"));

    let items = menus::menu_items(h.state());
    let reset = items.iter().find(|i| i.id == "clippy.layout.reset").expect("menu item");
    assert_eq!(reset.path, ["Window", "Interface"]);
    assert!(reset.enabled);
    let ctx = h.ctx.clone();
    menus::invoke(h.state_mut(), &ctx, "clippy.layout.reset", json!({})).unwrap();
    h.run_steps(3);
    assert!(shown(&h, "Layers"));
    assert_eq!(dock::current(h.state()), layout::default_layout());
}

#[test]
fn the_brush_paints_on_the_canvas_in_the_dock() {
    let mut h = harness(Arc::default());
    h.state_mut().run("file.new", json!({"width": 400, "height": 300, "background": "white"})).unwrap();
    h.run_steps(3);
    control(&mut h, "ui.set", json!({"tool": "brush"}));
    assert!(!menus::is_enabled(h.state(), "history.undo"));
    let canvas = canvas_rect(&h);
    drag(&mut h, canvas.center() - egui::vec2(60.0, 0.0), canvas.center() + egui::vec2(60.0, 20.0));
    assert!(menus::is_enabled(h.state(), "history.undo"), "the stroke is a history step");
}

#[test]
fn photoshop_mode_keeps_photocraft_s_dock() {
    let mut h = harness(Arc::default());
    h.state_mut().run("clippy.mode.set", json!({"mode": "photoshop"})).unwrap();
    h.run_steps(4);
    assert!(!shown(&h, "Sub Tool"));
    assert!(shown(&h, "Layers"));
}

/// Clippy whose preferences live in `store`, in English.
fn harness(store: Arc<Mutex<Option<String>>>) -> Harness {
    egui_kittest::Harness::builder().with_size(SIZE).with_max_steps(64).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
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
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), services);
        app.run("prefs.set", json!({"path": "interface.language", "value": "en"})).unwrap();
        app
    })
}

fn shown(h: &Harness, label: &str) -> bool {
    h.query_all_by_label(label).next().is_some()
}

/// The topmost node labelled `label` (a tab, above its palette's own heading).
fn top(h: &Harness, label: &str) -> egui::Rect {
    h.query_all_by_label(label).map(|n| n.rect()).min_by(|a, b| a.top().total_cmp(&b.top())).expect(label)
}

fn control(h: &mut Harness, method: &str, params: Value) {
    let ctx = h.ctx.clone();
    let (req, _rx) = ControlRequest::new(method, params);
    handle(h.state_mut(), &ctx, &req);
    h.run_steps(2);
}

/// The canvas: the space between the default layout's left (280 pt) and right (260 pt) docks,
/// below the document tabs.
fn canvas_rect(h: &Harness) -> egui::Rect {
    let screen = h.ctx.content_rect();
    let top = top(h, "Sub Tool").bottom() + 40.0;
    egui::Rect::from_min_max(egui::pos2(screen.left() + 290.0, top), egui::pos2(screen.right() - 270.0, screen.bottom() - 40.0))
}

/// Press at `from`, move to `to` in steps, release: a real pointer drag.
fn drag(h: &mut Harness, from: egui::Pos2, to: egui::Pos2) {
    let button = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
    h.event(egui::Event::PointerMoved(from));
    h.run_steps(1);
    h.event(button(from, true));
    h.run_steps(1);
    for i in 1..=12 {
        h.event(egui::Event::PointerMoved(from + (to - from) * (i as f32 / 12.0)));
        h.run_steps(1);
    }
    h.event(button(to, false));
    h.run_steps(4);
}
