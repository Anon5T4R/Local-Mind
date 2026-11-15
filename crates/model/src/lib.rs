//! The mind map itself: the nodes, the tree their `parent_id`s form, the edits every command goes
//! through, and the undo history.
//!
//! The shape of the file is the shape of these types. LocalMind (the Electron app this one ports)
//! writes a `.tmind` as pretty JSON with `id`, `title`, `nodes`, `createdAt` and `updatedAt`, and
//! each node with `id`, `text`, `parentId` and, when it has them, `collapsed`, `color`, `note`,
//! `width`, `fontSize` and `side`. Two rules come from "the map is the user's":
//!
//! - every optional field is optional on read, so a hand-edited or older file opens;
//! - a field this build does not know is carried in [`MindNode::extra`]/[`MindMap::extra`] and
//!   written back, so opening and saving a map never drops what another version wrote.
//!
//! The tree is a flat list: a node is a child of the node its `parent_id` names, siblings are in
//! list order, and the root is the FIRST node with no parent (the layout and the exports agree on
//! that rule, which is not free: it was the source of a bug in the original app).

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable))]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Which direction from the root a subtree grows.
///
/// Only meaningful on the root's direct children (descendants inherit it); `None` means
/// "auto-balance left/right".
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Left,
    Right,
    Up,
    Down,
}

impl Side {
    /// The four directions, in the order the original app groups them.
    pub const ALL: [Side; 4] = [Side::Left, Side::Right, Side::Up, Side::Down];
}

/// One node of the map.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MindNode {
    pub id: String,
    pub text: String,
    /// `None` for the root.
    pub parent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collapsed: Option<bool>,
    /// Accent colour of the card, `#rrggbb` as the user picked it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Free-form note shown as a dot on the card and written by the exports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Card width in px; `None` or a non-positive value means the layout default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
    /// Card font size in px; `None` or a non-positive value means the layout default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<Side>,
    /// Fields written by another version of the app, kept verbatim.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl MindNode {
    /// A node with nothing but what the format requires.
    pub fn new(id: impl Into<String>, text: impl Into<String>, parent_id: Option<String>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            parent_id,
            collapsed: None,
            color: None,
            note: None,
            width: None,
            font_size: None,
            side: None,
            extra: BTreeMap::new(),
        }
    }

    /// Whether the node has a note worth showing.
    pub fn has_note(&self) -> bool {
        self.note.as_deref().is_some_and(|n| !n.trim().is_empty())
    }
}

/// One map, as the `.tmind` file holds it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MindMap {
    pub id: String,
    pub title: String,
    pub nodes: Vec<MindNode>,
    /// Milliseconds since the Unix epoch, as the original app wrote them.
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl MindMap {
    /// A new map with a single central node.
    pub fn new(title: impl Into<String>, root_text: impl Into<String>) -> Self {
        let now = now_ms();
        let root = MindNode::new(new_id(), root_text, None);
        Self { id: new_id(), title: title.into(), nodes: vec![root], created_at: now, updated_at: now, extra: BTreeMap::new() }
    }

    /// The root: the FIRST node with no parent.
    ///
    /// A file can hold more than one (a bad import, a merge of two maps); the layout and the
    /// exports both take the first, and this is the only place that decides it.
    pub fn root(&self) -> Option<&MindNode> {
        self.nodes.iter().find(|n| n.parent_id.is_none())
    }

    /// The root's id, when the map has one.
    pub fn root_id(&self) -> Option<&str> {
        self.root().map(|n| n.id.as_str())
    }

    pub fn node(&self, id: &str) -> Option<&MindNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    fn node_mut(&mut self, id: &str) -> Option<&mut MindNode> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    /// The children of `parent`, in sibling order (the order they sit in the file).
    pub fn children(&self, parent: &str) -> Vec<&MindNode> {
        self.nodes.iter().filter(|n| n.parent_id.as_deref() == Some(parent)).collect()
    }

    /// Every descendant of `id`, never `id` itself.
    pub fn descendant_ids(&self, id: &str) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let mut stack: Vec<&str> = self.children(id).iter().map(|n| n.id.as_str()).collect();
        while let Some(current) = stack.pop() {
            if !out.insert(current.to_string()) {
                continue;
            }
            stack.extend(self.children(current).iter().map(|n| n.id.as_str()));
        }
        out
    }

    /// The nodes hidden because an ancestor (or themselves) is collapsed: the descendants of every
    /// collapsed node, which never includes the collapsed node itself.
    pub fn hidden_ids(&self) -> BTreeSet<String> {
        let mut hidden = BTreeSet::new();
        for node in self.nodes.iter().filter(|n| n.collapsed == Some(true)) {
            hidden.extend(self.descendant_ids(&node.id));
        }
        hidden
    }

    /// The chain of texts from the root down to `id` (inclusive), which the AI prompts use as
    /// context. Empty when the node does not exist; a cycle in a hand-edited file stops the walk
    /// at the first node seen twice.
    pub fn path_texts(&self, id: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        let mut cursor = self.node(id);
        while let Some(node) = cursor {
            if !seen.insert(node.id.as_str()) {
                break;
            }
            out.push(node.text.clone());
            cursor = node.parent_id.as_deref().and_then(|parent| self.node(parent));
        }
        out.reverse();
        out
    }

    fn touch(&mut self) {
        self.updated_at = now_ms();
    }

    /// Runs `edit` on one node and stamps `updated_at` when it changed something.
    fn edit_node(&mut self, id: &str, edit: impl FnOnce(&mut MindNode) -> bool) -> bool {
        let changed = match self.node_mut(id) {
            Some(node) => edit(node),
            None => false,
        };
        if changed {
            self.touch();
        }
        changed
    }

    /// Adds a child to `parent` and returns its id. A collapsed `parent` is expanded: the new node
    /// would otherwise be created out of sight.
    pub fn add_child(&mut self, parent: &str, text: impl Into<String>, side: Option<Side>) -> String {
        let id = new_id();
        if let Some(node) = self.node_mut(parent) {
            node.collapsed = None;
        }
        let mut child = MindNode::new(id.clone(), text, Some(parent.to_string()));
        child.side = side;
        self.nodes.push(child);
        self.touch();
        id
    }

    /// Adds a sibling right after `sibling`, or `None` when `sibling` is the root (a root has no
    /// siblings) or does not exist.
    ///
    /// `keep_side` is the side the caller resolved for `sibling` (layout work, so it is passed in
    /// rather than computed here): the original app freezes the sibling's side when it is a root
    /// child, because auto-balancing is unstable while children are being added.
    pub fn add_sibling(&mut self, sibling: &str, text: impl Into<String>, keep_side: Option<Side>) -> Option<String> {
        let node = self.node(sibling)?.clone();
        let parent = node.parent_id.clone()?;
        let id = new_id();
        if let Some(sibling_node) = self.node_mut(sibling)
            && keep_side.is_some()
        {
            sibling_node.side = keep_side;
        }
        let mut created = MindNode::new(id.clone(), text, Some(parent));
        created.side = keep_side;
        let at = self.nodes.iter().position(|n| n.id == sibling).map_or(self.nodes.len(), |i| i + 1);
        self.nodes.insert(at, created);
        self.touch();
        Some(id)
    }

    /// Appends nodes built elsewhere (an AI answer, a paste), expanding `uncollapse` when given.
    pub fn add_nodes(&mut self, nodes: Vec<MindNode>, uncollapse: Option<&str>) {
        if nodes.is_empty() {
            return;
        }
        if let Some(parent) = uncollapse.and_then(|id| self.node_mut(id)) {
            parent.collapsed = None;
        }
        self.nodes.extend(nodes);
        self.touch();
    }

    /// Removes `id` and its whole subtree, returning the selection the caller should move to (the
    /// parent). The root cannot be deleted.
    pub fn delete_node(&mut self, id: &str) -> Option<String> {
        let node = self.node(id)?;
        let parent = node.parent_id.clone()?;
        let doomed = self.descendant_ids(id);
        self.nodes.retain(|n| n.id != id && !doomed.contains(&n.id));
        self.touch();
        Some(parent)
    }

    /// Moves a node one place up among its siblings.
    pub fn move_up(&mut self, id: &str) -> bool {
        self.swap_with_sibling(id, -1)
    }

    /// Moves a node one place down among its siblings.
    pub fn move_down(&mut self, id: &str) -> bool {
        self.swap_with_sibling(id, 1)
    }

    fn swap_with_sibling(&mut self, id: &str, delta: isize) -> bool {
        let Some(node) = self.node(id) else { return false };
        let parent = node.parent_id.clone();
        let Some(index) = self.nodes.iter().position(|n| n.id == id) else { return false };
        // Siblings are the entries that share the parent; the nearest one in the wanted direction
        // is what the node swaps places with in the list (which is what defines sibling order).
        let mut target = None;
        let mut cursor = index;
        loop {
            if delta < 0 {
                if cursor == 0 {
                    break;
                }
                cursor -= 1;
            } else {
                cursor += 1;
                if cursor >= self.nodes.len() {
                    break;
                }
            }
            if self.nodes[cursor].parent_id == parent {
                target = Some(cursor);
                break;
            }
        }
        let Some(other) = target else { return false };
        self.nodes.swap(index, other);
        self.touch();
        true
    }

    /// Makes `id` a child of its previous sibling (and expands that sibling).
    pub fn indent(&mut self, id: &str) -> bool {
        let Some(node) = self.node(id) else { return false };
        let parent = node.parent_id.clone();
        let siblings: Vec<String> = self.nodes.iter().filter(|n| n.parent_id == parent).map(|n| n.id.clone()).collect();
        let Some(position) = siblings.iter().position(|sibling| sibling == id) else { return false };
        if position == 0 {
            return false;
        }
        let new_parent = siblings[position - 1].clone();
        if let Some(node) = self.node_mut(id) {
            node.parent_id = Some(new_parent.clone());
        }
        if let Some(parent_node) = self.node_mut(&new_parent) {
            parent_node.collapsed = None;
        }
        self.touch();
        true
    }

    /// Makes `id` a sibling of its parent. A child of the root cannot be outdented further.
    pub fn outdent(&mut self, id: &str) -> bool {
        let Some(node) = self.node(id) else { return false };
        let Some(parent_id) = node.parent_id.clone() else { return false };
        let Some(grandparent) = self.node(&parent_id).and_then(|p| p.parent_id.clone()) else { return false };
        if let Some(node) = self.node_mut(id) {
            node.parent_id = Some(grandparent);
        }
        self.touch();
        true
    }

    pub fn toggle_collapse(&mut self, id: &str) -> bool {
        self.edit_node(id, |node| {
            node.collapsed = if node.collapsed == Some(true) { None } else { Some(true) };
            true
        })
    }

    /// Replaces a node's text. An empty text is refused: a card with no text cannot be clicked
    /// back into shape.
    pub fn set_text(&mut self, id: &str, text: &str) -> bool {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return false;
        }
        self.edit_node(id, |node| {
            node.text = trimmed.to_string();
            true
        })
    }

    pub fn set_note(&mut self, id: &str, note: Option<String>) {
        self.edit_node(id, |node| {
            node.note = note.clone().filter(|n| !n.trim().is_empty());
            true
        });
    }

    pub fn set_color(&mut self, id: &str, color: Option<String>) {
        self.edit_node(id, |node| {
            node.color = color.clone();
            true
        });
    }

    /// Sets the card width in px, clamped to the range the resize handle allows. `None` resets it.
    pub fn set_width(&mut self, id: &str, width: Option<f32>) {
        self.edit_node(id, |node| {
            node.width = width.map(|w| w.clamp(MIN_WIDTH, MAX_WIDTH));
            true
        });
    }

    /// Sets the card font size in px, clamped to the range the menu allows. `None` resets it.
    pub fn set_font_size(&mut self, id: &str, size: Option<f32>) {
        self.edit_node(id, |node| {
            node.font_size = size.map(|s| s.clamp(MIN_FONT, MAX_FONT));
            true
        });
    }

    pub fn set_side(&mut self, id: &str, side: Option<Side>) {
        self.edit_node(id, |node| {
            node.side = side;
            true
        });
    }
}

/// Card width limits, as the original app's resize handle enforces them.
pub const MIN_WIDTH: f32 = 120.0;
pub const MAX_WIDTH: f32 = 640.0;
/// Card font size limits, as the original app's menu enforces them.
pub const MIN_FONT: f32 = 9.0;
pub const MAX_FONT: f32 = 34.0;

/// The undo/redo history: whole-map snapshots, like the original app (it kept 100).
#[derive(Clone, Debug)]
pub struct History {
    past: Vec<MindMap>,
    future: Vec<MindMap>,
    limit: usize,
}

impl Default for History {
    fn default() -> Self {
        Self::new(100)
    }
}

impl History {
    pub fn new(limit: usize) -> Self {
        Self { past: Vec::new(), future: Vec::new(), limit: limit.max(1) }
    }

    /// Records the state an edit started from. Any redo branch is dropped, as editing after an
    /// undo always is.
    pub fn record(&mut self, before: MindMap) {
        self.past.push(before);
        if self.past.len() > self.limit {
            self.past.remove(0);
        }
        self.future.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    /// The previous state, when there is one; `current` goes to the redo side.
    pub fn undo(&mut self, current: MindMap) -> Option<MindMap> {
        let previous = self.past.pop()?;
        self.future.insert(0, current);
        if self.future.len() > self.limit {
            self.future.truncate(self.limit);
        }
        Some(previous)
    }

    /// The state an undo went back from; `current` goes to the undo side.
    pub fn redo(&mut self, current: MindMap) -> Option<MindMap> {
        let next = self.future.first().cloned()?;
        self.future.remove(0);
        self.past.push(current);
        if self.past.len() > self.limit {
            self.past.remove(0);
        }
        Some(next)
    }

    /// Forgets both directions (a new map, or one just opened from a file).
    pub fn clear(&mut self) {
        self.past.clear();
        self.future.clear();
    }

    /// How many undo steps are stored (the tests check that one drag is one step).
    pub fn steps(&self) -> usize {
        self.past.len()
    }
}

/// A fresh node/map id: eight characters of `[0-9a-z]`, the shape the original app's `nanoid(8)`
/// produced. Not a nanoid: the alphabet is the app's own, and nothing outside this crate parses it.
pub fn new_id() -> String {
    const ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0) as u64;
    // xorshift: the seed only has to differ between the ids of one session and between runs.
    let mut state = nanos ^ count.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let mut out = String::with_capacity(8);
    while out.len() < 8 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        out.push(ALPHABET[(state % ALPHABET.len() as u64) as usize] as char);
    }
    out
}

/// Milliseconds since the Unix epoch, the unit the file's `createdAt`/`updatedAt` use.
pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A map whose ids are the texts, so an assertion reads like the picture it describes.
    fn map(nodes: Vec<MindNode>) -> MindMap {
        MindMap { id: "m".into(), title: "t".into(), nodes, created_at: 0, updated_at: 0, extra: BTreeMap::new() }
    }

    fn node(id: &str, parent: Option<&str>) -> MindNode {
        MindNode::new(id, id, parent.map(str::to_string))
    }

    fn ids(nodes: &[&str]) -> Vec<String> {
        nodes.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn a_new_map_has_one_central_node() {
        let map = MindMap::new("Novo mapa", "Ideia central");
        assert_eq!(map.nodes.len(), 1);
        let root = map.root().expect("root");
        assert_eq!(root.text, "Ideia central");
        assert_eq!(root.parent_id, None);
        assert_eq!(root.id.len(), 8);
        assert_eq!(map.created_at, map.updated_at);
    }

    /// Two roots can arrive from an import or a merge. The first one is the root, everywhere.
    #[test]
    fn the_root_is_the_first_node_without_a_parent() {
        let map = map(vec![node("R1", None), node("R2", None), node("C", Some("R1"))]);
        assert_eq!(map.root_id(), Some("R1"));
    }

    #[test]
    fn children_come_in_sibling_order() {
        let map = map(vec![node("r", None), node("a", Some("r")), node("b", Some("r")), node("a1", Some("a"))]);
        let children: Vec<&str> = map.children("r").iter().map(|n| n.id.as_str()).collect();
        assert_eq!(children, ["a", "b"]);
        let grandchildren: Vec<&str> = map.children("a").iter().map(|n| n.id.as_str()).collect();
        assert_eq!(grandchildren, ["a1"]);
    }

    #[test]
    fn adding_a_child_expands_its_parent() {
        let mut map = map(vec![node("r", None)]); // the root starts expanded
        let id = map.add_child("r", "Novo", Some(Side::Up));
        assert_eq!(map.node(&id).and_then(|n| n.side), Some(Side::Up));
        assert_eq!(map.children("r").len(), 1);

        map.toggle_collapse("r");
        assert_eq!(map.node("r").and_then(|n| n.collapsed), Some(true));
        map.add_child("r", "Outro", None);
        assert_eq!(map.node("r").and_then(|n| n.collapsed), None);
    }

    /// The new sibling lands right after the one it was asked from, and a root child freezes both
    /// sides: auto-balancing while children are being added would move them around under the user.
    #[test]
    fn a_sibling_is_inserted_after_its_sibling_and_freezes_the_side() {
        let mut map = map(vec![node("r", None), node("a", Some("r")), node("b", Some("r"))]);
        let id = map.add_sibling("a", "novo", Some(Side::Left)).expect("sibling");
        let order: Vec<&str> = map.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(order, ["r", "a", id.as_str(), "b"]);
        // The sibling kept the side the caller resolved, and so did the new node.
        assert_eq!(map.node("a").and_then(|n| n.side), Some(Side::Left));
        assert_eq!(map.node(&id).and_then(|n| n.side), Some(Side::Left));
    }

    #[test]
    fn the_root_has_no_siblings() {
        let mut map = map(vec![node("r", None)]);
        assert_eq!(map.add_sibling("r", "x", None), None);
        assert_eq!(map.add_sibling("fantasma", "x", None), None);
    }

    /// Deleting takes the whole subtree and puts the selection on the parent; the root is the one
    /// node that cannot be deleted.
    #[test]
    fn deleting_a_node_takes_its_subtree_and_selects_the_parent() {
        let mut map = map(vec![node("r", None), node("a", Some("r")), node("a1", Some("a")), node("b", Some("r"))]);
        assert_eq!(map.delete_node("a"), Some("r".to_string()));
        let left: Vec<&str> = map.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(left, ["r", "b"]);
        assert_eq!(map.delete_node("r"), None);
        assert_eq!(map.nodes.len(), 2);
    }

    /// The sibling order is the order in the file: moving skips entries that are not siblings.
    ///
    /// The swap is between the two siblings' places in the list, so the subtree of the moved node
    /// travels with it (which is what makes the picture follow the command).
    #[test]
    fn moving_a_node_swaps_it_with_the_nearest_sibling() {
        let mut map = map(vec![node("r", None), node("a", Some("r")), node("a1", Some("a")), node("b", Some("r"))]);
        assert!(map.move_down("a"));
        let order: Vec<&str> = map.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(order, ["r", "b", "a1", "a"]);
        assert!(map.move_up("a"));
        let order: Vec<&str> = map.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(order, ["r", "a", "a1", "b"]);
        // The first sibling cannot move up, the last cannot move down.
        assert!(!map.move_up("a"));
        assert!(!map.move_down("b"));
    }

    #[test]
    fn indenting_makes_a_node_a_child_of_the_previous_sibling() {
        let mut map = map(vec![node("r", None), node("a", Some("r")), node("b", Some("r")), node("c", Some("r"))]);
        map.toggle_collapse("b");
        assert!(map.indent("c"));
        assert_eq!(map.node("c").and_then(|n| n.parent_id.as_deref()), Some("b"));
        // The new parent expands, or the node would disappear into a collapsed branch.
        assert_eq!(map.node("b").and_then(|n| n.collapsed), None);
        // The first sibling has nothing to indent under.
        assert!(!map.indent("a"));
    }

    #[test]
    fn outdenting_makes_a_node_a_sibling_of_its_parent() {
        let mut map = map(vec![node("r", None), node("a", Some("r")), node("b", Some("a"))]);
        assert!(map.outdent("b"));
        assert_eq!(map.node("b").and_then(|n| n.parent_id.as_deref()), Some("r"));
        // A child of the root cannot go further up, and the root cannot move at all.
        assert!(!map.outdent("a"));
        assert!(!map.outdent("r"));
    }

    #[test]
    fn the_text_is_trimmed_and_an_empty_text_is_refused() {
        let mut map = map(vec![node("r", None)]);
        assert!(map.set_text("r", "  Novo texto  "));
        assert_eq!(map.node("r").map(|n| n.text.as_str()), Some("Novo texto"));
        assert!(!map.set_text("r", "   "));
        assert_eq!(map.node("r").map(|n| n.text.as_str()), Some("Novo texto"));
        assert!(!map.set_text("fantasma", "x"));
    }

    /// The width and the font size have the ranges the resize handle and the menu enforce; a value
    /// outside them is clamped, not written as is.
    #[test]
    fn the_width_and_the_font_size_are_clamped() {
        let mut map = map(vec![node("r", None)]);
        map.set_width("r", Some(10.0));
        assert_eq!(map.node("r").and_then(|n| n.width), Some(MIN_WIDTH));
        map.set_width("r", Some(5_000.0));
        assert_eq!(map.node("r").and_then(|n| n.width), Some(MAX_WIDTH));
        map.set_width("r", None);
        assert_eq!(map.node("r").and_then(|n| n.width), None);

        map.set_font_size("r", Some(1.0));
        assert_eq!(map.node("r").and_then(|n| n.font_size), Some(MIN_FONT));
        map.set_font_size("r", Some(100.0));
        assert_eq!(map.node("r").and_then(|n| n.font_size), Some(MAX_FONT));
        map.set_font_size("r", None);
        assert_eq!(map.node("r").and_then(|n| n.font_size), None);
    }

    #[test]
    fn colour_note_and_side_are_set_and_cleared() {
        let mut map = map(vec![node("r", None)]);
        map.set_color("r", Some("#7c5cff".into()));
        map.set_side("r", Some(Side::Down));
        map.set_note("r", Some("  uma nota ".into()));
        let node = map.node("r").expect("node");
        assert_eq!(node.color.as_deref(), Some("#7c5cff"));
        assert_eq!(node.side, Some(Side::Down));
        assert_eq!(node.note.as_deref(), Some("  uma nota "));
        assert!(node.has_note());

        map.set_note("r", Some("   ".into()));
        assert_eq!(map.node("r").and_then(|n| n.note.clone()), None);
        map.set_color("r", None);
        map.set_side("r", None);
        let node = map.node("r").expect("node");
        assert_eq!(node.color, None);
        assert_eq!(node.side, None);
        assert!(!node.has_note());
    }

    #[test]
    fn descendants_exclude_the_node_itself_and_go_all_the_way_down() {
        let map = map(vec![node("r", None), node("a", Some("r")), node("b", Some("a")), node("c", Some("b")), node("z", Some("r"))]);
        let found = map.descendant_ids("a");
        assert!(!found.contains("a"));
        assert_eq!(found.into_iter().collect::<Vec<_>>(), ids(&["b", "c"]));
        assert!(map.descendant_ids("z").is_empty());
        assert!(map.descendant_ids("fantasma").is_empty());
    }

    /// The classic bug: including the collapsed node itself hides the button that expands it again.
    #[test]
    fn collapsing_hides_the_descendants_and_never_the_node_itself() {
        let mut map = map(vec![node("r", None), node("pai", Some("r")), node("filho", Some("pai"))]);
        map.toggle_collapse("pai");
        let hidden = map.hidden_ids();
        assert!(!hidden.contains("pai"));
        assert!(hidden.contains("filho"));
    }

    #[test]
    fn nested_collapsed_nodes_cover_the_whole_subtree() {
        let mut map = map(vec![node("r", None), node("a", Some("r")), node("b", Some("a")), node("c", Some("b")), node("livre", Some("r"))]);
        map.toggle_collapse("a");
        map.toggle_collapse("b");
        let hidden: Vec<String> = map.hidden_ids().into_iter().collect();
        assert_eq!(hidden, ids(&["b", "c"]));
    }

    #[test]
    fn nothing_is_hidden_when_nothing_is_collapsed() {
        let map = map(vec![node("r", None), node("a", Some("r"))]);
        assert!(map.hidden_ids().is_empty());
    }

    #[test]
    fn the_path_texts_run_from_the_root_down_to_the_node() {
        let map = map(vec![node("r", None), node("a", Some("r")), node("b", Some("a"))]);
        assert_eq!(map.path_texts("b"), ["r", "a", "b"]);
        assert!(map.path_texts("fantasma").is_empty());
    }

    /// A cycle in a hand-edited file must not make the path walk forever.
    #[test]
    fn a_cycle_in_the_path_is_bounded() {
        let mut a = node("a", Some("b"));
        a.parent_id = Some("b".into());
        let mut b = node("b", Some("a"));
        b.parent_id = Some("a".into());
        let map = map(vec![node("r", None), a, b]);
        assert!(map.path_texts("a").len() <= map.nodes.len());
    }

    #[test]
    fn adding_nodes_appends_them_and_expands_the_target() {
        let mut map = map(vec![node("r", None)]);
        map.toggle_collapse("r");
        map.add_nodes(vec![MindNode::new("x", "X", Some("r".into()))], Some("r"));
        assert_eq!(map.node("r").and_then(|n| n.collapsed), None);
        assert_eq!(map.children("r").len(), 1);
        // Nothing to add is not an edit.
        let before = map.updated_at;
        map.add_nodes(Vec::new(), None);
        assert_eq!(map.updated_at, before);
    }

    #[test]
    fn the_history_undoes_and_redoes_and_drops_the_redo_branch_on_a_new_edit() {
        let mut history = History::new(100);
        let mut map = map(vec![node("r", None)]);
        assert!(!history.can_undo() && !history.can_redo());

        history.record(map.clone());
        map.set_text("r", "um");
        history.record(map.clone());
        map.set_text("r", "dois");

        let previous = history.undo(map.clone()).expect("undo");
        assert_eq!(previous.node("r").map(|n| n.text.as_str()), Some("um"));
        assert!(history.can_redo());
        map = previous;

        let redone = history.redo(map.clone()).expect("redo");
        assert_eq!(redone.node("r").map(|n| n.text.as_str()), Some("dois"));
        map = redone;

        // Editing after an undo throws the redo branch away, as it must.
        map = history.undo(map).expect("undo again");
        history.record(map.clone());
        map.set_text("r", "três");
        assert!(!history.can_redo());
    }

    #[test]
    fn the_history_stops_at_its_limit() {
        let mut history = History::new(2);
        let map = map(vec![node("r", None)]);
        for step in 0..5 {
            let mut edit = map.clone();
            edit.set_text("r", &format!("v{step}"));
            history.record(edit);
        }
        // Only the last two snapshots survived: two undos, then nothing.
        assert!(history.undo(map.clone()).is_some());
        assert!(history.undo(map.clone()).is_some());
        assert!(history.undo(map.clone()).is_none());
    }

    #[test]
    fn every_edit_stamps_the_update_time() {
        let mut map = map(vec![node("r", None)]);
        map.updated_at = 0;
        map.set_text("r", "novo");
        assert!(map.updated_at > 0);
    }

    #[test]
    fn the_ids_are_eight_characters_and_do_not_repeat() {
        let first = new_id();
        assert_eq!(first.len(), 8);
        assert!(first.chars().all(|c| c.is_ascii_alphanumeric()));
        let many: BTreeSet<String> = (0..1_000).map(|_| new_id()).collect();
        assert_eq!(many.len(), 1_000);
    }
}
