//! Clippy's dock layout: the default one, reading a saved one, and the rules a change must keep.
//!
//! The window under the menu is one `craft_ui::docking::Layout`: the canvas, protected, in the
//! middle, and a dock on any side of it (features D1, D2). A dock is a column (left, right) or a
//! row (top, bottom) of tab groups; inside it groups only stack along it, never side by side
//! (D14), and each side of the canvas has at most one dock.
//!
//! A dock can be collapsed to an icon rail (D8): the saved value then also lists its side under
//! `collapsed` (`["Left"]`), beside craft_ui's own `root` and `floating`, which older builds read
//! past. The frame shows a [`View`]: the layout without the collapsed docks.

use craft_ui::docking::{Action, DockError, Floating, Layout, Node, Placement, Zone};
use craft_ui::layout::{SplitAxis, SplitSize};
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

/// The key of the collapsed sides in a saved layout.
pub const COLLAPSED_KEY: &str = "collapsed";

/// One dock: a split on the way from the root to the canvas, and the side of the canvas it holds.
#[derive(Clone, Debug, PartialEq)]
pub struct Dock {
    pub side: Zone,
    /// The split's path from the root (false: first child), as `Action::ResizeSplit` names it.
    pub path: Vec<bool>,
    /// Which child of the split holds the canvas (true: the second).
    pub canvas_second: bool,
    /// Its palettes, group by group: top to bottom in a column, left to right in a row.
    pub groups: Vec<Vec<PanelId>>,
    /// Its width (left, right) or height (top, bottom) when the split fixes it.
    pub extent: Option<f32>,
}

/// Every dock, outermost first.
pub fn docks(layout: &DockLayout) -> Vec<Dock> {
    let mut found = Vec::new();
    let mut node = layout.root.as_ref();
    let mut path = Vec::new();
    while let Some(Node::Split { axis, size, first, second }) = node {
        let canvas_second = contains(second, PanelId::Canvas);
        if !canvas_second && !contains(first, PanelId::Canvas) {
            break;
        }
        let side = match (axis, canvas_second) {
            (SplitAxis::Horizontal, true) => Zone::Left,
            (SplitAxis::Horizontal, false) => Zone::Right,
            (SplitAxis::Vertical, true) => Zone::Top,
            (SplitAxis::Vertical, false) => Zone::Bottom,
        };
        let extent = match (size, canvas_second) {
            (SplitSize::FixedFirst(v), true) | (SplitSize::FixedSecond(v), false) => Some(*v),
            _ => None,
        };
        let (dock, canvas) = if canvas_second { (first, second) } else { (second, first) };
        found.push(Dock { side, path: path.clone(), canvas_second, groups: groups(dock), extent });
        path.push(canvas_second);
        node = Some(canvas.as_ref());
    }
    found
}

/// The side of the canvas whose dock holds `panel`; None for the canvas and floating palettes.
pub fn dock_side(layout: &DockLayout, panel: PanelId) -> Option<Zone> {
    docks(layout).into_iter().find(|d| d.groups.iter().any(|g| g.contains(&panel))).map(|d| d.side)
}

/// Every tab group in `node`, in order; a stack counts as one group.
pub fn groups(node: &Node<PanelId>) -> Vec<Vec<PanelId>> {
    let mut found = Vec::new();
    let mut pending = vec![node];
    while let Some(node) = pending.pop() {
        match node {
            Node::Split { first, second, .. } => pending.extend([second.as_ref(), first.as_ref()]),
            Node::Tabs { panels, .. } => found.push(panels.clone()),
            Node::Stack { entries } => found.push(entries.iter().map(|e| e.panel).collect()),
        }
    }
    found
}

/// The group holding `panel`, docked or floating.
pub fn group_of(layout: &DockLayout, panel: PanelId) -> Option<Vec<PanelId>> {
    let docked = layout.root.as_ref().map(groups).unwrap_or_default();
    docked.into_iter().chain(layout.floating.iter().map(|f| f.panels.clone())).find(|g| g.contains(&panel))
}

/// The collapsed sides of a saved layout: sides `layout` has a dock on, each once. Anything else
/// under the key is ignored.
pub fn read_collapsed(saved: &Value, layout: &DockLayout) -> Vec<Zone> {
    let sides = sides(layout);
    let mut found = Vec::new();
    for value in saved.get(COLLAPSED_KEY).and_then(Value::as_array).into_iter().flatten().take(8) {
        if let Ok(zone) = serde_json::from_value::<Zone>(value.clone())
            && zone != Zone::Center
            && sides.contains(&zone)
            && !found.contains(&zone)
        {
            found.push(zone);
        }
    }
    found
}

/// The value saved for `layout` with `collapsed` docks.
pub fn to_value(layout: &DockLayout, collapsed: &[Zone]) -> Value {
    let mut value = serde_json::to_value(layout).unwrap_or(Value::Null);
    if !collapsed.is_empty()
        && let Some(map) = value.as_object_mut()
    {
        map.insert(COLLAPSED_KEY.into(), serde_json::to_value(collapsed).unwrap_or(Value::Null));
    }
    value
}

/// What the dock area draws: the layout without its collapsed docks, which show as rails.
#[derive(Clone, Debug)]
pub struct View {
    pub layout: DockLayout,
    /// The collapsed docks, outermost first.
    pub rails: Vec<Dock>,
}

/// The view of `layout` with the docks on `collapsed` sides left out.
pub fn view(layout: &DockLayout, collapsed: &[Zone]) -> View {
    let rails: Vec<Dock> = docks(layout).into_iter().filter(|d| collapsed.contains(&d.side)).collect();
    let root = layout.root.as_ref().map(|root| without(root, &mut Vec::new(), &rails));
    View { layout: Layout { root, floating: layout.floating.clone() }, rails }
}

/// `node` at `path` with the splits of `rails` replaced by their canvas side. `check` bounds the
/// depth (craft_ui's MAX_DEPTH), so the recursion is shallow.
fn without(node: &Node<PanelId>, path: &mut Vec<bool>, rails: &[Dock]) -> Node<PanelId> {
    let Node::Split { axis, size, first, second } = node else { return node.clone() };
    if let Some(rail) = rails.iter().find(|r| r.path == *path) {
        path.push(rail.canvas_second);
        let kept = without(if rail.canvas_second { second } else { first }, path, rails);
        path.pop();
        return kept;
    }
    path.push(false);
    let first = without(first, path, rails);
    path.pop();
    path.push(true);
    let second = without(second, path, rails);
    path.pop();
    Node::Split { axis: *axis, size: *size, first: Box::new(first), second: Box::new(second) }
}

impl View {
    /// The path in the full layout of the split at `path` in the view.
    pub fn real_path(&self, path: &[bool]) -> Vec<bool> {
        let mut real = Vec::new();
        let mut bits = path.iter();
        // Each pass adds one bit; the left-out splits are finitely many and only get longer.
        loop {
            if let Some(rail) = self.rails.iter().find(|r| r.path == real) {
                real.push(rail.canvas_second);
                continue;
            }
            match bits.next() {
                Some(bit) => real.push(*bit),
                None => return real,
            }
        }
    }

    /// An action on the view as one on the full layout.
    pub fn real_action(&self, action: Action<PanelId>) -> Action<PanelId> {
        match action {
            Action::ResizeSplit { path, size } => Action::ResizeSplit { path: self.real_path(&path), size },
            other => other,
        }
    }
}

/// `layout` with the absent `panel` shown again: as a tab beside a palette it shares a group with
/// in the default layout, else floating at `rect` (`[x, y, width, height]`). None when the panel
/// is already there or it can't be placed.
pub fn reopened(layout: &DockLayout, panel: PanelId, rect: [f32; 4]) -> Option<DockLayout> {
    if panel == PanelId::Canvas || layout.contains(&panel) {
        return None;
    }
    let mates = group_of(&default_layout(), panel).unwrap_or_default();
    let at = mates.iter().position(|p| *p == panel).unwrap_or(0);
    for anchor in mates.iter().filter(|p| **p != panel && layout.contains(p)) {
        let group = group_of(layout, *anchor).unwrap_or_default();
        // Before the next palette of the default order that is in that group.
        let before = mates.iter().skip(at + 1).find(|p| group.contains(p)).copied();
        let mut next = layout.clone();
        if next.apply(Action::OpenAt { panel, anchor: *anchor, placement: Placement::Tab { before } }).is_ok()
            && next.apply(Action::Activate { panel }).is_ok()
            && check(&next).is_ok()
        {
            return Some(next);
        }
    }
    let mut next = layout.clone();
    next.floating.push(Floating { panels: vec![panel], active: 0, rect });
    check(&next).is_ok().then_some(next)
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
    fn collapsed_docks_leave_the_view_and_paths_map_back() {
        let l = default_layout();
        let docks = docks(&l);
        assert_eq!(docks.iter().map(|d| d.side).collect::<Vec<_>>(), [Zone::Left, Zone::Right]);
        assert_eq!(docks[0].extent, Some(280.0));
        assert_eq!(docks[1].extent, Some(260.0));
        assert_eq!(docks[1].path, [true]);
        assert_eq!(docks[0].groups[0], [PanelId::SubTool]);
        assert_eq!(dock_side(&l, PanelId::Layer), Some(Zone::Right));
        assert_eq!(dock_side(&l, PanelId::Canvas), None);

        let v = view(&l, &[Zone::Left]);
        assert!(!v.layout.contains(&PanelId::SubTool) && v.layout.contains(&PanelId::Layer));
        assert_eq!(v.rails.len(), 1);
        // The view's root is the full layout's second child; the right dock's split follows.
        assert_eq!(v.real_path(&[]), [true]);
        assert_eq!(v.real_path(&[true, false]), [true, true, false]);
        let both = view(&l, &[Zone::Right, Zone::Left]);
        assert_eq!(both.layout.root, Some(Node::Tabs { panels: vec![PanelId::Canvas], active: 0 }));
        assert_eq!(both.real_path(&[]), [true, false]);
        assert_eq!(view(&l, &[]).real_path(&[true]), [true]);
    }

    #[test]
    fn collapsed_sides_round_trip_and_unknown_ones_are_dropped() {
        let l = default_layout();
        let saved = to_value(&l, &[Zone::Right]);
        assert_eq!(saved[COLLAPSED_KEY], serde_json::json!(["Right"]));
        assert_eq!(read(&saved).unwrap(), l);
        assert_eq!(read_collapsed(&saved, &l), [Zone::Right]);
        assert_eq!(to_value(&l, &[]).get(COLLAPSED_KEY), None);
        let odd = serde_json::json!({"collapsed": ["Top", "Right", "Right", "Center", 3, "left"]});
        assert_eq!(read_collapsed(&odd, &l), [Zone::Right]);
        assert!(read_collapsed(&serde_json::json!({"collapsed": "Left"}), &l).is_empty());
    }

    #[test]
    fn a_closed_palette_reopens_beside_its_default_group() {
        let mut l = default_layout();
        l.apply(Action::Close { panel: PanelId::Info }).unwrap();
        let back = reopened(&l, PanelId::Info, [0.0, 0.0, 200.0, 200.0]).unwrap();
        assert_eq!(group_of(&back, PanelId::Info).unwrap(), [PanelId::Navigator, PanelId::Info, PanelId::Histogram]);
        assert!(reopened(&back, PanelId::Info, [0.0, 0.0, 200.0, 200.0]).is_none(), "already shown");
        // Without any of its group, it floats.
        for p in [PanelId::Info, PanelId::Navigator, PanelId::Histogram] {
            l.apply(Action::Close { panel: p }).ok();
        }
        let floating = reopened(&l, PanelId::Histogram, [10.0, 20.0, 200.0, 220.0]).unwrap();
        assert_eq!(floating.floating[0].panels, [PanelId::Histogram]);
        assert!(reopened(&l, PanelId::Canvas, [0.0, 0.0, 1.0, 1.0]).is_none());
        // A palette the default layout doesn't show floats too.
        assert!(reopened(&default_layout(), PanelId::Character, [0.0, 0.0, 200.0, 200.0]).unwrap().floating.len() == 1);
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
