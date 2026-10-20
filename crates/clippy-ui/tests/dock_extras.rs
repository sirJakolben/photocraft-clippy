//! Clippy's dock extras (M3), one test per interaction: icon tabs (D5), the ☰ palette menu (D6),
//! the icon rail (D8), workspaces (D9), Window's palette list, and the drag rules of G10 (drop
//! preview, Esc, minimum width, double click). The app runs offscreen without a GPU.

use std::sync::{Arc, Mutex};

use craft_ui::docking::{Node, Zone};
use craft_ui::layout::SplitSize;
use egui_kittest::kittest::Queryable;
use photocraft_clippy_ui::dock::{self, layout, panels::PanelId, rail};
use photocraft_ui_egui::{PhotocraftApp, Services, menus};
use serde_json::{Value, json};

type Harness = egui_kittest::Harness<'static, PhotocraftApp>;

const SIZE: egui::Vec2 = egui::vec2(1280.0, 800.0);

/// D5: an inactive tab is just its icon, the active one icon and name; the name is the tooltip.
#[test]
fn inactive_tabs_show_their_icon_and_the_active_one_its_name() {
    let mut h = harness(&Setup::default());
    h.run_steps(4);
    // Swatches is the active tab of the four colour palettes in the left dock.
    let active = top(&h, "Swatches");
    let inactive = ["Intermediate Color", "Color History", "Color Mixing"].map(|l| top(&h, l));
    for tab in inactive {
        assert!(tab.width() < 48.0, "an icon tab: {tab:?}");
        assert!((tab.top() - active.top()).abs() < 0.5, "one strip");
        assert!(tab.left() >= active.right() - 0.5, "no overlap: {tab:?} after {active:?}");
    }
    assert!(active.width() > 90.0, "icon and name: {active:?}");
    assert!(inactive[2].right() <= 280.0, "all four fit in the 280 pt dock");

    // Hovering an icon tab shows its name.
    let before = h.query_all_by_label("Color History").count();
    h.hover_at(inactive[1].center());
    h.run_steps(8);
    assert!(h.query_all_by_label("Color History").count() > before, "tooltip");

    // A click makes it the active tab, wide with its name.
    click_at(&mut h, inactive[1].center());
    assert!(top(&h, "Color History").width() > 90.0);
    // The first tab also holds the ☰ button.
    assert!(top(&h, "Swatches").width() < 70.0);
}

/// D6: the ☰ menu at the left of a palette header closes, collapses, floats and resets.
#[test]
fn the_palette_menu_closes_collapses_floats_and_resets() {
    let mut h = harness(&Setup::default());
    h.run_steps(4);
    let layers = top(&h, "Layers");
    let menu = palette_menu(&h, "Layers");
    assert!(menu.left() >= layers.left() - 0.5 && menu.right() <= layers.left() + 40.0, "at the left of the header: {menu:?} {layers:?}");

    click_at(&mut h, menu.center());
    h.get_by_label("Close Palette").click();
    h.run_steps(4);
    assert!(!dock::current(h.state()).contains(&PanelId::Layer));
    assert!(!dock::saved(h.state()).is_null(), "saved");

    let menu = palette_menu(&h, "Navigator");
    click_at(&mut h, menu.center());
    h.get_by_label("Float Palette").click();
    h.run_steps(4);
    assert_eq!(dock::current(h.state()).floating.first().map(|f| f.panels.clone()), Some(vec![PanelId::Navigator]));

    let menu = palette_menu(&h, "Sub Tool");
    click_at(&mut h, menu.center());
    let collapse = entry(&h, "Collapse Area", menu);
    click_at(&mut h, collapse);
    h.run_steps(4);
    assert_eq!(dock::collapsed(h.state()), [Zone::Left]);

    let menu = palette_menu(&h, "Properties");
    click_at(&mut h, menu.center());
    h.get_by_label("Reset Clippy Workspace").click();
    h.run_steps(4);
    assert_eq!(dock::saved(h.state()), Value::Null);
    assert_eq!(dock::current(h.state()), layout::default_layout());
    assert!(dock::collapsed(h.state()).is_empty());
}

/// D8: « collapses a dock to an icon rail; an icon opens its palette beside the rail, Esc closes
/// it; the expand button brings the dock back. The collapsed dock is saved.
#[test]
fn a_dock_collapses_to_a_rail_whose_icons_open_their_palette() {
    let setup = Setup::default();
    let mut h = harness(&setup);
    h.state_mut().run("file.new", json!({"width": 400, "height": 300, "background": "white"})).unwrap();
    h.run_steps(4);
    assert!(shown(&h, "Background"), "the Layers palette lists the layer");
    let collapse = h.query_all_by_label("Collapse Area").map(|n| n.rect()).find(|r| r.left() > SIZE.x * 0.5).expect("the right dock's button");
    click_at(&mut h, collapse.center());
    assert_eq!(dock::collapsed(h.state()), [Zone::Right]);
    assert_eq!(dock::saved(h.state())["collapsed"], json!(["Right"]));
    assert!(!shown(&h, "Background"), "the dock is a rail");
    // The canvas takes the dock's room, beside a rail of icons.
    let rail_icon = h.query_all_by_label("Layers").map(|n| n.rect()).find(|r| r.left() > SIZE.x - rail::THICKNESS - 1.0).expect("the rail's Layers icon");
    assert!(rail_icon.width() < rail::THICKNESS);

    click_at(&mut h, rail_icon.center());
    assert_eq!(rail::overlay(&h.ctx), Some(PanelId::Layer));
    assert!(shown(&h, "Background"), "the palette opens beside the rail");
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert_eq!(rail::overlay(&h.ctx), None);
    assert!(!shown(&h, "Background"));

    // It stays collapsed after a restart, until the expand button.
    drop(h);
    let mut h = harness(&setup);
    h.run_steps(4);
    assert_eq!(dock::collapsed(h.state()), [Zone::Right]);
    h.get_by_label("Expand Area").click();
    h.run_steps(4);
    assert!(dock::collapsed(h.state()).is_empty());
    assert!(shown(&h, "Navigator"));
}

/// D9: Window › Workspace › New Workspace… saves the layout to `workspaces/<name>.json`, its
/// menu item loads it back (also after a restart), Delete Workspace… removes it.
#[test]
fn workspaces_save_load_and_delete_from_the_window_menu() {
    let dir = tempfile::tempdir().unwrap();
    let setup = Setup { workspaces: Some(dir.path().join("workspaces")), ..Default::default() };
    let mut h = harness(&setup);
    h.run_steps(4);
    let ctx = h.ctx.clone();
    assert_eq!(menus::invoke(h.state_mut(), &ctx, "clippy.palette.navigator", json!({})).unwrap()["shown"], false);
    h.run_steps(3);
    let workspace = |h: &Harness| menus::menu_items(h.state()).into_iter().filter(|i| i.path == ["Window", "Workspace"]).collect::<Vec<_>>();
    assert_eq!(
        workspace(&h).iter().map(|i| i.label.as_str()).collect::<Vec<_>>(),
        ["New Workspace…", "Delete Workspace…", "---", "Keyboard Shortcuts…", "Lock Workspace"]
    );

    assert_eq!(menus::invoke(h.state_mut(), &ctx, "clippy.workspace.save", json!({})).unwrap(), json!({"dialog": "newWorkspace"}));
    h.run_steps(3);
    let title = top(&h, "New Workspace");
    let field =
        h.query_all_by_role(egui::accesskit::Role::TextInput).find(|n| n.rect().top() > title.top() && (n.rect().center().x - title.center().x).abs() < 220.0);
    field.expect("the name field").type_text("Comic");
    h.run_steps(2);
    h.get_by_label("OK").click();
    h.run_steps(3);
    let file = dir.path().join("workspaces").join("Comic.json");
    assert!(!std::fs::read_to_string(&file).unwrap().contains("navigator"), "saved without the Navigator");
    assert!(h.query_by_label("New Workspace").is_none(), "the dialog closed");

    // Another layout, then the workspace again from its menu item.
    menus::invoke(h.state_mut(), &ctx, "clippy.layout.reset", json!({})).unwrap();
    h.run_steps(3);
    assert!(dock::current(h.state()).contains(&PanelId::Navigator));
    let load = workspace(&h).into_iter().find(|i| i.label == "Comic").expect("the workspace's item");
    menus::invoke(h.state_mut(), &ctx, &load.id, json!({})).unwrap();
    h.run_steps(3);
    assert!(!dock::current(h.state()).contains(&PanelId::Navigator));
    assert!(!shown(&h, "Navigator"));

    // The loaded layout is the one a restart shows.
    drop(h);
    let mut h = harness(&setup);
    h.run_steps(4);
    assert!(!dock::current(h.state()).contains(&PanelId::Navigator));

    let ctx = h.ctx.clone();
    menus::invoke(h.state_mut(), &ctx, "clippy.workspace.delete", json!({})).unwrap();
    h.run_steps(3);
    assert!(h.query_by_label("Comic").is_some(), "the dialog lists it");
    let delete = entry(&h, "Delete", top(&h, "Delete Workspace"));
    click_at(&mut h, delete);
    h.run_steps(3);
    assert!(!file.exists());
    assert!(!workspace(&h).iter().any(|i| i.label == "Comic"));
}

/// Window lists every palette in Clippy mode, checked while shown; choosing one closes or shows
/// it. Photoshop mode keeps PhotoCraft's Window menu.
#[test]
fn the_window_menu_lists_every_palette_and_toggles_it() {
    let mut h = harness(&Setup::default());
    h.run_steps(4);
    let items = menus::menu_items(h.state());
    let palettes: Vec<_> = items.iter().filter(|i| i.id.starts_with(dock::PALETTE_PREFIX)).collect();
    assert_eq!(palettes.len(), 25, "every palette but the canvas");
    assert!(palettes.iter().all(|i| i.path == ["Window"] && i.enabled));
    assert!(!items.iter().any(|i| i.id == "window.panel.layers"), "Clippy's Layers palette replaces PhotoCraft's panel");
    let checked = |h: &Harness, id: &str| menus::menu_items(h.state()).into_iter().find(|i| i.id == id).and_then(|i| i.checked);
    assert_eq!(checked(&h, "clippy.palette.layer"), Some(true));
    assert_eq!(checked(&h, "clippy.palette.character"), Some(false));

    let ctx = h.ctx.clone();
    menus::invoke(h.state_mut(), &ctx, "clippy.palette.layer", json!({})).unwrap();
    h.run_steps(3);
    assert_eq!(checked(&h, "clippy.palette.layer"), Some(false));
    assert!(!shown(&h, "Layers"));
    menus::invoke(h.state_mut(), &ctx, "clippy.palette.layer", json!({})).unwrap();
    h.run_steps(3);
    assert_eq!(checked(&h, "clippy.palette.layer"), Some(true));
    assert_eq!(layout::group_of(&dock::current(h.state()), PanelId::Layer).unwrap(), [PanelId::Layer, PanelId::History, PanelId::AutoAction]);
    // A palette the default layout doesn't show floats.
    menus::invoke(h.state_mut(), &ctx, "clippy.palette.character", json!({})).unwrap();
    h.run_steps(3);
    assert_eq!(dock::current(h.state()).floating.first().map(|f| f.panels.clone()), Some(vec![PanelId::Character]));
    assert!(shown(&h, "Character"));
    assert!(menus::invoke(h.state_mut(), &ctx, "clippy.palette.canvas", json!({})).is_err());
    assert!(menus::invoke(h.state_mut(), &ctx, "clippy.palette.brushes", json!({})).is_err());

    // Photoshop mode: PhotoCraft's own items, nothing of Clippy's but the interface switch.
    h.state_mut().run("clippy.mode.set", json!({"mode": "photoshop"})).unwrap();
    let items = menus::menu_items(h.state());
    assert!(items.iter().any(|i| i.id == "window.panel.layers"));
    assert!(items.iter().any(|i| i.id == "window.workspace.essentials"));
    assert!(!items.iter().any(|i| i.id.starts_with(dock::PALETTE_PREFIX) || i.id.starts_with("clippy.workspace")));
    assert!(menus::invoke(h.state_mut(), &ctx, "clippy.palette.layer", json!({})).is_err(), "not a command outside Clippy mode");
}

/// G10: dragging a tab beside a group of a side dock shows no drop preview (D14 refuses it),
/// above or below the group does; a release where it is refused changes nothing.
#[test]
fn the_drop_preview_never_offers_a_refused_drop() {
    let mut h = harness(&Setup::default());
    h.run_steps(4);
    let before = dock::current(h.state());
    let info = top(&h, "Info").center();
    let layers = top(&h, "Layers");
    let properties = top(&h, "Properties");
    let beside = egui::pos2(layers.left() + 4.0, layers.bottom() + 60.0);
    let below = egui::pos2(layers.left() + 120.0, properties.top() - 10.0);

    press(&mut h, info);
    move_to(&mut h, info, beside);
    assert!(dock::drop_refused(&h.ctx), "beside Layers");
    move_to(&mut h, beside, below);
    assert!(!dock::drop_refused(&h.ctx), "below Layers");
    move_to(&mut h, below, beside);
    assert!(dock::drop_refused(&h.ctx));
    release(&mut h, beside);
    assert_eq!(dock::current(h.state()), before);
    assert!(!dock::drop_refused(&h.ctx));
}

/// G10: Esc cancels a tab drag; the release then drops nothing.
#[test]
fn esc_cancels_a_drag() {
    let mut h = harness(&Setup::default());
    h.run_steps(4);
    let before = dock::current(h.state());
    let info = top(&h, "Info").center();
    let bottom = egui::pos2(640.0, SIZE.y - 50.0);
    press(&mut h, info);
    move_to(&mut h, info, bottom);
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    move_to(&mut h, bottom, bottom + egui::vec2(4.0, 0.0));
    release(&mut h, bottom + egui::vec2(4.0, 0.0));
    assert_eq!(dock::current(h.state()), before);
    assert_eq!(dock::saved(h.state()), Value::Null);
}

/// G10: a palette is never narrower than 160 pt, however far its dock's edge is dragged.
#[test]
fn a_dock_is_never_narrower_than_160() {
    let mut h = harness(&Setup::default());
    h.run_steps(4);
    let y = SIZE.y * 0.5;
    let edge = egui::pos2(top(&h, "Sub Tool").left() + 282.0, y);
    press(&mut h, edge);
    move_to(&mut h, edge, egui::pos2(10.0, y));
    release(&mut h, egui::pos2(10.0, y));
    let Some(Node::Split { size: SplitSize::FixedFirst(width), .. }) = dock::current(h.state()).root else { panic!("the left dock's split") };
    assert!(width < 280.0, "the edge moved: {width}");
    assert!(width >= 160.0 - 0.01, "{width}");
    assert!(top(&h, "Sub Tool").width() <= width, "the header fits");
}

/// G10: a double click on a tab strip (its free part or a tab) collapses the area.
#[test]
fn a_double_click_on_a_tab_strip_collapses_the_area() {
    let mut h = harness(&Setup { step_dt: Some(1.0 / 60.0), ..Default::default() });
    h.run_steps(4);
    let sub_tool = top(&h, "Sub Tool");
    double_click_at(&mut h, egui::pos2(sub_tool.right() + 60.0, sub_tool.center().y));
    assert_eq!(dock::collapsed(h.state()), [Zone::Left]);
    let layers = top(&h, "Layers").center();
    double_click_at(&mut h, layers);
    assert_eq!(dock::collapsed(h.state()), [Zone::Left, Zone::Right]);
    // A single click only selects.
    let expand = h.query_all_by_label("Expand Area").map(|n| n.rect()).min_by(|a, b| a.left().total_cmp(&b.left())).unwrap();
    click_at(&mut h, expand.center());
    h.run_steps(4);
    let sub_tool = top(&h, "Sub Tool").center();
    click_at(&mut h, sub_tool);
    assert_eq!(dock::collapsed(h.state()), [Zone::Right]);
}

#[derive(Default)]
struct Setup {
    prefs: Arc<Mutex<Option<String>>>,
    workspaces: Option<std::path::PathBuf>,
    /// Seconds per frame; kittest's default quarter second makes two clicks too slow for a
    /// double click.
    step_dt: Option<f32>,
}

/// Clippy in English whose preferences live in `setup.prefs`.
fn harness(setup: &Setup) -> Harness {
    let store = setup.prefs.clone();
    let workspaces = setup.workspaces.clone();
    let mut builder = egui_kittest::Harness::builder().with_size(SIZE).with_max_steps(64);
    if let Some(dt) = setup.step_dt {
        builder = builder.with_step_dt(dt);
    }
    builder.build_eframe(move |cc| {
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
        app.session.clippy.workspace_dir = workspaces.clone();
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

/// The ☰ button of the group whose tab is `tab`.
fn palette_menu(h: &Harness, tab: &str) -> egui::Rect {
    let tab = top(h, tab);
    let strip = |r: &egui::Rect| (r.center().y - tab.center().y).abs() < 2.0 && r.center().x <= tab.right() + 1.0;
    h.query_all_by_label("Palette Menu").map(|n| n.rect()).filter(strip).max_by(|a, b| a.left().total_cmp(&b.left())).expect("☰")
}

/// The centre of the node labelled `label` below `anchor` and near it (a menu entry under its
/// button, a dialog's button under its title).
fn entry(h: &Harness, label: &str, anchor: egui::Rect) -> egui::Pos2 {
    h.query_all_by_label(label)
        .map(|n| n.rect())
        .filter(|r| r.top() >= anchor.top() && (r.center().x - anchor.center().x).abs() < 220.0)
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .expect(label)
        .center()
}

fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE }
}

fn click_at(h: &mut Harness, pos: egui::Pos2) {
    h.event(egui::Event::PointerMoved(pos));
    h.run_steps(1);
    h.event(button(pos, true));
    h.event(button(pos, false));
    h.run_steps(4);
}

fn double_click_at(h: &mut Harness, pos: egui::Pos2) {
    // Long enough after any earlier click that egui doesn't count a triple click.
    h.event(egui::Event::PointerMoved(pos));
    h.run_steps(60);
    // Each event is a step of its own (see `Setup::step_dt`).
    for _ in 0..2 {
        h.event(button(pos, true));
        h.event(button(pos, false));
    }
    h.run_steps(4);
}

fn press(h: &mut Harness, pos: egui::Pos2) {
    h.event(egui::Event::PointerMoved(pos));
    h.run_steps(1);
    h.event(button(pos, true));
    h.run_steps(1);
}

/// Move a held pointer in steps, as a real drag does.
fn move_to(h: &mut Harness, from: egui::Pos2, to: egui::Pos2) {
    for i in 1..=10 {
        h.event(egui::Event::PointerMoved(from + (to - from) * (i as f32 / 10.0)));
        h.run_steps(1);
    }
}

fn release(h: &mut Harness, pos: egui::Pos2) {
    h.event(button(pos, false));
    h.run_steps(4);
}
