//! Clippy's dock layout: the default one, reading a saved one, and the rules a change must keep.
//!
//! The window under the menu is one `craft_ui::docking::Layout`: the canvas, protected, in the
//! middle, and a dock on any side of it (features D1, D2). A dock is a column (left, right) or a
//! row (top, bottom) of tab groups; inside it groups only stack along it, never side by side
//! (D14), and each side of the canvas has at most one dock.

use craft_ui::docking::{Action, DockError, Layout, Node, Placement, Zone};
use craft_ui::layout::SplitAxis;
use serde_json::Value;

use super::panels::PanelId;

pub type DockLayout = Layout<PanelId>;

/// The layout of a first start (`docs/paletten.md`, D10).
const DEFAULT_JSON: &str = include_str!("../../assets/default-layout.json");

/// Why a saved layout can't be shown.
#[derive(Clone, Debug, PartialEq)]
pub enum Invalid {
    /// Not a layout of known palettes.
    Unreadable(String),
    Dock(DockError),
    /// The canvas is missing, floating or shares its tab group.
    Canvas,
}

/// The default layout. Should the bundled file ever be broken (a test guards it), the canvas
/// alone, so the window still works.
pub fn default_layout() -> DockLayout {
    serde_json::from_str::<DockLayout>(DEFAULT_JSON).ok().filter(|l| check(l).is_ok()).unwrap_or_else(canvas_only)
}

/// A saved layout (`clippy.layouts.clippy`); null or missing is the default.
pub fn read(saved: &Value) -> Result<DockLayout, Invalid> {
    if saved.is_null() {
        return Ok(default_layout());
    }
    let layout = serde_json::from_value::<DockLayout>(saved.clone()).map_err(|e| Invalid::Unreadable(e.to_string()))?;
    check(&layout)?;
    Ok(layout)
}

/// What the dock can show: a valid tree with the canvas alone in its group, docked.
pub fn check(layout: &DockLayout) -> Result<(), Invalid> {
    layout.validate().map_err(Invalid::Dock)?;
    if layout.floating.iter().any(|f| f.panels.contains(&PanelId::Canvas)) {
        return Err(Invalid::Canvas);
    }
    let mut pending: Vec<&Node<PanelId>> = layout.root.iter().collect();
    while let Some(node) = pending.pop() {
        match node {
            Node::Split { first, second, .. } => pending.extend([first.as_ref(), second.as_ref()]),
            Node::Tabs { panels, .. } if panels.contains(&PanelId::Canvas) => return if panels.len() == 1 { Ok(()) } else { Err(Invalid::Canvas) },
            Node::Stack { entries } if entries.iter().any(|e| e.panel == PanelId::Canvas) => return Err(Invalid::Canvas),
            _ => {}
        }
    }
    Err(Invalid::Canvas)
}

/// Whether a dock change keeps the rules above. Moves elsewhere (tabs, floating) always do.
pub fn allowed(layout: &DockLayout, action: &Action<PanelId>) -> bool {
    let (anchor, zone) = match action {
        Action::Move { anchor, placement: Placement::Split(zone), .. } | Action::OpenAt { anchor, placement: Placement::Split(zone), .. } => (*anchor, *zone),
        _ => return true,
    };
    if anchor == PanelId::Canvas {
        // Beside the canvas: a new dock, only where that side has none yet.
        return !sides(layout).contains(&zone);
    }
    match dock_axis(layout, anchor) {
        // A column: groups stack top to bottom.
        Some(SplitAxis::Horizontal) => !matches!(zone, Zone::Left | Zone::Right),
        // A row: groups line up left to right.
        Some(SplitAxis::Vertical) => !matches!(zone, Zone::Top | Zone::Bottom),
        None => true,
    }
}

/// The sides of the canvas that have a dock.
pub fn sides(layout: &DockLayout) -> Vec<Zone> {
    let mut found = Vec::new();
    let mut node = layout.root.as_ref();
    while let Some(Node::Split { axis, first, second, .. }) = node {
        let (in_first, in_second) = (contains(first, PanelId::Canvas), contains(second, PanelId::Canvas));
        let side = match (axis, in_first) {
            (SplitAxis::Horizontal, true) => Zone::Right,
            (SplitAxis::Horizontal, false) => Zone::Left,
            (SplitAxis::Vertical, true) => Zone::Bottom,
            (SplitAxis::Vertical, false) => Zone::Top,
        };
        if !found.contains(&side) {
            found.push(side);
        }
        node = if in_first {
            Some(first.as_ref())
        } else if in_second {
            Some(second.as_ref())
        } else {
            None
        };
    }
    found
}

/// The axis of the split that puts `panel`'s dock beside the canvas: Horizontal for a side dock
/// (column), Vertical for a top or bottom dock (row). None for the canvas or a floating panel.
fn dock_axis(layout: &DockLayout, panel: PanelId) -> Option<SplitAxis> {
    if panel == PanelId::Canvas {
        return None;
    }
    let mut node = layout.root.as_ref()?;
    // `check` bounds the depth (craft_ui's MAX_DEPTH), so this loop ends.
    loop {
        let Node::Split { axis, first, second, .. } = node else { return None };
        let (panel_first, canvas_first) = (contains(first, panel), contains(first, PanelId::Canvas));
        let (panel_second, canvas_second) = (contains(second, panel), contains(second, PanelId::Canvas));
        if (panel_first && canvas_second) || (panel_second && canvas_first) {
            return Some(*axis);
        }
        node = if panel_first && canvas_first {
            first
        } else if panel_second && canvas_second {
            second
        } else {
            return None;
        };
    }
}

fn contains(node: &Node<PanelId>, panel: PanelId) -> bool {
    match node {
        Node::Split { first, second, .. } => contains(first, panel) || contains(second, panel),
        Node::Tabs { panels, .. } => panels.contains(&panel),
        Node::Stack { entries } => entries.iter().any(|e| e.panel == panel),
    }
}

fn canvas_only() -> DockLayout {
    Layout { root: Some(Node::Tabs { panels: vec![PanelId::Canvas], active: 0 }), floating: Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dock::panels::PANELS;

    #[test]
    fn the_bundled_default_is_valid_and_matches_the_palette_plan() {
        let layout = serde_json::from_str::<DockLayout>(DEFAULT_JSON).unwrap();
        check(&layout).unwrap();
        assert_eq!(sides(&layout), [Zone::Left, Zone::Right]);
        let shown = layout.panels();
        for id in [
            PanelId::SubTool,
            PanelId::ToolProperty,
            PanelId::ColorSet,
            PanelId::ColorSlider,
            PanelId::Navigator,
            PanelId::Layer,
            PanelId::LayerProperty,
            PanelId::Tool,
        ] {
            assert!(shown.contains(&&id), "{id:?}");
        }
        assert!(layout.floating.is_empty());
    }

    #[test]
    fn the_registry_lists_every_palette_once_in_order() {
        for (i, p) in PANELS.iter().enumerate() {
            assert_eq!(p.id as usize, i, "{:?}", p.id);
            assert!(photocraft_ui_egui::icons::exists(p.icon), "{:?}: icon {}", p.id, p.icon);
        }
    }

    #[test]
    fn side_docks_stack_and_rows_line_up() {
        let l = default_layout();
        let mv = |panel, anchor, zone| Action::Move { panel, anchor, placement: Placement::Split(zone) };
        // Layers is in the right column: above or below, never beside.
        assert!(allowed(&l, &mv(PanelId::Info, PanelId::Layer, Zone::Top)));
        assert!(allowed(&l, &mv(PanelId::Info, PanelId::Layer, Zone::Bottom)));
        assert!(!allowed(&l, &mv(PanelId::Info, PanelId::Layer, Zone::Left)));
        assert!(!allowed(&l, &mv(PanelId::Info, PanelId::Layer, Zone::Right)));
        // Tabs are always fine.
        assert!(allowed(&l, &Action::Move { panel: PanelId::Info, anchor: PanelId::Layer, placement: Placement::Tab { before: None } }));
        // Beside the canvas: a new top or bottom dock, but not a second left or right one.
        assert!(allowed(&l, &mv(PanelId::Info, PanelId::Canvas, Zone::Top)));
        assert!(allowed(&l, &mv(PanelId::Info, PanelId::Canvas, Zone::Bottom)));
        assert!(!allowed(&l, &mv(PanelId::Info, PanelId::Canvas, Zone::Left)));
        assert!(!allowed(&l, &mv(PanelId::Info, PanelId::Canvas, Zone::Right)));

        // A top dock is a row.
        let mut top = l.clone();
        top.apply(mv(PanelId::Info, PanelId::Canvas, Zone::Top)).unwrap();
        assert!(sides(&top).contains(&Zone::Top));
        assert!(allowed(&top, &mv(PanelId::Histogram, PanelId::Info, Zone::Right)));
        assert!(!allowed(&top, &mv(PanelId::Histogram, PanelId::Info, Zone::Bottom)));
        assert!(!allowed(&top, &mv(PanelId::Histogram, PanelId::Canvas, Zone::Top)));
    }

    #[test]
    fn saved_layouts_are_read_or_rejected_whole() {
        assert_eq!(read(&Value::Null).unwrap(), default_layout());
        let saved = serde_json::to_value(default_layout()).unwrap();
        assert_eq!(read(&saved).unwrap(), default_layout());
        for bad in [
            serde_json::json!({}),
            serde_json::json!([]),
            serde_json::json!("layout"),
            serde_json::json!({"root": {"Tabs": {"panels": ["brushes"], "active": 0}}, "floating": []}),
            serde_json::json!({"root": {"Tabs": {"panels": ["layer"], "active": 0}}, "floating": []}),
            serde_json::json!({"root": {"Tabs": {"panels": ["canvas", "layer"], "active": 0}}, "floating": []}),
            serde_json::json!({"root": {"Tabs": {"panels": ["canvas"], "active": 3}}, "floating": []}),
            serde_json::json!({"root": {"Tabs": {"panels": ["canvas", "canvas"], "active": 0}}, "floating": []}),
            serde_json::json!({"root": null, "floating": [{"panels": ["canvas"], "active": 0, "rect": [0.0, 0.0, 10.0, 10.0]}]}),
        ] {
            assert!(read(&bad).is_err(), "{bad}");
        }
    }
}
