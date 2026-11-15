//! Editing the map: the selection, the inline text field, the node menu, the note window, the
//! keyboard and the undo history.
//!
//! The commands are the ones the original app had: Tab adds a child, Enter a sibling, Delete removes
//! the subtree (and selects the parent), Space collapses, the arrows navigate, Alt+arrows move the
//! node around its parent, F2 or a double click edits the text, Ctrl+Z/Ctrl+Y undo and redo. The one
//! thing this app does differently is the note: it is editable here (the card's menu -> Note...),
//! where the original drew the dot but offered no way to write it.
//!
//! IME: the inline field is an egui `TextEdit`, which tracks the composition itself. What it cannot
//! tell this module is whether the Enter in this frame's input belongs to the text or to the
//! candidate list, so the composition is tracked from the IME events and Enter never commits while
//! one is active - the difference between typing Japanese and losing the word being picked.

use egui::{Context, Key, Pos2, Rect, Ui, vec2};

use localmind_layout::{ResolvedSide, font_of, layout_tree, node_size, resolve_sides};
use localmind_model::{History, MAX_FONT, MIN_FONT, MindMap, Side};

use crate::canvas::{self, CanvasView};
use crate::doc::Document;
use crate::tl;

/// The colours the node menu offers: the same six the original app had.
pub const COLORS: [&str; 6] = ["#7c5cff", "#3a8dde", "#27ae60", "#e67e22", "#e74c3c", "#e84393"];
/// How much one "A+" / "A-" step changes the card font.
const FONT_STEP: f32 = 2.0;

/// One open map and everything the user is doing to it.
pub struct Editor {
    pub doc: Document,
    pub view: CanvasView,
    pub selected: Option<String>,
    pub editing: Option<Editing>,
    pub menu: Option<MenuState>,
    pub note: Option<NoteState>,
    /// The canvas rect of the last frame (the zoom commands need it).
    pub rect: Rect,
    history: History,
    /// A width drag is in progress: its undo step was already recorded.
    resizing: bool,
}

/// The inline text field over a card.
pub struct Editing {
    pub node: String,
    pub draft: String,
    /// The first frame asks for focus and selects the whole text.
    focus_requested: bool,
    /// An IME composition is on: Enter belongs to the candidate list, not to the commit.
    composing: bool,
}

/// The card menu: which node and where it opened.
#[derive(Clone, Debug)]
pub struct MenuState {
    pub node: String,
    pub at: Pos2,
}

/// The note window.
pub struct NoteState {
    pub node: String,
    pub draft: String,
}

impl Editor {
    pub fn new(doc: Document) -> Self {
        let selected = doc.root_id();
        Self {
            doc,
            view: CanvasView::default(),
            selected,
            editing: None,
            menu: None,
            note: None,
            rect: Rect::NOTHING,
            history: History::default(),
            resizing: false,
        }
    }

    /// Swaps in another document (opened, new) and forgets the editing state.
    pub fn replace(&mut self, doc: Document) {
        self.doc = doc;
        self.selected = self.doc.root_id();
        self.view = CanvasView::default();
        self.editing = None;
        self.menu = None;
        self.note = None;
        self.history.clear();
        self.resizing = false;
    }

    /// The document was just written to disk.
    pub fn mark_saved(&mut self) {
        self.doc.dirty = false;
    }

    pub fn fit(&mut self) {
        self.view.needs_fit = true;
    }

    pub fn zoom_step(&mut self, factor: f32) {
        self.view.zoom_around(factor, self.rect.center(), self.rect);
    }

    /// Applies an edit and records one undo step when it changed something.
    fn edit(&mut self, apply: impl FnOnce(&mut MindMap) -> bool) -> bool {
        let before = self.doc.map.clone();
        if !apply(&mut self.doc.map) {
            return false;
        }
        self.history.record(before);
        self.doc.dirty = true;
        true
    }

    // ---- commands the menus, the toolbar and the keyboard call ------------------------------

    /// Adds a child to `node`, selects it and opens its text field (Tab).
    pub fn add_child(&mut self, node: &str) {
        let mut created = None;
        self.edit(|map| {
            created = Some(map.add_child(node, tl!("New node"), None));
            true
        });
        if let Some(id) = created {
            self.selected = Some(id.clone());
            // A child can land outside the canvas: bring it into view before opening its field, or
            // the user types into a text box that is not over the card it belongs to.
            self.reveal_selected();
            self.begin_edit(&id);
        }
    }

    /// Adds a sibling after `node`, selecting and editing it (Enter). A root child freezes both
    /// sides first: auto-balancing is unstable while children are being added.
    pub fn add_sibling(&mut self, node: &str) {
        let keep_side = {
            let map = &self.doc.map;
            match map.node(node).and_then(|n| n.parent_id.clone()) {
                Some(parent) if Some(parent.as_str()) == map.root_id() => {
                    resolve_sides(&map.nodes).get(node).copied().and_then(ResolvedSide::as_side)
                }
                _ => None,
            }
        };
        let mut created = None;
        self.edit(|map| {
            created = map.add_sibling(node, tl!("New node"), keep_side);
            created.is_some()
        });
        if let Some(id) = created {
            self.selected = Some(id.clone());
            self.reveal_selected();
            self.begin_edit(&id);
        }
    }

    /// Removes `node` and its subtree, selecting the parent (Delete).
    pub fn delete(&mut self, node: &str) {
        let mut parent = None;
        let removed = self.edit(|map| {
            parent = map.delete_node(node);
            parent.is_some()
        });
        if removed {
            self.editing = None;
            self.selected = parent;
        }
    }

    pub fn toggle_collapse(&mut self, node: &str) {
        self.edit(|map| map.toggle_collapse(node));
    }

    pub fn set_side(&mut self, node: &str, side: Option<Side>) {
        self.edit(|map| {
            map.set_side(node, side);
            true
        });
    }

    pub fn set_color(&mut self, node: &str, color: Option<String>) {
        self.edit(|map| {
            map.set_color(node, color);
            true
        });
    }

    /// One "A+" / "A-" step, clamped by the model.
    pub fn bump_font(&mut self, node: &str, delta: f32) {
        let current = self.doc.map.node(node).map_or(localmind_layout::DEFAULT_FONT, font_of);
        let next = (current + delta).clamp(MIN_FONT, MAX_FONT);
        self.edit(|map| {
            map.set_font_size(node, Some(next));
            true
        });
    }

    pub fn reset_width(&mut self, node: &str) {
        self.edit(|map| {
            map.set_width(node, None);
            true
        });
    }

    /// Writes the note of a node (or removes it when blank).
    pub fn set_note(&mut self, node: &str, note: Option<String>) {
        self.edit(|map| {
            map.set_note(node, note);
            true
        });
    }

    pub fn begin_edit(&mut self, node: &str) {
        let Some(target) = self.doc.map.node(node) else { return };
        self.editing = Some(Editing { node: node.to_string(), draft: target.text.clone(), focus_requested: false, composing: false });
        self.menu = None;
    }

    /// Ends the inline edit, writing the draft. An empty text is not a rename: it reverts.
    pub fn commit_edit(&mut self) {
        let Some(editing) = self.editing.take() else { return };
        let trimmed = editing.draft.trim();
        if trimmed.is_empty() {
            return;
        }
        let changed = self.doc.map.node(&editing.node).is_some_and(|node| node.text != trimmed);
        if changed {
            let node = editing.node;
            let text = trimmed.to_string();
            self.edit(|map| map.set_text(&node, &text));
        }
    }

    pub fn cancel_edit(&mut self) {
        self.editing = None;
    }

    pub fn open_note(&mut self, node: &str) {
        let Some(target) = self.doc.map.node(node) else { return };
        self.note = Some(NoteState { node: node.to_string(), draft: target.note.clone().unwrap_or_default() });
        self.menu = None;
    }

    pub fn undo(&mut self) {
        let current = self.doc.map.clone();
        if let Some(previous) = self.history.undo(current) {
            self.doc.map = previous;
            self.doc.dirty = true;
            self.editing = None;
            self.menu = None;
            self.note = None;
            self.fix_selection();
        }
    }

    pub fn redo(&mut self) {
        let current = self.doc.map.clone();
        if let Some(next) = self.history.redo(current) {
            self.doc.map = next;
            self.doc.dirty = true;
            self.editing = None;
            self.menu = None;
            self.note = None;
            self.fix_selection();
        }
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// The selection must always name a node that exists (an undo can delete it).
    fn fix_selection(&mut self) {
        let alive = self.selected.as_deref().is_some_and(|id| self.doc.map.node(id).is_some());
        if !alive {
            self.selected = self.doc.root_id();
        }
    }

    /// Moves the selection through the tree (arrows).
    pub fn navigate(&mut self, key: Key) {
        let Some(current) = self.selected.clone() else { return };
        let map = &self.doc.map;
        let Some(node) = map.node(&current) else { return };
        let next = match key {
            Key::ArrowLeft => node.parent_id.clone(),
            Key::ArrowRight => {
                if node.collapsed == Some(true) {
                    self.toggle_collapse(&current);
                    return;
                }
                map.children(&current).first().map(|child| child.id.clone())
            }
            Key::ArrowUp | Key::ArrowDown => {
                let siblings: Vec<String> = map.nodes.iter().filter(|n| n.parent_id == node.parent_id).map(|n| n.id.clone()).collect();
                let at = siblings.iter().position(|id| id == &current);
                match (at, key) {
                    (Some(0), Key::ArrowUp) | (Some(_), Key::ArrowUp) => at.and_then(|i| siblings.get(i.wrapping_sub(1)).cloned()),
                    (Some(i), Key::ArrowDown) => siblings.get(i + 1).cloned(),
                    (None, _) => None,
                    _ => None,
                }
            }
            _ => None,
        };
        if let Some(next) = next {
            self.selected = Some(next);
            self.reveal_selected();
        }
    }

    /// Moves the node itself: up/down among siblings, right to indent, left to outdent.
    pub fn structural(&mut self, key: Key) {
        let Some(current) = self.selected.clone() else { return };
        self.edit(|map| match key {
            Key::ArrowUp => map.move_up(&current),
            Key::ArrowDown => map.move_down(&current),
            Key::ArrowRight => map.indent(&current),
            Key::ArrowLeft => map.outdent(&current),
            _ => false,
        });
    }

    /// Pans the view just enough to show the selected card (navigation can walk off screen).
    fn reveal_selected(&mut self) {
        if self.rect == Rect::NOTHING {
            return;
        }
        let Some(id) = self.selected.clone() else { return };
        let Some(node) = self.doc.map.node(&id) else { return };
        let layout = layout_tree(&self.doc.map.nodes);
        let Some(pos) = layout.positions.get(&id) else { return };
        let size = node_size(node);
        let card = canvas::card_rect(&self.view, self.rect, *pos, (size.w, size.h));
        let margin = 24.0;
        let view = self.rect.shrink(margin);
        let mut delta = vec2(0.0, 0.0);
        if card.left() < view.left() {
            delta.x = view.left() - card.left();
        } else if card.right() > view.right() {
            delta.x = view.right() - card.right();
        }
        if card.top() < view.top() {
            delta.y = view.top() - card.top();
        } else if card.bottom() > view.bottom() {
            delta.y = view.bottom() - card.bottom();
        }
        if delta != vec2(0.0, 0.0) {
            self.view.pan += delta;
        }
    }

    // ---- the frame ---------------------------------------------------------------------------

    /// The map area: canvas, inline field, keyboard, card menu and note window.
    pub fn ui(&mut self, ui: &mut Ui) {
        // Read before any widget runs: the text field consumes the keys it uses, Enter included,
        // and the commit has to see the Enter of the frame it happened in.
        let keys = KeysThisFrame::read(ui.ctx());
        let editing = self.editing.as_ref().map(|e| e.node.clone());
        let out = canvas::show(ui, &self.doc.map, &mut self.view, self.selected.as_deref(), editing.as_deref());
        self.rect = out.rect;
        self.apply(out);
        // The field goes before the keyboard so the frame that commits is the frame that typed -
        // and the key that committed belongs to the field, not to the map commands.
        let field_used_the_keys = self.inline_editor(ui.ctx(), keys);
        if !field_used_the_keys {
            self.keyboard(ui.ctx());
        }
        self.card_menu(ui.ctx());
        self.note_window(ui.ctx());
    }

    fn apply(&mut self, out: canvas::CanvasOutput) {
        if let Some(id) = out.clicked.clone() {
            self.selected = Some(id);
            self.menu = None;
        }
        if out.clicked_empty {
            self.selected = None;
            self.menu = None;
        }
        if let Some(id) = out.double_clicked {
            self.selected = Some(id.clone());
            self.begin_edit(&id);
        }
        if let Some(id) = out.badge_clicked {
            self.toggle_collapse(&id);
        }
        if let Some((id, at)) = out.context_menu {
            self.selected = Some(id.clone());
            self.menu = Some(MenuState { node: id, at });
        }
        if let Some((id, width)) = out.width {
            self.resize_width(&id, width);
        }
        if out.width_finished {
            self.resizing = false;
        }
    }

    /// A width drag is one undo step, recorded when it first changes something.
    fn resize_width(&mut self, node: &str, width: f32) {
        let current = self.doc.map.node(node).map(localmind_layout::width_of);
        if !self.resizing {
            if current.is_some_and(|current| (current - width).abs() < 0.5) {
                return;
            }
            let before = self.doc.map.clone();
            self.history.record(before);
            self.resizing = true;
        }
        self.doc.map.set_width(node, Some(width));
        self.doc.dirty = true;
    }

    /// The inline text field, over the card being edited. `true` when it used this frame's Enter or
    /// Escape: the map commands must not see the key that closed the field.
    fn inline_editor(&mut self, ctx: &Context, keys: KeysThisFrame) -> bool {
        let Some(mut editing) = self.editing.take() else { return false };
        editing.track_ime(ctx);
        let Some(node) = self.doc.map.node(&editing.node) else { return true };
        let layout = layout_tree(&self.doc.map.nodes);
        let Some(pos) = layout.positions.get(&editing.node).copied() else {
            self.editing = Some(editing);
            return true;
        };
        let size = node_size(node);
        let zoom = self.view.zoom;
        let card = canvas::card_rect(&self.view, self.rect, pos, (size.w, size.h));
        let font = (font_of(node) * zoom).clamp(11.0, 40.0);

        let mut commit = false;
        let mut cancel = false;
        let area = egui::Area::new(egui::Id::unique("localmind-inline-editor"))
            .fixed_pos(card.left_top() + vec2(4.0 * zoom, 3.0 * zoom))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let width = (card.width() - 8.0 * zoom - 8.0).max(60.0);
                ui.set_max_width(width);
                let visuals = ui.style().visuals.clone();
                egui::Frame::NONE
                    .fill(visuals.widgets.inactive.bg_fill)
                    .stroke(egui::Stroke::new(1.5, visuals.selection.bg_fill))
                    .corner_radius(egui::CornerRadius::same(6))
                    .inner_margin(egui::Margin::symmetric(4, 2))
                    .show(ui, |ui| {
                        let output = egui::TextEdit::multiline(&mut editing.draft)
                            .id(egui::Id::unique("localmind-inline-editor-text"))
                            .font(egui::FontId::proportional(font))
                            .text_color(visuals.text_color())
                            .desired_rows(1)
                            .desired_width(width)
                            .frame(egui::Frame::NONE)
                            .margin(egui::Margin::same(2))
                            .show(ui);
                        let response = output.response.response;
                        if !editing.focus_requested {
                            response.request_focus();
                            // Everything selected: a double click means "replace this text".
                            let mut state = output.state;
                            let chars = editing.draft.chars().count();
                            state
                                .cursor
                                .set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(chars))));
                            state.store(ui.ctx(), response.id);
                            editing.focus_requested = true;
                        }
                        if response.lost_focus() {
                            commit = true;
                        }
                        if keys.escape {
                            cancel = true;
                        }
                        // Enter commits - but never while an IME composition is open: there it
                        // belongs to the candidate list. Shift+Enter is a line break.
                        if !editing.composing && keys.enter && !keys.shift {
                            commit = true;
                        }
                    });
            });
        // The menu can steal focus; a click on the canvas already went through `apply`.
        let _ = area;

        if cancel {
            self.editing = Some(editing);
            self.cancel_edit();
            return true;
        }
        if commit {
            self.editing = Some(editing);
            self.commit_edit();
            return true;
        }
        self.editing = Some(editing);
        true
    }

    /// The keyboard, when no field, menu or window owns it.
    fn keyboard(&mut self, ctx: &Context) {
        if self.editing.is_some() || self.menu.is_some() || self.note.is_some() {
            return;
        }
        let command = modifiers_of(ctx, |m| m.command);
        let (undo, redo) = ctx.input(|i| {
            (
                command && i.key_pressed(Key::Z) && !i.modifiers.shift,
                command && ((i.key_pressed(Key::Z) && i.modifiers.shift) || i.key_pressed(Key::Y)),
            )
        });
        if undo {
            self.undo();
            return;
        }
        if redo {
            self.redo();
            return;
        }
        // Ctrl/Cmd+<key> belongs to the window's menus (save, open, new): nothing here.
        if command {
            return;
        }
        let Some(selected) = self.selected.clone() else { return };
        if modifiers_of(ctx, |m| m.alt) {
            for key in [Key::ArrowUp, Key::ArrowDown, Key::ArrowRight, Key::ArrowLeft] {
                if ctx.input(|i| i.key_pressed(key)) {
                    self.structural(key);
                    self.reveal_selected();
                    return;
                }
            }
            return;
        }
        if ctx.input(|i| i.key_pressed(Key::F2)) {
            self.begin_edit(&selected);
        } else if ctx.input(|i| i.key_pressed(Key::Tab)) {
            self.add_child(&selected);
        } else if ctx.input(|i| i.key_pressed(Key::Enter)) {
            self.add_sibling(&selected);
        } else if ctx.input(|i| i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace)) {
            self.delete(&selected);
        } else if ctx.input(|i| i.key_pressed(Key::Space)) {
            self.toggle_collapse(&selected);
        } else {
            for key in [Key::ArrowLeft, Key::ArrowRight, Key::ArrowUp, Key::ArrowDown] {
                if ctx.input(|i| i.key_pressed(key)) {
                    self.navigate(key);
                    return;
                }
            }
        }
    }

    /// What the card menu needs to know about its node, read before the closure borrows self.
    fn menu_facts(&self, node: &str) -> Option<MenuFacts> {
        let target = self.doc.map.node(node)?;
        let is_root = target.parent_id.is_none();
        let is_root_child = target.parent_id.as_deref().is_some() && target.parent_id.as_deref() == self.doc.map.root_id();
        Some(MenuFacts {
            text: target.text.clone(),
            is_root,
            is_root_child,
            children: self.doc.map.children(node).len(),
            collapsed: target.collapsed == Some(true),
            width: target.width,
            note: target.note.clone().unwrap_or_default(),
        })
    }

    /// The card menu: structure, style, note and delete.
    fn card_menu(&mut self, ctx: &Context) {
        let Some(menu) = self.menu.clone() else { return };
        let Some(facts) = self.menu_facts(&menu.node) else {
            self.menu = None;
            return;
        };
        let mut action: Option<MenuAction> = None;
        let area = egui::Area::new(egui::Id::unique("localmind-card-menu")).fixed_pos(menu.at).order(egui::Order::Foreground).show(ctx, |ui| {
            egui::Frame::menu(ui.style()).show(ui, |ui| {
                ui.set_min_width(220.0);
                ui.label(egui::RichText::new(truncate(&facts.text, 40)).weak());
                ui.separator();
                if ui.button(tl!("Edit text")).clicked() {
                    action = Some(MenuAction::Edit);
                }
                if ui.button(tl!("New child")).clicked() {
                    action = Some(MenuAction::Child);
                }
                if !facts.is_root && ui.button(tl!("New sibling")).clicked() {
                    action = Some(MenuAction::Sibling);
                }
                if facts.children > 0 {
                    let label = if facts.collapsed { tl!("Expand branch") } else { tl!("Collapse branch") };
                    if ui.button(label).clicked() {
                        action = Some(MenuAction::Toggle);
                    }
                }
                if ui.button(tl!("Note…")).on_hover_text(truncate(&facts.note, 60)).clicked() {
                    action = Some(MenuAction::Note);
                }
                if facts.is_root_child && !facts.is_root {
                    ui.menu_button(tl!("Side"), |ui| {
                        for (label, side) in [
                            (tl!("Left"), Some(Side::Left)),
                            (tl!("Right"), Some(Side::Right)),
                            (tl!("Up"), Some(Side::Up)),
                            (tl!("Down"), Some(Side::Down)),
                            (tl!("Automatic"), None),
                        ] {
                            if ui.button(label).clicked() {
                                action = Some(MenuAction::Side(side));
                            }
                        }
                    });
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(tl!("Text size"));
                    if ui.small_button("A-").clicked() {
                        action = Some(MenuAction::Font(-FONT_STEP));
                    }
                    if ui.small_button("A+").clicked() {
                        action = Some(MenuAction::Font(FONT_STEP));
                    }
                    if ui.add_enabled(facts.width.is_some(), egui::Button::new(tl!("Reset width")).small()).clicked() {
                        action = Some(MenuAction::ResetWidth);
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label(tl!("Colour"));
                    for colour in COLORS {
                        let (rect, response) = ui.allocate_exact_size(vec2(18.0, 18.0), egui::Sense::click());
                        let parsed = parse_colour(colour);
                        ui.painter().rect_filled(rect, egui::CornerRadius::same(4), parsed);
                        if response.clicked() {
                            action = Some(MenuAction::Colour(Some(colour.to_string())));
                        }
                    }
                    if ui.small_button(tl!("None")).on_hover_text(tl!("No colour")).clicked() {
                        action = Some(MenuAction::Colour(None));
                    }
                });
                if !facts.is_root {
                    ui.separator();
                    if ui.button(tl!("Delete node")).clicked() {
                        action = Some(MenuAction::Delete);
                    }
                }
            });
        });

        // A click anywhere else, or Escape, closes the menu (a menu that stays open hides the map).
        let rect = area.response.rect;
        let outside = ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !rect.contains(p)));
        if outside || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.menu = None;
        }

        let Some(action) = action else { return };
        self.menu = None;
        let node = menu.node;
        match action {
            MenuAction::Edit => self.begin_edit(&node),
            MenuAction::Child => self.add_child(&node),
            MenuAction::Sibling => self.add_sibling(&node),
            MenuAction::Toggle => self.toggle_collapse(&node),
            MenuAction::Note => self.open_note(&node),
            MenuAction::Side(side) => self.set_side(&node, side),
            MenuAction::Font(delta) => self.bump_font(&node, delta),
            MenuAction::ResetWidth => self.reset_width(&node),
            MenuAction::Colour(colour) => self.set_color(&node, colour),
            MenuAction::Delete => self.delete(&node),
        }
    }

    /// The note window: the one improvement over the original app's read-only note.
    fn note_window(&mut self, ctx: &Context) {
        let Some(mut note) = self.note.take() else { return };
        let Some(target) = self.doc.map.node(&note.node) else { return };
        let subject = truncate(&target.text, 60);
        let mut open = true;
        let mut save = false;
        let mut remove = false;
        let mut cancel = false;
        egui::Window::new(tl!("Note"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(420.0)
            .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(egui::RichText::new(subject).weak());
                ui.add_space(4.0);
                let field = ui.add(egui::TextEdit::multiline(&mut note.draft).desired_rows(8).desired_width(f32::INFINITY));
                field.request_focus();
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(!note.draft.is_empty(), egui::Button::new(tl!("Remove note"))).clicked() {
                        remove = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(tl!("Save")).clicked() {
                            save = true;
                        }
                        if ui.button(tl!("Cancel")).clicked() {
                            cancel = true;
                        }
                    });
                });
            });

        if cancel || (!open && !save && !remove) {
            return;
        }
        if remove {
            self.set_note(&note.node, None);
            return;
        }
        if save {
            self.set_note(&note.node, Some(note.draft.clone()));
            return;
        }
        if !open {
            return;
        }
        self.note = Some(note);
    }
}

/// What the card menu shows about its node.
struct MenuFacts {
    text: String,
    is_root: bool,
    is_root_child: bool,
    children: usize,
    collapsed: bool,
    width: Option<f32>,
    note: String,
}

enum MenuAction {
    Edit,
    Child,
    Sibling,
    Toggle,
    Note,
    Side(Option<Side>),
    Font(f32),
    ResetWidth,
    Colour(Option<String>),
    Delete,
}

impl Editing {
    /// Reads this frame's IME events: a non-empty pre-edit means the candidate list is open, and
    /// Enter belongs to it.
    fn track_ime(&mut self, ctx: &Context) {
        for event in ctx.input(|i| i.events.clone()) {
            if let egui::Event::Ime(ime) = event {
                match ime {
                    egui::ImeEvent::Preedit { text, .. } => self.composing = !text.is_empty(),
                    egui::ImeEvent::Commit(_) => self.composing = false,
                    _ => {}
                }
            }
        }
    }
}

/// The keys the frame's commit path needs, read before any widget can consume them.
#[derive(Clone, Copy, Debug, Default)]
struct KeysThisFrame {
    enter: bool,
    shift: bool,
    escape: bool,
}

impl KeysThisFrame {
    fn read(ctx: &Context) -> Self {
        // One `input` pass: a nested one would take the same read lock and deadlock.
        let (enter, escape, shift_state, shift_events) = ctx.input(|i| {
            (
                i.key_pressed(Key::Enter),
                i.key_pressed(Key::Escape),
                i.modifiers.shift,
                i.events.iter().any(|event| matches!(event, egui::Event::Key { pressed: true, modifiers, .. } if modifiers.shift)),
            )
        });
        Self { enter, shift: shift_state || shift_events, escape }
    }
}

/// Whether this frame's keys carry `pick` (Ctrl/Cmd, Alt, Shift).
///
/// The input state's `modifiers` follow the platform's raw input, which a synthetic key event (the
/// headless tests, a remote desktop injecting keys) does not necessarily update, so the events are
/// consulted as well: the modifier a key was pressed with is on the key event itself.
fn modifiers_of(ctx: &Context, pick: impl Fn(&egui::Modifiers) -> bool) -> bool {
    ctx.input(|i| {
        pick(&i.modifiers) || i.events.iter().any(|event| matches!(event, egui::Event::Key { pressed: true, modifiers, .. } if pick(modifiers)))
    })
}

/// A `#rrggbb` as a drawing colour (the menu's swatches are ours, so this never fails).
fn parse_colour(hex: &str) -> egui::Color32 {
    let hex = hex.trim_start_matches('#');
    let channel = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).unwrap_or(0);
    egui::Color32::from_rgb(channel(0), channel(2), channel(4))
}

/// A short label for a menu line: the text with an ellipsis when it is long.
fn truncate(text: &str, max: usize) -> String {
    let mut out: String = text.chars().take(max).collect();
    if text.chars().count() > max {
        out.push('…');
    }
    out
}

/// The document a new window starts with, in the current language.
pub fn new_document() -> Document {
    Document::untitled(tl!("An untitled map"), tl!("Central idea"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor() -> Editor {
        Editor::new(new_document())
    }

    fn root(editor: &Editor) -> String {
        editor.doc.root_id().expect("root")
    }

    #[test]
    fn tab_adds_a_child_in_edit_mode_and_enter_commits_it() {
        let mut editor = editor();
        let root = root(&editor);
        editor.add_child(&root);
        let child = editor.selected.clone().expect("the new node is selected");
        assert_eq!(editor.doc.map.node(&child).map(|n| n.parent_id.clone()), Some(Some(root.clone())));
        assert!(editor.doc.dirty, "adding a node marks the map");
        assert!(editor.editing.is_some(), "the new node opens for typing");

        editor.editing.as_mut().expect("editing").draft = "  Ramo novo  ".into();
        editor.commit_edit();
        assert_eq!(editor.doc.map.node(&child).map(|n| n.text.as_str()), Some("Ramo novo"));
        assert!(editor.editing.is_none());
    }

    #[test]
    fn an_empty_draft_is_not_a_rename() {
        let mut editor = editor();
        let root = root(&editor);
        let before = editor.doc.map.clone();
        editor.begin_edit(&root);
        editor.editing.as_mut().expect("editing").draft = "   ".into();
        editor.commit_edit();
        assert_eq!(editor.doc.map, before);
        assert!(!editor.doc.dirty);
    }

    #[test]
    fn enter_adds_a_sibling_and_a_root_child_keeps_its_side() {
        let mut editor = editor();
        let root = root(&editor);
        let first = editor.doc.map.add_child(&root, "primeiro", None);
        editor.doc.map.set_side(&first, Some(Side::Left));
        editor.add_sibling(&first);
        let created = editor.selected.clone().expect("selected");
        let created_node = editor.doc.map.node(&created).expect("node");
        assert_eq!(created_node.parent_id.as_deref(), Some(root.as_str()));
        assert_eq!(created_node.side, Some(Side::Left), "the sibling stays on the same side");
        assert_eq!(editor.doc.map.node(&first).and_then(|n| n.side), Some(Side::Left), "and so does the original");
        // The root has no siblings: nothing is created and the selection does not move.
        let before = editor.doc.map.clone();
        editor.add_sibling(&root);
        assert_eq!(editor.doc.map, before);
        assert_eq!(editor.selected.as_deref(), Some(created.as_str()));
    }

    #[test]
    fn delete_removes_the_subtree_and_selects_the_parent() {
        let mut editor = editor();
        let root = root(&editor);
        let child = editor.doc.map.add_child(&root, "filho", None);
        let grandchild = editor.doc.map.add_child(&child, "neto", None);
        editor.delete(&child);
        assert!(editor.doc.map.node(&child).is_none());
        assert!(editor.doc.map.node(&grandchild).is_none());
        assert_eq!(editor.selected.as_deref(), Some(root.as_str()));
        // The root is not deletable.
        editor.delete(&root);
        assert!(editor.doc.map.node(&root).is_some());
    }

    #[test]
    fn undo_and_redo_walk_the_edits() {
        let mut editor = editor();
        let root = root(&editor);
        let original = node_text(&editor, &root).expect("root text");
        editor.begin_edit(&root);
        editor.editing.as_mut().expect("editing").draft = "Renomeado".into();
        editor.commit_edit();
        assert_eq!(node_text(&editor, &root).as_deref(), Some("Renomeado"));
        assert!(editor.can_undo());

        editor.undo();
        assert_eq!(node_text(&editor, &root).as_deref(), Some(original.as_str()));
        assert!(editor.can_redo());

        editor.redo();
        assert_eq!(node_text(&editor, &root).as_deref(), Some("Renomeado"));
    }

    fn node_text(editor: &Editor, id: &str) -> Option<String> {
        editor.doc.map.node(id).map(|n| n.text.clone())
    }

    #[test]
    fn undo_after_a_delete_restores_the_subtree_and_fixes_the_selection() {
        let mut editor = editor();
        let root = root(&editor);
        let child = editor.doc.map.add_child(&root, "filho", None);
        editor.edit(|map| {
            map.add_child(&child, "neto", None);
            true
        });
        editor.delete(&child);
        editor.undo();
        assert!(editor.doc.map.node(&child).is_some());
        // The selection pointed at the parent, which still exists: nothing to repair.
        assert_eq!(editor.selected.as_deref(), Some(root.as_str()));
    }

    #[test]
    fn collapsing_is_an_edit_and_undoes() {
        let mut editor = editor();
        let root = root(&editor);
        let child = editor.doc.map.add_child(&root, "filho", None);
        editor.toggle_collapse(&child);
        assert_eq!(editor.doc.map.node(&child).and_then(|n| n.collapsed), Some(true));
        editor.undo();
        assert_eq!(editor.doc.map.node(&child).and_then(|n| n.collapsed), None);
    }

    #[test]
    fn the_arrows_walk_parent_child_and_siblings() {
        let mut editor = editor();
        let root = root(&editor);
        let first = editor.doc.map.add_child(&root, "um", None);
        let second = editor.doc.map.add_child(&root, "dois", None);
        editor.selected = Some(first.clone());

        editor.navigate(Key::ArrowRight);
        assert_eq!(editor.selected.as_deref(), Some(first.as_str()), "a leaf has no child to go to");
        editor.navigate(Key::ArrowDown);
        assert_eq!(editor.selected.as_deref(), Some(second.as_str()));
        editor.navigate(Key::ArrowUp);
        assert_eq!(editor.selected.as_deref(), Some(first.as_str()));
        editor.navigate(Key::ArrowRight);
        assert_eq!(editor.selected.as_deref(), Some(first.as_str()));
        editor.navigate(Key::ArrowLeft);
        assert_eq!(editor.selected.as_deref(), Some(root.as_str()));

        // Right on a collapsed node expands it instead of walking into a hidden child.
        editor.toggle_collapse(&first);
        editor.selected = Some(first.clone());
        editor.navigate(Key::ArrowRight);
        assert_eq!(editor.doc.map.node(&first).and_then(|n| n.collapsed), None);
        assert_eq!(editor.selected.as_deref(), Some(first.as_str()));
    }

    #[test]
    fn alt_arrows_move_the_node_around_its_parent() {
        let mut editor = editor();
        let root = root(&editor);
        let a = editor.doc.map.add_child(&root, "a", None);
        let b = editor.doc.map.add_child(&root, "b", None);
        editor.selected = Some(b.clone());

        editor.structural(Key::ArrowUp);
        let order: Vec<&str> = editor.doc.map.children(&root).iter().map(|n| n.id.as_str()).collect();
        assert_eq!(order, [b.as_str(), a.as_str()]);

        // Moving down puts it back below its sibling (an indent needs a sibling above it).
        editor.structural(Key::ArrowDown);
        editor.structural(Key::ArrowRight);
        assert_eq!(editor.doc.map.node(&b).and_then(|n| n.parent_id.as_deref()), Some(a.as_str()));

        editor.structural(Key::ArrowLeft);
        assert_eq!(editor.doc.map.node(&b).and_then(|n| n.parent_id.as_deref()), Some(root.as_str()));
    }

    #[test]
    fn a_width_drag_is_one_undo_step() {
        let mut editor = editor();
        let root = root(&editor);
        let depth = editor.history.steps();
        editor.resize_width(&root, 300.0);
        editor.resize_width(&root, 320.0);
        editor.resize_width(&root, 340.0);
        assert_eq!(editor.history.steps(), depth + 1, "one drag, one step");
        assert_eq!(editor.doc.map.node(&root).and_then(|n| n.width), Some(340.0));

        // A second drag (the first one finished) is a new step.
        editor.resizing = false;
        editor.resize_width(&root, 400.0);
        assert_eq!(editor.history.steps(), depth + 2);
    }

    #[test]
    fn the_note_is_edited_and_can_be_removed() {
        let mut editor = editor();
        let root = root(&editor);
        editor.set_note(&root, Some("contexto".into()));
        assert_eq!(editor.doc.map.node(&root).and_then(|n| n.note.as_deref()), Some("contexto"));
        assert!(editor.doc.map.node(&root).is_some_and(|n| n.has_note()));
        editor.set_note(&root, None);
        assert!(!editor.doc.map.node(&root).is_some_and(|n| n.has_note()));
        editor.undo();
        assert!(editor.doc.map.node(&root).is_some_and(|n| n.has_note()));
    }

    #[test]
    fn colour_font_and_width_are_edits() {
        let mut editor = editor();
        let root = root(&editor);
        editor.set_color(&root, Some("#7c5cff".into()));
        editor.bump_font(&root, FONT_STEP);
        editor.bump_font(&root, FONT_STEP);
        let node = editor.doc.map.node(&root).expect("root");
        assert_eq!(node.color.as_deref(), Some("#7c5cff"));
        assert_eq!(node.font_size, Some(localmind_layout::DEFAULT_FONT + 2.0 * FONT_STEP));
        editor.reset_width(&root);
        assert_eq!(editor.doc.map.node(&root).and_then(|n| n.width), None);
    }

    #[test]
    fn a_new_document_forgets_the_history_and_the_selection() {
        let mut editor = editor();
        let root = root(&editor);
        editor.begin_edit(&root);
        editor.editing.as_mut().expect("editing").draft = "x".into();
        editor.commit_edit();
        editor.replace(new_document());
        assert!(!editor.can_undo());
        assert!(editor.editing.is_none());
        assert_eq!(editor.selected, editor.doc.root_id());
    }
}
