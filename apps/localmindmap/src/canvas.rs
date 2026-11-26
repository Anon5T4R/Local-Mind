//! The map canvas: where every card is drawn, and how the view and the pointer act on it.
//!
//! Geometry comes from `localmindmap-layout` (the port of the original app's balanced four-way
//! layout); this module maps it to the screen, draws it, and turns pointer input into the actions
//! the editor applies ([`CanvasOutput`]). The cards are painted shapes rather than widgets, so the
//! hit tests live here too - and they are unit-tested, because a click that lands on the wrong
//! card is the kind of bug that only shows up in a hand test.

use egui::{Color32, CornerRadius, FontId, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};
use localmindmap_layout::{Layout, ResolvedSide, font_of, layout_tree, node_size, width_of};
use localmindmap_model::{MAX_WIDTH, MIN_WIDTH, MindMap, MindNode, Side};

use crate::tl;

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
/// Radius of the collapse badge, in layout units (before the zoom).
const BADGE_RADIUS: f32 = 8.0;
/// Side of the width handle on the selected card, in layout units (before the zoom).
const HANDLE_SIDE: f32 = 14.0;

/// Radius of the root's "+" buttons, in layout units (a 20 px circle).
const CORNER_RADIUS: f32 = 10.0;
/// Distance from a card edge to the centre of its "+", in layout units. The button's outer edge
/// sits 26 px from the card, as in the original app.
const CORNER_GAP: f32 = 16.0;

/// The view over the map: where the canvas is panned to, how much it is zoomed, and the width drag
/// in progress.
#[derive(Clone, Debug)]
pub struct CanvasView {
    /// Offset of the layout origin from the centre of the canvas, in screen pixels.
    pub pan: Vec2,
    pub zoom: f32,
    /// Set when the map changed enough that the view should be refitted (opened, new, generated).
    pub needs_fit: bool,
    /// The width drag happening right now, if any.
    pub resizing: Option<Resize>,
}

/// A width drag on the selected card: where it started and from which width.
#[derive(Clone, Debug, PartialEq)]
pub struct Resize {
    pub node: String,
    pub start_pointer_x: f32,
    pub start_width: f32,
}

impl Default for CanvasView {
    fn default() -> Self {
        Self { pan: Vec2::ZERO, zoom: 1.0, needs_fit: true, resizing: None }
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
pub fn card_rect(view: &CanvasView, rect: Rect, pos: localmindmap_layout::Pos, size: (f32, f32)) -> Rect {
    let min = view.to_screen(view.origin(rect), pos.x, pos.y);
    Rect::from_min_size(min, vec2(size.0, size.1) * view.zoom)
}

/// The collapse badge of a card: top-right corner, inside the card.
pub fn badge_center(view: &CanvasView, card: Rect) -> Pos2 {
    pos2(card.right() - 8.0 * view.zoom, card.top() + 8.0 * view.zoom)
}

/// Radius of the badge on screen.
pub fn badge_radius(view: &CanvasView) -> f32 {
    (BADGE_RADIUS * view.zoom).clamp(3.0, 9.0)
}

/// The width handle of the selected card: bottom-right corner.
pub fn handle_rect(view: &CanvasView, card: Rect) -> Rect {
    let side = (HANDLE_SIDE * view.zoom).clamp(6.0, 16.0);
    Rect::from_min_max(pos2(card.right() - side, card.bottom() - side), card.max)
}

/// Radius of a root "+" on screen (a 20 px circle at 1x).
pub fn corner_radius(view: &CanvasView) -> f32 {
    (CORNER_RADIUS * view.zoom).clamp(3.0, 11.0)
}

fn corner_gap(view: &CanvasView) -> f32 {
    (CORNER_GAP * view.zoom).clamp(4.0, 20.0)
}

/// The centre of the root's "+" for `side`: just outside that edge of the card.
pub fn corner_center(view: &CanvasView, card: Rect, side: Side) -> Pos2 {
    let gap = corner_gap(view);
    match side {
        Side::Up => pos2(card.center().x, card.top() - gap),
        Side::Down => pos2(card.center().x, card.bottom() + gap),
        Side::Left => pos2(card.left() - gap, card.center().y),
        Side::Right => pos2(card.right() + gap, card.center().y),
    }
}

/// The root's card, when it has a place on the canvas.
fn root_card(map: &MindMap, layout: &Layout, view: &CanvasView, rect: Rect) -> Option<Rect> {
    let root = map.root()?;
    let pos = layout.positions.get(&root.id)?;
    let size = node_size(root);
    Some(card_rect(view, rect, *pos, (size.w, size.h)))
}

/// The root's "+" under `pointer`, if any.
pub fn corner_at(map: &MindMap, layout: &Layout, view: &CanvasView, rect: Rect, pointer: Pos2) -> Option<Side> {
    let card = root_card(map, layout, view, rect)?;
    let radius = corner_radius(view) + 2.0;
    Side::ALL.iter().copied().find(|side| corner_center(view, card, *side).distance(pointer) <= radius)
}

/// The card under `pointer`, if any.
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

/// The collapse badge under `pointer`, if any (checked before the card: it sits inside the card).
pub fn badge_at(map: &MindMap, layout: &Layout, view: &CanvasView, rect: Rect, pointer: Pos2) -> Option<String> {
    let hidden = map.hidden_ids();
    let radius = badge_radius(view) + 2.0;
    map.nodes
        .iter()
        .filter(|node| !hidden.contains(&node.id) && !map.children(&node.id).is_empty())
        .find(|node| {
            let Some(pos) = layout.positions.get(&node.id) else { return false };
            let size = node_size(node);
            let card = card_rect(view, rect, *pos, (size.w, size.h));
            badge_center(view, card).distance(pointer) <= radius
        })
        .map(|node| node.id.clone())
}

/// What the frame's interaction produced. The editor applies these; the canvas never mutates the
/// map, so an undo step is always the caller's decision.
#[derive(Clone, Debug)]
pub struct CanvasOutput {
    /// The canvas rect of this frame, which the app uses for a later fit.
    pub rect: Rect,
    /// A card was clicked (not its badge).
    pub clicked: Option<String>,
    /// Empty canvas was clicked (the selection goes away).
    pub clicked_empty: bool,
    /// A card was double-clicked: start editing its text.
    pub double_clicked: Option<String>,
    /// The collapse badge of a card was clicked.
    pub badge_clicked: Option<String>,
    /// A card was right-clicked: open the context menu there.
    pub context_menu: Option<(String, Pos2)>,
    /// The width the selected card should get, while its handle is being dragged.
    pub width: Option<(String, f32)>,
    /// The width drag ended this frame.
    pub width_finished: bool,
    /// One of the root's "+" buttons was clicked: add a branch on that side.
    pub hot_corner: Option<Side>,
    /// The canvas's own widget: having the keyboard, it is still the map that has it.
    pub id: Id,
    /// The minimap's widget, for the same reason.
    pub minimap_id: Id,
}

impl Default for CanvasOutput {
    fn default() -> Self {
        Self {
            rect: Rect::NOTHING,
            clicked: None,
            clicked_empty: false,
            double_clicked: None,
            badge_clicked: None,
            context_menu: None,
            width: None,
            width_finished: false,
            hot_corner: None,
            id: Id::NULL,
            minimap_id: Id::NULL,
        }
    }
}

/// Draws the map and turns pointer input into [`CanvasOutput`].
pub fn show(
    ui: &mut Ui,
    map: &MindMap,
    view: &mut CanvasView,
    selected: Option<&str>,
    editing: Option<&str>,
    colors: &crate::theme::Colors,
) -> CanvasOutput {
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let mut out = CanvasOutput { rect, id: response.id, ..Default::default() };

    let layout = layout_tree(&map.nodes);
    interact(
        ui,
        &response,
        rect,
        map,
        view,
        &layout,
        selected,
        editing,
        pointer_over_minimap(rect, ui.input(|i| i.pointer.interact_pos())),
        &mut out,
    );

    paint(&ui.painter_at(rect), map, &layout, view, rect, selected, editing, colors);
    hot_corners(&ui.painter_at(rect), map, &layout, view, rect, colors, ui.input(|i| i.pointer.hover_pos()), editing);
    // The "+" are painted shapes, not widgets, so their tooltip is opened by hand at the pointer.
    if !editing.is_some_and(|id| map.root_id() == Some(id))
        && let Some(side) = ui.input(|i| i.pointer.hover_pos()).and_then(|p| corner_at(map, &layout, view, rect, p))
    {
        egui::Tooltip::always_open(
            ui.ctx().clone(),
            ui.layer_id(),
            ui.make_persistent_id(("hot-corner-tip", side as u8)),
            egui::PopupAnchor::Pointer,
        )
        .show(|ui| ui.label(corner_label(side)));
    }
    minimap(ui, rect, map, &layout, view, colors);
    // The minimap's id is its own persistent one, the same whoever asks for it.
    out.minimap_id = ui.make_persistent_id("localmindmap-minimap");
    hover_cursor(ui, &response, map, &layout, view, rect, selected);
    out
}

/// The map on a painter: background, grid, links and cards. Shared by the window and the PNG export,
/// so what the user sees and what the file holds come from the same code.
#[allow(clippy::too_many_arguments)]
pub fn paint(
    painter: &egui::Painter,
    map: &MindMap,
    layout: &Layout,
    view: &CanvasView,
    rect: Rect,
    selected: Option<&str>,
    editing: Option<&str>,
    colors: &crate::theme::Colors,
) {
    let palette = Palette::from(colors);
    painter.rect_filled(rect, CornerRadius::ZERO, palette.canvas);

    let origin = view.origin(rect);
    let hidden = map.hidden_ids();
    let by_id: std::collections::HashMap<&str, &MindNode> = map.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    dots(painter, rect, view, origin, &palette);
    edges(painter, map, layout, &by_id, &hidden, view, origin, &palette);

    for node in map.nodes.iter().filter(|node| !hidden.contains(&node.id)) {
        let Some(pos) = layout.positions.get(&node.id) else { continue };
        let size = node_size(node);
        let card = card_rect(view, rect, *pos, (size.w, size.h));
        let is_selected = selected == Some(node.id.as_str());
        let is_editing = editing == Some(node.id.as_str());
        draw_card(painter, card, node, view, &palette, is_selected, map, is_editing);
    }
}

/// Zoom, pan, width drag and clicks.
#[allow(clippy::too_many_arguments)]
fn interact(
    ui: &Ui,
    response: &egui::Response,
    rect: Rect,
    map: &MindMap,
    view: &mut CanvasView,
    layout: &Layout,
    selected: Option<&str>,
    editing: Option<&str>,
    over_minimap: bool,
    out: &mut CanvasOutput,
) {
    let pointer = ui.input(|i| i.pointer.interact_pos());
    // The root's "+" buttons: inert while the root's own text field is open, so a click there is
    // the field's, not a new branch's.
    let hot = if editing.is_some_and(|id| map.root_id() == Some(id)) { None } else { pointer.and_then(|p| corner_at(map, layout, view, rect, p)) };

    if response.hovered() && !over_minimap {
        let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
        if let Some(pointer) = pointer {
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

    // A drag that starts on the selected card's handle resizes it; anywhere else pans the canvas.
    // The press origin is what decides: by the time the drag is recognized the pointer has already
    // moved off the handle.
    if response.drag_started()
        && let (Some(pressed_at), Some(id), Some(node)) = (ui.input(|i| i.pointer.press_origin()), selected, selected.and_then(|id| map.node(id)))
        && let Some(pos) = layout.positions.get(id)
    {
        let size = node_size(node);
        let card = card_rect(view, rect, *pos, (size.w, size.h));
        if handle_rect(view, card).contains(pressed_at) {
            view.resizing = Some(Resize { node: id.to_string(), start_pointer_x: pressed_at.x, start_width: width_of(node) });
        }
    }

    if let Some(resize) = view.resizing.clone() {
        if response.dragged() {
            if let Some(pointer) = pointer {
                let width = (resize.start_width + (pointer.x - resize.start_pointer_x) / view.zoom).clamp(MIN_WIDTH, MAX_WIDTH);
                out.width = Some((resize.node.clone(), width));
            }
        } else if !response.drag_started() {
            view.resizing = None;
            out.width_finished = true;
        }
    } else if response.dragged() && !over_minimap && hot.is_none() {
        view.pan += response.drag_delta();
    }

    // The minimap handles its own clicks and drags (it is drawn on top of the canvas).
    if over_minimap {
        return;
    }
    let Some(pointer) = pointer else { return };
    // The root's "+" buttons come before the cards and the empty canvas: a click there adds a
    // branch on that side, and does nothing else.
    if let Some(side) = hot {
        if response.clicked() {
            out.hot_corner = Some(side);
        }
        return;
    }
    if response.clicked() {
        if let Some(id) = badge_at(map, layout, view, rect, pointer) {
            out.badge_clicked = Some(id);
        } else if let Some(id) = hit_test(map, layout, view, rect, pointer) {
            out.clicked = Some(id);
        } else {
            out.clicked_empty = true;
        }
    }
    if response.double_clicked()
        && let Some(id) = hit_test(map, layout, view, rect, pointer)
        && badge_at(map, layout, view, rect, pointer).is_none()
    {
        out.double_clicked = Some(id);
    }
    if response.secondary_clicked()
        && let Some(id) = hit_test(map, layout, view, rect, pointer)
    {
        out.context_menu = Some((id, pointer));
    }
}

/// The pointer says what the thing under it does: a card is clickable, the handle and the badge have
/// their own cursor.
fn hover_cursor(ui: &Ui, response: &egui::Response, map: &MindMap, layout: &Layout, view: &CanvasView, rect: Rect, selected: Option<&str>) {
    if !response.hovered() {
        return;
    }
    let Some(pointer) = ui.input(|i| i.pointer.hover_pos()) else { return };
    let cursor = if let Some(node) = selected.and_then(|id| map.node(id)) {
        let card = layout.positions.get(&node.id).map(|pos| {
            let size = node_size(node);
            card_rect(view, rect, *pos, (size.w, size.h))
        });
        match card {
            Some(card) if handle_rect(view, card).contains(pointer) => Some(egui::CursorIcon::ResizeHorizontal),
            _ => None,
        }
    } else {
        None
    };
    let cursor = cursor.or_else(|| badge_at(map, layout, view, rect, pointer).map(|_| egui::CursorIcon::PointingHand));
    let cursor = cursor.or_else(|| corner_at(map, layout, view, rect, pointer).map(|_| egui::CursorIcon::PointingHand));
    let cursor = cursor.or_else(|| hit_test(map, layout, view, rect, pointer).map(|_| egui::CursorIcon::PointingHand));
    let cursor = cursor.or(Some(egui::CursorIcon::Grab));
    if let Some(cursor) = cursor {
        ui.ctx().set_cursor_icon(cursor);
    }
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
    dots: Color32,
}

impl From<&crate::theme::Colors> for Palette {
    fn from(colors: &crate::theme::Colors) -> Self {
        let accent = colors.accent;
        let card = colors.panel;
        Self {
            canvas: colors.canvas,
            card,
            // The root is the map's centre: same card, tinted towards the accent, so it reads as the
            // trunk at a glance without inventing a second palette.
            card_root: card.lerp_to_gamma(accent, 0.18),
            text: colors.text,
            edge: colors.border,
            accent,
            accent_second: colors.accent_second,
            selection: accent,
            note: colors.accent_second,
            dots: colors.dots,
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
    let color = palette.dots;
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

/// Size and margin of the minimap, in screen pixels.
const MINIMAP_SIZE: Vec2 = vec2(184.0, 124.0);
/// Distance from the canvas corner.
const MINIMAP_MARGIN: f32 = 12.0;

/// The minimap rectangle (bottom-right corner), or `None` when the canvas is too small for one.
pub fn minimap_rect(canvas: Rect) -> Option<Rect> {
    if canvas.width() < 2.0 * MINIMAP_SIZE.x + 40.0 || canvas.height() < 2.0 * MINIMAP_SIZE.y + 40.0 {
        return None;
    }
    let min = pos2(canvas.right() - MINIMAP_MARGIN - MINIMAP_SIZE.x, canvas.bottom() - MINIMAP_MARGIN - MINIMAP_SIZE.y);
    Some(Rect::from_min_size(min, MINIMAP_SIZE))
}

/// Whether the pointer is over the minimap (the canvas must not act on those clicks).
pub fn pointer_over_minimap(canvas: Rect, pointer: Option<Pos2>) -> bool {
    matches!((minimap_rect(canvas), pointer), (Some(rect), Some(point)) if rect.contains(point))
}

/// The slice of layout space the canvas is showing right now.
pub fn world_view(view: &CanvasView, canvas: Rect) -> Rect {
    let top_left = (canvas.left_top() - canvas.center() - view.pan) / view.zoom;
    let bottom_right = (canvas.right_bottom() - canvas.center() - view.pan) / view.zoom;
    Rect::from_min_max(pos2(top_left.x, top_left.y), pos2(bottom_right.x, bottom_right.y))
}

/// How the minimap maps layout space to its own box: the map and the viewport together, centred.
pub struct MinimapFit {
    origin: Pos2,
    scale: f32,
}

impl MinimapFit {
    pub fn new(rect: Rect, map: Rect, viewport: Rect) -> Self {
        let world = map.union(viewport);
        let inner = rect.shrink(6.0);
        let scale = (inner.width() / world.width().max(1.0)).min(inner.height() / world.height().max(1.0)).max(0.0001);
        let origin = rect.center() - world.center().to_vec2() * scale;
        Self { origin, scale }
    }

    fn to_minimap(&self, point: Pos2) -> Pos2 {
        self.origin + point.to_vec2() * self.scale
    }

    fn to_world(&self, point: Pos2) -> Pos2 {
        ((point - self.origin) / self.scale).to_pos2()
    }
}

/// The minimap: the whole map, the viewport and a click/drag that moves the view.
fn minimap(ui: &Ui, canvas: Rect, map: &MindMap, layout: &Layout, view: &mut CanvasView, colors: &crate::theme::Colors) {
    let Some(rect) = minimap_rect(canvas) else { return };
    let Some((map_min, map_size)) = layout.bounds(&map.nodes) else { return };
    let map_rect = Rect::from_min_size(pos2(map_min.x, map_min.y), vec2(map_size.w, map_size.h));
    let viewport = world_view(view, canvas);
    let fit = MinimapFit::new(rect, map_rect, viewport);

    let painter = ui.painter_at(rect);
    painter.rect(rect, CornerRadius::same(8), colors.surface, Stroke::new(1.0, colors.border), StrokeKind::Inside);

    for node in map.nodes.iter() {
        let Some(pos) = layout.positions.get(&node.id) else { continue };
        let size = node_size(node);
        let place = Rect::from_min_size(fit.to_minimap(pos2(pos.x, pos.y)), vec2(size.w, size.h) * fit.scale);
        let place = Rect::from_min_size(place.min, place.size().max(vec2(2.0, 2.0)));
        painter.rect_filled(place, CornerRadius::same(1), colors.minimap_node);
    }

    // What is outside the viewport is dimmed, so the minimap says where you are.
    let seen = Rect::from_min_size(fit.to_minimap(viewport.min), viewport.size() * fit.scale).intersect(rect);
    if seen.is_positive() {
        let mask = colors.minimap_mask;
        painter.rect_filled(Rect::from_min_max(rect.min, pos2(rect.right(), seen.top())), CornerRadius::ZERO, mask);
        painter.rect_filled(Rect::from_min_max(pos2(rect.left(), seen.bottom()), rect.max), CornerRadius::ZERO, mask);
        painter.rect_filled(Rect::from_min_max(pos2(rect.left(), seen.top()), pos2(seen.left(), seen.bottom())), CornerRadius::ZERO, mask);
        painter.rect_filled(Rect::from_min_max(pos2(seen.right(), seen.top()), pos2(rect.right(), seen.bottom())), CornerRadius::ZERO, mask);
        painter.rect_stroke(seen, CornerRadius::ZERO, Stroke::new(1.0, colors.accent), StrokeKind::Inside);
    }

    let response = ui.interact(rect, ui.make_persistent_id("localmindmap-minimap"), Sense::click_and_drag());
    if (response.clicked() || response.dragged())
        && let Some(pointer) = ui.input(|i| i.pointer.interact_pos())
    {
        // Centre the canvas on the layout point that was clicked.
        let target = fit.to_world(pointer);
        view.pan = -target.to_vec2() * view.zoom;
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

/// The root's four "+" buttons, drawn just outside its card: the fast way to add a branch on a
/// direction. They are screen chrome, not part of the map, so the PNG export (which calls
/// [`paint`]) never draws them.
#[allow(clippy::too_many_arguments)]
fn hot_corners(
    painter: &egui::Painter,
    map: &MindMap,
    layout: &Layout,
    view: &CanvasView,
    rect: Rect,
    colors: &crate::theme::Colors,
    pointer: Option<Pos2>,
    editing: Option<&str>,
) {
    if editing.is_some_and(|id| map.root_id() == Some(id)) {
        return;
    }
    let Some(card) = root_card(map, layout, view, rect) else { return };
    let radius = corner_radius(view);
    let hovered = pointer.and_then(|p| corner_at(map, layout, view, rect, p));
    for side in Side::ALL {
        let center = corner_center(view, card, side);
        let hot = hovered == Some(side);
        let fill = if hot { colors.accent } else { colors.surface };
        painter.circle_filled(center, radius, fill);
        dashed_circle(painter, center, radius, colors.accent, (view.zoom).clamp(0.5, 1.5));
        let arm = radius * 0.45;
        let ink = if hot { colors.on_accent() } else { colors.accent };
        let stroke = Stroke::new((1.5 * view.zoom).clamp(1.0, 2.5), ink);
        painter.line_segment([pos2(center.x - arm, center.y), pos2(center.x + arm, center.y)], stroke);
        painter.line_segment([pos2(center.x, center.y - arm), pos2(center.x, center.y + arm)], stroke);
    }
}

/// A dashed circle: `dashes` short arcs with gaps, which is how the original app's "+" buttons were
/// outlined. egui has no dashed stroke, so the arcs are drawn as short polylines.
fn dashed_circle(painter: &egui::Painter, center: Pos2, radius: f32, color: Color32, width: f32) {
    let dashes = 12;
    let stroke = Stroke::new(width, color);
    let step = std::f32::consts::TAU / dashes as f32;
    for i in 0..dashes {
        let start = i as f32 * step;
        let end = start + step * 0.55;
        let mut points = Vec::with_capacity(4);
        for n in 0..=3 {
            let angle = start + (end - start) * (n as f32 / 3.0);
            points.push(center + vec2(angle.cos() * radius, angle.sin() * radius));
        }
        painter.add(egui::Shape::line(points, stroke));
    }
}

/// What a root "+" creates, in the words of the original app's tooltips.
fn corner_label(side: Side) -> &'static str {
    match side {
        Side::Up => tl!("New branch above"),
        Side::Down => tl!("New branch below"),
        Side::Left => tl!("New branch to the left"),
        Side::Right => tl!("New branch to the right"),
    }
}

/// The links, drawn under the cards: a cubic curve from the parent's centre to the child's, with
/// the control points halfway along the depth axis (what the original app drew).#[allow(clippy::too_many_arguments)]
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

/// One card: background, accent bar, text, collapse badge, note dot and (when selected) the width
/// handle.
fn draw_card(
    painter: &egui::Painter,
    card: Rect,
    node: &MindNode,
    view: &CanvasView,
    palette: &Palette,
    selected: bool,
    map: &MindMap,
    editing: bool,
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

    // The node being edited draws no text: the text field over the card is what shows the draft.
    if !editing && view.zoom >= TEXT_MIN_ZOOM {
        let font_size = font_of(node) * view.zoom;
        let wrap = (card.width() - 24.0 * view.zoom).max(8.0);
        let galley = painter.layout(node.text.clone(), FontId::proportional(font_size), palette.text, wrap);
        let text_pos = pos2(card.left() + 12.0 * view.zoom, card.center().y - galley.size().y / 2.0);
        painter.galley(text_pos, galley, palette.text);
    }
    // A card too small to show text still shows that it is being edited.
    if editing {
        painter.rect_stroke(card, CornerRadius::same(radius as u8), Stroke::new(1.0, palette.selection), StrokeKind::Inside);
    }

    // Children that are not drawn because the node is collapsed: the count, so the branch is not
    // silently empty.
    if node.collapsed == Some(true) {
        let count = map.children(&node.id).len();
        if count > 0 {
            let at = badge_center(view, card);
            painter.circle_filled(at, badge_radius(view), palette.accent);
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

    // The width handle lives on the selected card only: it is the one that can be resized.
    if selected {
        let handle = handle_rect(view, card);
        let r = (handle.width() / 2.0).max(2.0);
        painter.circle_filled(handle.center(), r, palette.selection);
        painter.circle_filled(handle.center(), r * 0.45, palette.canvas);
    }
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
    use localmindmap_layout::{NODE_H, NODE_W};
    use localmindmap_model::MindMap;

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
        let view = CanvasView { zoom: 1.6, pan: vec2(-40.0, 25.0), needs_fit: false, resizing: None };
        let layout = layout_tree(&map.nodes);
        let canvas = rect();
        let node = map.node(&root).expect("root");
        let size = node_size(node);
        let card = card_rect(&view, canvas, *layout.positions.get(&root).expect("placed"), (size.w, size.h));
        assert_eq!(hit_test(&map, &layout, &view, canvas, card.center()).as_deref(), Some(root.as_str()));
        // Just outside the same card there is nothing to hit.
        assert_eq!(hit_test(&map, &layout, &view, canvas, card.right_center() + vec2(4.0, 0.0)), None);
    }

    /// The badge sits inside the card and wins over it: clicking it collapses, it does not select.
    #[test]
    fn the_badge_is_hit_before_the_card_body() {
        let mut map = MindMap::new("t", "central");
        let root = map.root_id().map(str::to_string).unwrap_or_default();
        map.add_child(&root, "ramo", None);
        map.toggle_collapse(&root);
        let layout = layout_tree(&map.nodes);
        let view = CanvasView::default();
        let canvas = rect();
        let node = map.node(&root).expect("root");
        let size = node_size(node);
        let card = card_rect(&view, canvas, *layout.positions.get(&root).expect("placed"), (size.w, size.h));
        assert_eq!(badge_at(&map, &layout, &view, canvas, badge_center(&view, card)).as_deref(), Some(root.as_str()));

        // A leaf has no badge, even at the same corner (its root stays expanded here).
        let mut leaf_map = MindMap::new("t", "central");
        let leaf_root = leaf_map.root_id().map(str::to_string).unwrap_or_default();
        let leaf = leaf_map.add_child(&leaf_root, "folha", None);
        let leaf_layout = layout_tree(&leaf_map.nodes);
        let leaf_node = leaf_map.node(&leaf).expect("leaf");
        let leaf_size = node_size(leaf_node);
        let leaf_card = card_rect(&view, canvas, *leaf_layout.positions.get(&leaf).expect("placed"), (leaf_size.w, leaf_size.h));
        assert_eq!(badge_at(&leaf_map, &leaf_layout, &view, canvas, badge_center(&view, leaf_card)), None);
    }

    /// The handle is the bottom-right corner of the selected card only.
    #[test]
    fn the_width_handle_is_at_the_corner_of_the_card() {
        let view = CanvasView::default();
        let card = Rect::from_min_size(pos2(100.0, 100.0), vec2(NODE_W, NODE_H));
        let handle = handle_rect(&view, card);
        assert!(card.contains_rect(handle));
        assert_eq!(handle.max, card.max);
        assert!(!handle.contains(card.center()));
    }

    /// The minimap maps layout space onto its own box and back: the click that moves the view relies
    /// on the inverse.
    #[test]
    fn the_minimap_fit_maps_layout_space_both_ways() {
        let minimap = Rect::from_min_size(pos2(900.0, 600.0), vec2(184.0, 124.0));
        let map = Rect::from_min_size(pos2(-300.0, -200.0), vec2(600.0, 400.0));
        let viewport = Rect::from_min_size(pos2(-100.0, -50.0), vec2(200.0, 100.0));
        let fit = MinimapFit::new(minimap, map, viewport);
        for point in [map.min, map.center(), map.max, viewport.center()] {
            let there = fit.to_minimap(point);
            let back = fit.to_world(there);
            assert!((back.x - point.x).abs() < 0.01, "{point:?} -> {back:?}");
            assert!((back.y - point.y).abs() < 0.01, "{point:?} -> {back:?}");
            assert!(minimap.expand(2.0).contains(there), "{point:?} maps outside the minimap: {there:?}");
        }
    }

    /// The slice of layout space the canvas shows is the inverse of its own transform.
    #[test]
    fn the_world_view_matches_the_mapping() {
        let canvas = rect();
        let view = CanvasView { zoom: 1.5, pan: vec2(30.0, -20.0), needs_fit: false, resizing: None };
        let world = world_view(&view, canvas);
        let screen = card_rect(&view, canvas, localmindmap_layout::Pos { x: world.center().x, y: world.center().y }, (0.0, 0.0)).min;
        assert!((screen.x - canvas.center().x).abs() < 0.01);
        assert!((screen.y - canvas.center().y).abs() < 0.01);
    }

    /// A canvas too small for the minimap simply has none, and clicks there are canvas clicks.
    #[test]
    fn a_small_canvas_has_no_minimap() {
        let small = Rect::from_min_size(pos2(0.0, 0.0), vec2(300.0, 200.0));
        assert!(minimap_rect(small).is_none());
        assert!(!pointer_over_minimap(small, Some(small.center())));
        let big = rect();
        assert!(minimap_rect(big).is_some());
        assert!(pointer_over_minimap(big, minimap_rect(big).map(|r| r.center())));
    }
}
