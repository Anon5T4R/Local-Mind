//! The map canvas: where every card is drawn, and how the view moves over it.
//!
//! Geometry comes from `localmind-layout` (the port of the original app's balanced four-way
//! layout); this module only maps it to the screen, draws it, and turns a click into a node id.
//! Pan, zoom and the selected card live in [`CanvasView`], which the app owns across frames.

use egui::{Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};
use localmind_layout::{Layout, ResolvedSide, font_of, layout_tree, node_size};
use localmind_model::{MindMap, MindNode};

/// Zoom limits, as the original app's canvas had them.
pub const MIN_ZOOM: f32 = 0.2;
/// Zoom limits, as the original app's canvas had them.
pub const MAX_ZOOM: f32 = 2.0;
/// Below this zoom the card text is not drawn at all: at a few pixels it is a smear, and laying
/// out thousands of galleys per frame is what would make the canvas feel slow.
const TEXT_MIN_ZOOM: f32 = 0.35;
/// Spacing of the background dots, in layout units.
const DOT_STEP: f32 = 24.0;
/// The most dots the background draws, so a zoomed-out map does not paint a million of them.
const DOT_LIMIT: usize = 6_000;

/// The view over the map: where the canvas is panned to and how much it is zoomed.
#[derive(Clone, Debug)]
pub struct CanvasView {
    /// Offset of the layout origin from the centre of the canvas, in screen pixels.
    pub pan: Vec2,
    pub zoom: f32,
    /// Set when the map changed enough that the view should be refitted (opened, new, generated).
    pub needs_fit: bool,
}

impl Default for CanvasView {
    fn default() -> Self {
        Self { pan: Vec2::ZERO, zoom: 1.0, needs_fit: true }
    }
}

impl CanvasView {
    /// Frames the whole map in `rect` with some margin.
    pub fn fit(&mut self, map: &MindMap, rect: Rect) {
        let layout = layout_tree(&map.nodes);
        let Some((origin, size)) = layout.bounds(&map.nodes) else {
            self.pan = Vec2::ZERO;
            self.zoom = 1.0;
            return;
        };
        let pad = 40.0;
        let zoom_x = (rect.width() - 2.0 * pad) / size.w.max(1.0);
        let zoom_y = (rect.height() - 2.0 * pad) / size.h.max(1.0);
        self.zoom = zoom_x.min(zoom_y).clamp(MIN_ZOOM, MAX_ZOOM);
        let center = vec2(origin.x + size.w / 2.0, origin.y + size.h / 2.0);
        self.pan = -center * self.zoom;
    }

    /// Zooms by `factor` keeping the layout point under `pointer` where it is on screen.
    pub fn zoom_around(&mut self, factor: f32, pointer: Pos2, rect: Rect) {
        let before = self.zoom;
        self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        if (self.zoom - before).abs() < f32::EPSILON {
            return;
        }
        // The layout point under the pointer has to stay under the pointer: solve for the pan.
        let origin = rect.center() + self.pan;
        let world = (pointer - origin) / before;
        let new_origin = pointer - world * self.zoom;
        self.pan = new_origin - rect.center();
    }

    fn origin(&self, rect: Rect) -> Pos2 {
        rect.center() + self.pan
    }

    fn to_screen(&self, origin: Pos2, x: f32, y: f32) -> Pos2 {
        origin + vec2(x, y) * self.zoom
    }
}

/// The screen rectangle a card occupies, given the layout and the current view.
pub fn card_rect(view: &CanvasView, rect: Rect, pos: localmind_layout::Pos, size: (f32, f32)) -> Rect {
    let min = view.to_screen(view.origin(rect), pos.x, pos.y);
    Rect::from_min_size(min, vec2(size.0, size.1) * view.zoom)
}

/// The card under `pointer`, if any. Cards are painted shapes rather than widgets, so the hit test
/// lives here (and is unit-tested); the selection follows it.
pub fn hit_test(map: &MindMap, layout: &Layout, view: &CanvasView, rect: Rect, pointer: Pos2) -> Option<String> {
    let hidden = map.hidden_ids();
    map.nodes
        .iter()
        .filter(|node| !hidden.contains(&node.id))
        .find(|node| {
            let Some(pos) = layout.positions.get(&node.id) else { return false };
            let size = node_size(node);
            card_rect(view, rect, *pos, (size.w, size.h)).contains(pointer)
        })
        .map(|node| node.id.clone())
}

/// What the frame's interaction produced.#[derive(Clone, Debug)]
pub struct CanvasOutput {
    /// A card was clicked.
    pub clicked: Option<String>,
    /// Empty canvas was clicked (the selection goes away).
    pub clicked_empty: bool,
    /// The canvas rect of this frame, which the app uses for a later fit.
    pub rect: Rect,
}

impl Default for CanvasOutput {
    fn default() -> Self {
        Self { clicked: None, clicked_empty: false, rect: Rect::NOTHING }
    }
}

/// Draws the map and handles pan, zoom and click-to-select.
pub fn show(ui: &mut Ui, map: &MindMap, view: &mut CanvasView, selected: Option<&str>) -> CanvasOutput {
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let mut out = CanvasOutput { rect, ..Default::default() };

    let palette = Palette::from(&ui.visuals().clone());
    ui.painter().rect_filled(rect, CornerRadius::ZERO, palette.canvas);

    if response.dragged() {
        view.pan += response.drag_delta();
    }
    if response.hovered() {
        let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
        if let Some(pointer) = ui.input(|i| i.pointer.hover_pos()) {
            if scroll.abs() > 0.0 {
                view.zoom_around((scroll * 0.0015).exp(), pointer, rect);
            }
            if (pinch - 1.0).abs() > f32::EPSILON {
                view.zoom_around(pinch, pointer, rect);
            }
        }
    }
    if view.needs_fit && rect.width() > 1.0 && rect.height() > 1.0 {
        view.fit(map, rect);
        view.needs_fit = false;
    }

    let layout = layout_tree(&map.nodes);
    let origin = view.origin(rect);
    let painter = ui.painter_at(rect);
    let hidden = map.hidden_ids();
    let by_id: std::collections::HashMap<&str, &MindNode> = map.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    dots(&painter, rect, view, origin, &palette);
    edges(&painter, map, &layout, &by_id, &hidden, view, origin, &palette);

    let mut clicked_card = None;
    for node in map.nodes.iter().filter(|n| !hidden.contains(&n.id)) {
        let Some(pos) = layout.positions.get(&node.id) else { continue };
        let size = node_size(node);
        let card = card_rect(view, rect, *pos, (size.w, size.h));
        let is_selected = selected == Some(node.id.as_str());
        draw_card(&painter, card, node, &layout, view, &palette, is_selected, map);
    }
    if response.clicked()
        && let Some(pointer) = ui.input(|i| i.pointer.interact_pos())
    {
        clicked_card = hit_test(map, &layout, view, rect, pointer);
    }
    if response.clicked() && clicked_card.is_none() {
        out.clicked_empty = true;
    }
    out.clicked = clicked_card;
    out
}

/// The colours this frame draws with: the app's theme comes through `visuals`, so a light or dark
/// interface needs no separate palette here.
struct Palette {
    canvas: Color32,
    card: Color32,
    card_root: Color32,
    text: Color32,
    edge: Color32,
    accent: Color32,
    accent_second: Color32,
    selection: Color32,
    note: Color32,
}

impl From<&egui::Visuals> for Palette {
    fn from(visuals: &egui::Visuals) -> Self {
        let accent = visuals.selection.bg_fill;
        let card = visuals.widgets.inactive.bg_fill;
        Self {
            canvas: visuals.extreme_bg_color,
            card,
            // The root is the map's centre: same card, tinted towards the accent, so it reads as the
            // trunk at a glance without inventing a second palette.
            card_root: card.lerp_to_gamma(accent, 0.18),
            text: visuals.text_color(),
            edge: visuals.widgets.noninteractive.bg_stroke.color,
            accent,
            accent_second: visuals.hyperlink_color,
            selection: accent,
            note: visuals.warn_fg_color,
        }
    }
}

/// A sparse dot grid, so panning has a reference. It moves with the layout and is capped.
fn dots(painter: &egui::Painter, rect: Rect, view: &CanvasView, origin: Pos2, palette: &Palette) {
    if view.zoom < 0.4 {
        return;
    }
    let step = DOT_STEP * view.zoom;
    if step < 6.0 {
        return;
    }
    let color = palette.edge.gamma_multiply(0.35);
    let start_x = origin.x - ((origin.x - rect.left()) / step).floor() * step;
    let start_y = origin.y - ((origin.y - rect.top()) / step).floor() * step;
    let mut painted = 0usize;
    let mut y = start_y;
    while y < rect.bottom() {
        let mut x = start_x;
        while x < rect.right() {
            painter.circle_filled(pos2(x, y), 1.0, color);
            painted += 1;
            if painted >= DOT_LIMIT {
                return;
            }
            x += step;
        }
        y += step;
    }
}

/// The links, drawn under the cards: a cubic curve from the parent's centre to the child's, with
/// the control points halfway along the depth axis (what the original app drew).
fn edges(
    painter: &egui::Painter,
    map: &MindMap,
    layout: &Layout,
    by_id: &std::collections::HashMap<&str, &MindNode>,
    hidden: &std::collections::BTreeSet<String>,
    view: &CanvasView,
    origin: Pos2,
    palette: &Palette,
) {
    let stroke = Stroke::new((1.5 * view.zoom).clamp(0.8, 3.0), palette.edge);
    for node in map.nodes.iter() {
        let Some(parent_id) = node.parent_id.as_deref() else { continue };
        if hidden.contains(&node.id) || hidden.contains(parent_id) {
            continue;
        }
        let (Some(parent), Some(child_pos)) = (by_id.get(parent_id), layout.positions.get(&node.id)) else { continue };
        let Some(parent_pos) = layout.positions.get(parent_id) else { continue };
        let parent_size = node_size(parent);
        let child_size = node_size(node);
        let from = view.to_screen(origin, parent_pos.x + parent_size.w / 2.0, parent_pos.y + parent_size.h / 2.0);
        let to = view.to_screen(origin, child_pos.x + child_size.w / 2.0, child_pos.y + child_size.h / 2.0);
        // The control points sit halfway between the centres, on the parent's and child's depth
        // lines: the same curve the exported PNG strokes.
        let vertical = matches!(layout.sides.get(&node.id), Some(ResolvedSide::Up | ResolvedSide::Down));
        let (c1, c2) = if vertical {
            (pos2(from.x, (from.y + to.y) / 2.0), pos2(to.x, (from.y + to.y) / 2.0))
        } else {
            (pos2((from.x + to.x) / 2.0, from.y), pos2((from.x + to.x) / 2.0, to.y))
        };
        painter.add(egui::Shape::CubicBezier(egui::epaint::CubicBezierShape::from_points_stroke(
            [from, c1, c2, to],
            false,
            Color32::TRANSPARENT,
            stroke,
        )));
    }
}

/// One card: background, accent bar, text, collapse badge and note dot.
#[allow(clippy::too_many_arguments)]
fn draw_card(
    painter: &egui::Painter,
    card: Rect,
    node: &MindNode,
    layout: &Layout,
    view: &CanvasView,
    palette: &Palette,
    selected: bool,
    map: &MindMap,
) {
    let is_root = node.parent_id.is_none();
    let radius = (10.0 * view.zoom).clamp(2.0, 12.0);
    let fill = if is_root { palette.card_root } else { palette.card };
    let stroke =
        if selected { Stroke::new((2.0 * view.zoom).clamp(1.0, 3.0), palette.selection) } else { Stroke::new(1.0, palette.edge.gamma_multiply(0.6)) };
    painter.rect(card, CornerRadius::same(radius as u8), fill, stroke, StrokeKind::Inside);

    // The accent bar: the node's own colour when it has one, else the root/secondary accents. The
    // root's is a little heavier, which is what makes the trunk findable in a busy map.
    let accent = node_color(node).unwrap_or(if is_root { palette.accent } else { palette.accent_second });
    let bar_width = ((if is_root { 7.0 } else { 5.0 }) * view.zoom).clamp(2.0, 8.0);
    let bar = Rect::from_min_size(card.min, vec2(bar_width, card.height()));
    painter.rect_filled(bar, CornerRadius { nw: radius as u8, sw: radius as u8, ne: 0, se: 0 }, accent);

    if view.zoom >= TEXT_MIN_ZOOM {
        let font_size = font_of(node) * view.zoom;
        let wrap = (card.width() - 24.0 * view.zoom).max(8.0);
        let galley = painter.layout(node.text.clone(), FontId::proportional(font_size), palette.text, wrap);
        let text_pos = pos2(card.left() + 12.0 * view.zoom, card.center().y - galley.size().y / 2.0);
        painter.galley(text_pos, galley, palette.text);
    }

    // Children that are not drawn because the node is collapsed: the count, so the branch is not
    // silently empty.
    if node.collapsed == Some(true) {
        let count = map.children(&node.id).len();
        if count > 0 {
            let at = pos2(card.right() - 8.0 * view.zoom, card.top() + 8.0 * view.zoom);
            painter.circle_filled(at, (8.0 * view.zoom).clamp(3.0, 9.0), palette.accent);
            if view.zoom >= TEXT_MIN_ZOOM {
                painter.text(
                    at,
                    egui::Align2::CENTER_CENTER,
                    count.to_string(),
                    FontId::proportional((10.0 * view.zoom).clamp(6.0, 11.0)),
                    palette.canvas,
                );
            }
        }
    }

    // A node with a note says so; the note itself is edited from the card's menu.
    if node.has_note() {
        painter.circle_filled(pos2(card.left() + 14.0 * view.zoom, card.bottom() - 8.0 * view.zoom), (3.0 * view.zoom).clamp(1.5, 4.0), palette.note);
    }
    let _ = layout;
}

/// A node's own colour, when it holds a `#rrggbb` the UI can draw. Anything else falls back to the
/// theme, which is what the original app did with an unparsable value.
fn node_color(node: &MindNode) -> Option<Color32> {
    let hex = node.color.as_deref()?.trim();
    let hex = hex.strip_prefix('#')?;
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).ok();
    Some(Color32::from_rgb(channel(0)?, channel(2)?, channel(4)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use localmind_model::MindMap;

    fn rect() -> Rect {
        Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))
    }

    /// Fit puts the whole map inside the canvas, centred, whatever the zoom ended at.
    #[test]
    fn fit_frames_the_whole_map() {
        let mut map = MindMap::new("t", "central");
        let root = map.root_id().map(str::to_string).unwrap_or_default();
        for text in ["um", "dois", "três"] {
            map.add_child(&root, text, None);
        }
        let mut view = CanvasView::default();
        view.fit(&map, rect());
        let layout = layout_tree(&map.nodes);
        let (origin, size) = layout.bounds(&map.nodes).expect("bounds");
        let screen_origin = view.origin(rect());
        let top_left = view.to_screen(screen_origin, origin.x, origin.y);
        let bottom_right = view.to_screen(screen_origin, origin.x + size.w, origin.y + size.h);
        assert!(top_left.x >= rect().left() - 0.5 && top_left.y >= rect().top() - 0.5);
        assert!(bottom_right.x <= rect().right() + 0.5 && bottom_right.y <= rect().bottom() + 0.5);
        // Centred on both axes.
        let center = pos2((top_left.x + bottom_right.x) / 2.0, (top_left.y + bottom_right.y) / 2.0);
        assert!((center.x - rect().center().x).abs() < 0.5);
        assert!((center.y - rect().center().y).abs() < 0.5);
    }

    /// Zooming keeps the layout point under the pointer: the canvas grows around the cursor, it
    /// does not slide away from it.
    #[test]
    fn zooming_keeps_the_point_under_the_pointer() {
        let mut view = CanvasView::default();
        let pointer = pos2(300.0, 200.0);
        let before_origin = view.origin(rect());
        let world = (pointer - before_origin) / view.zoom;
        view.zoom_around(1.7, pointer, rect());
        let after_origin = view.origin(rect());
        let again = after_origin + world * view.zoom;
        assert!((again.x - pointer.x).abs() < 0.01, "{again:?} vs {pointer:?}");
        assert!((again.y - pointer.y).abs() < 0.01);
    }

    #[test]
    fn the_zoom_is_clamped_to_the_range() {
        let mut view = CanvasView::default();
        view.zoom_around(1_000.0, rect().center(), rect());
        assert_eq!(view.zoom, MAX_ZOOM);
        view.zoom_around(0.000_01, rect().center(), rect());
        assert_eq!(view.zoom, MIN_ZOOM);
    }

    #[test]
    fn only_a_full_hex_colour_is_a_card_colour() {
        let mut node = MindNode::new("a", "A", None);
        node.color = Some("#7c5cff".into());
        assert!(node_color(&node).is_some());
        node.color = Some("7c5cff".into());
        assert!(node_color(&node).is_none());
        node.color = Some("#zzzzzz".into());
        assert!(node_color(&node).is_none());
        node.color = Some("#fff".into());
        assert!(node_color(&node).is_none());
    }

    #[test]
    fn an_empty_map_fits_without_panicking() {
        let map = MindMap { id: "m".into(), title: "t".into(), nodes: Vec::new(), created_at: 0, updated_at: 0, extra: Default::default() };
        let mut view = CanvasView::default();
        view.fit(&map, rect());
        assert_eq!(view.zoom, 1.0);
    }

    /// Cards are painted, not widgets: the click has to be matched against the geometry, and only
    /// against the cards that are actually on screen.
    #[test]
    fn a_click_lands_on_the_card_under_the_pointer() {
        let mut map = MindMap::new("t", "central");
        let root = map.root_id().map(str::to_string).unwrap_or_default();
        let child = map.add_child(&root, "ramo", None);
        map.toggle_collapse(&child);
        let grandchild = map.add_child(&child, "neto", None);
        map.toggle_collapse(&child);

        let layout = layout_tree(&map.nodes);
        let view = CanvasView::default();
        let canvas = rect();

        let center_of = |id: &str| {
            let node = map.node(id).expect("node");
            let size = node_size(node);
            card_rect(&view, canvas, *layout.positions.get(id).expect("placed"), (size.w, size.h)).center()
        };
        assert_eq!(hit_test(&map, &layout, &view, canvas, center_of(&root)).as_deref(), Some(root.as_str()));
        assert_eq!(hit_test(&map, &layout, &view, canvas, center_of(&child)).as_deref(), Some(child.as_str()));
        // The grandchild is inside a collapsed branch: it has no card, so nothing is hit there.
        assert!(!layout.positions.contains_key(&grandchild));
        assert_eq!(hit_test(&map, &layout, &view, canvas, pos2(canvas.right() - 2.0, canvas.bottom() - 2.0)), None);
    }

    /// A card moved by pan and zoom is hit where it is drawn, not where it was laid out.
    #[test]
    fn the_hit_test_follows_pan_and_zoom() {
        let map = MindMap::new("t", "central");
        let root = map.root_id().map(str::to_string).unwrap_or_default();
        let view = CanvasView { zoom: 1.6, pan: vec2(-40.0, 25.0), needs_fit: false };
        let layout = layout_tree(&map.nodes);
        let canvas = rect();
        let node = map.node(&root).expect("root");
        let size = node_size(node);
        let card = card_rect(&view, canvas, *layout.positions.get(&root).expect("placed"), (size.w, size.h));
        assert_eq!(hit_test(&map, &layout, &view, canvas, card.center()).as_deref(), Some(root.as_str()));
        // Just outside the same card there is nothing to hit.
        assert_eq!(hit_test(&map, &layout, &view, canvas, card.right_center() + vec2(4.0, 0.0)), None);
    }
}
