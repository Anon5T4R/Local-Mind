//! The mind map layout: where every card sits, and which direction from the root its subtree grows.
//!
//! A port of the original Electron app's `layout.ts` with the same constants and the same
//! arithmetic, because the geometry is visible: the exported PNG, the minimap and the cards on
//! screen all come from here, and a card that moves by a pixel moves in every one of them. The
//! numbers below are therefore literals on purpose, not tuning parameters.
//!
//! The root sits at the origin. Its children fan out left, right, up and down; a child without an
//! explicit side takes the lighter side (and an explicit one counts towards the load, like the
//! pixels it occupies). Each subtree's band on the cross axis is the subtree's extent, not the
//! card's, so a tall block never overlaps its neighbour. Descendants of a collapsed node are not
//! placed at all.
//!
//! Nothing here recurses without a bound: a file that somebody hand-edited can hold a cycle or a
//! very deep chain, and a layout that overflows the stack would take the app down with it.

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable))]

use std::collections::{BTreeMap, HashMap, HashSet};

use localmind_model::{MindNode, Side};

/// The default card width.
pub const NODE_W: f32 = 220.0;
/// The minimum card height.
pub const NODE_H: f32 = 52.0;
/// Depth gap for the horizontal directions.
const H_GAP: f32 = 80.0;
/// Depth gap for the vertical directions.
const V_DEPTH_GAP: f32 = 64.0;
/// Sibling gap on the cross axis for horizontal layouts.
const V_GAP: f32 = 22.0;
/// Sibling gap on the cross axis for vertical layouts.
const H_CROSS_GAP: f32 = 34.0;
/// Horizontal padding inside a card (accent bar and margins).
const NODE_PAD_X: f32 = 30.0;
/// Vertical padding inside a card.
const NODE_PAD_Y: f32 = 22.0;
/// Font size a node without `fontSize` is measured at.
pub const DEFAULT_FONT: f32 = 13.5;
/// Average glyph width as a fraction of the font size (the original app's estimate).
const CHAR_WIDTH: f32 = 0.56;
/// Line height as a multiple of the font size.
pub const LINE_HEIGHT: f32 = 1.35;
/// Padding the exports add around the map.
pub const EXPORT_PAD: f32 = 60.0;

/// Where a node sits relative to the root: the root itself, or one of the four directions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolvedSide {
    Root,
    Left,
    Right,
    Up,
    Down,
}

impl ResolvedSide {
    /// The direction as the file's `side` value, `None` for the root.
    pub fn as_side(self) -> Option<Side> {
        match self {
            ResolvedSide::Root => None,
            ResolvedSide::Left => Some(Side::Left),
            ResolvedSide::Right => Some(Side::Right),
            ResolvedSide::Up => Some(Side::Up),
            ResolvedSide::Down => Some(Side::Down),
        }
    }

    fn is_vertical(self) -> bool {
        matches!(self, ResolvedSide::Up | ResolvedSide::Down)
    }
}

impl From<Side> for ResolvedSide {
    fn from(side: Side) -> Self {
        match side {
            Side::Left => ResolvedSide::Left,
            Side::Right => ResolvedSide::Right,
            Side::Up => ResolvedSide::Up,
            Side::Down => ResolvedSide::Down,
        }
    }
}

/// The top-left corner of a card, in layout space (the root is centred on the origin).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pos {
    pub x: f32,
    pub y: f32,
}

/// A card's size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Size {
    pub w: f32,
    pub h: f32,
}

/// The result of a layout pass.
#[derive(Clone, Debug, Default)]
pub struct Layout {
    /// Position of every node that is drawn (descendants of a collapsed node are absent).
    pub positions: BTreeMap<String, Pos>,
    /// The direction of every reachable node, collapsed or not, so expanding does not have to
    /// recompute it.
    pub sides: BTreeMap<String, ResolvedSide>,
}

impl Layout {
    /// The bounding box of the placed cards, `None` when nothing was placed.
    pub fn bounds(&self, nodes: &[MindNode]) -> Option<(Pos, Size)> {
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        let mut any = false;
        for node in nodes {
            let Some(pos) = self.positions.get(&node.id) else { continue };
            let size = node_size(node);
            any = true;
            min_x = min_x.min(pos.x);
            min_y = min_y.min(pos.y);
            max_x = max_x.max(pos.x + size.w);
            max_y = max_y.max(pos.y + size.h);
        }
        if any { Some((Pos { x: min_x, y: min_y }, Size { w: max_x - min_x, h: max_y - min_y })) } else { None }
    }
}

/// A card's width: `width` when it is positive, the default otherwise (`width: 0` is a file that
/// would draw an invisible card).
pub fn width_of(node: &MindNode) -> f32 {
    match node.width {
        Some(w) if w > 0.0 => w,
        _ => NODE_W,
    }
}

/// A card's font size: `font_size` when it is positive, the default otherwise.
pub fn font_of(node: &MindNode) -> f32 {
    match node.font_size {
        Some(f) if f > 0.0 => f,
        _ => DEFAULT_FONT,
    }
}

/// A card's estimated size. The height comes from the wrapped text, so a block of several lines
/// reserves the room it needs instead of overlapping its neighbours.
pub fn node_size(node: &MindNode) -> Size {
    Size { w: width_of(node), h: height_of(node) }
}

fn height_of(node: &MindNode) -> f32 {
    let width = width_of(node);
    let font = font_of(node);
    // Floor at six characters per line: a card narrower than its own padding would otherwise
    // divide by a non-positive number and poison the whole geometry with an infinite height.
    let chars_per_line = ((width - NODE_PAD_X) / (font * CHAR_WIDTH)).floor().max(6.0);
    let mut lines = 0.0;
    for paragraph in node.text.split('\n') {
        let count = paragraph.chars().count() as f32;
        lines += (count / chars_per_line).ceil().max(1.0);
    }
    let text_height = lines * font * LINE_HEIGHT;
    (text_height + NODE_PAD_Y).round().max(NODE_H)
}

/// The tree, indexed by id, with the children of each node in sibling order.
struct Index<'a> {
    by_id: HashMap<&'a str, &'a MindNode>,
    children: HashMap<&'a str, Vec<&'a str>>,
    root: Option<&'a str>,
}

impl<'a> Index<'a> {
    fn build(nodes: &'a [MindNode]) -> Self {
        let by_id: HashMap<&str, &MindNode> = nodes.iter().map(|n| (n.id.as_str(), n)).collect();
        let root = nodes.iter().find(|n| n.parent_id.is_none()).map(|n| n.id.as_str());
        let mut children: HashMap<&str, Vec<&str>> = HashMap::new();
        for node in nodes {
            if let Some(parent) = node.parent_id.as_deref() {
                children.entry(parent).or_default().push(node.id.as_str());
            }
        }
        Self { by_id, children, root }
    }

    fn kids(&self, id: &str) -> &[&'a str] {
        self.children.get(id).map_or(&[], Vec::as_slice)
    }

    /// The children that exist as nodes (an orphan points at a parent nobody has).
    fn placed_kids(&self, node: &MindNode) -> Vec<&'a str> {
        if node.collapsed == Some(true) {
            return Vec::new();
        }
        self.kids(node.id.as_str()).iter().copied().filter(|id| self.by_id.contains_key(*id)).collect()
    }
}

/// The direction of every node, given the tree: the root, its children (explicit side, else the
/// lighter direction) and everything below them, which inherits.
fn compute_sides(index: &Index<'_>) -> BTreeMap<String, ResolvedSide> {
    let mut sides = BTreeMap::new();
    let Some(root_id) = index.root else { return sides };
    sides.insert(root_id.to_string(), ResolvedSide::Root);

    let mut left_load = 0usize;
    let mut right_load = 0usize;
    for kid in index.kids(root_id) {
        let Some(node) = index.by_id.get(kid) else { continue };
        let resolved = match node.side {
            Some(side) => ResolvedSide::from(side),
            None => {
                if right_load <= left_load {
                    ResolvedSide::Right
                } else {
                    ResolvedSide::Left
                }
            }
        };
        // An explicit side also counts: the pixels it occupies are real, so the next automatic
        // sibling balances against it.
        match resolved {
            ResolvedSide::Left => left_load += 1,
            ResolvedSide::Right => right_load += 1,
            _ => {}
        }
        sides.insert((*kid).to_string(), resolved);
        // Descendants inherit the direction, all the way down.
        let mut stack = vec![*kid];
        let mut seen = HashSet::new();
        while let Some(current) = stack.pop() {
            if !seen.insert(current) {
                continue;
            }
            for child in index.kids(current) {
                sides.insert((*child).to_string(), resolved);
                stack.push(child);
            }
        }
    }
    sides
}

/// The direction of every reachable node (what [`layout_tree`] uses).
pub fn resolve_sides(nodes: &[MindNode]) -> BTreeMap<String, ResolvedSide> {
    compute_sides(&Index::build(nodes))
}

/// Lays the map out: the root at the origin, each subtree fanned out in its direction.
pub fn layout_tree(nodes: &[MindNode]) -> Layout {
    let index = Index::build(nodes);
    let sides = compute_sides(&index);
    let mut positions = BTreeMap::new();
    let Some(root_id) = index.root else { return Layout { positions, sides } };
    let Some(root) = index.by_id.get(root_id) else { return Layout { positions, sides } };

    let root_size = node_size(root);
    positions.insert(root_id.to_string(), Pos { x: -root_size.w / 2.0, y: -root_size.h / 2.0 });

    // Cross-axis extent of a subtree, per orientation: the subtree's band, not the card's.
    let extent_h = extents(&index, false);
    let extent_v = extents(&index, true);

    // Groups of root children, by direction, in sibling order.
    let mut groups: [Vec<&str>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    for kid in index.placed_kids(root) {
        let Some(side) = sides.get(kid) else { continue };
        let slot = match side {
            ResolvedSide::Left => 0,
            ResolvedSide::Right => 1,
            ResolvedSide::Down => 2,
            ResolvedSide::Up => 3,
            ResolvedSide::Root => continue,
        };
        groups[slot].push(kid);
    }

    // Order matters only for determinism of the traversal; the geometry is independent per group.
    for dir in [ResolvedSide::Right, ResolvedSide::Left, ResolvedSide::Down, ResolvedSide::Up] {
        let kids = match dir {
            ResolvedSide::Left => &groups[0],
            ResolvedSide::Right => &groups[1],
            ResolvedSide::Down => &groups[2],
            ResolvedSide::Up => &groups[3],
            ResolvedSide::Root => continue,
        };
        if kids.is_empty() {
            continue;
        }
        let vertical = dir.is_vertical();
        let extents_by_id = if vertical { &extent_v } else { &extent_h };
        let cross_gap = if vertical { H_CROSS_GAP } else { V_GAP };
        let parent_depth_size = if vertical { root_size.h } else { root_size.w };

        // The block of siblings is centred on the root: starting at zero would grow the map
        // downwards only, leaving the root at the top edge of its own map.
        let total: f32 = kids.iter().map(|k| extents_by_id.get(*k).copied().unwrap_or(0.0)).sum::<f32>() + cross_gap * (kids.len() as f32 - 1.0);
        let mut top = -total / 2.0;

        // Depth-first, with an explicit stack: a hand-edited file can be arbitrarily deep.
        let mut stack: Vec<(&str, ResolvedSide, f32, f32, f32)> = Vec::new();
        let mut entries = Vec::with_capacity(kids.len());
        for kid in kids.iter() {
            let extent = extents_by_id.get(*kid).copied().unwrap_or(0.0);
            entries.push((*kid, dir, 0.0, parent_depth_size, top));
            top += extent + cross_gap;
        }
        // Reversed: the stack pops the first sibling first.
        stack.extend(entries.into_iter().rev());
        while let Some((id, dir, parent_center_depth, parent_depth_size, cross_top)) = stack.pop() {
            let Some(node) = index.by_id.get(id) else { continue };
            let vertical = dir.is_vertical();
            let extents_by_id = if vertical { &extent_v } else { &extent_h };
            let size = node_size(node);
            let my_depth_size = if vertical { size.h } else { size.w };
            let extent = extents_by_id.get(id).copied().unwrap_or(if vertical { size.w } else { size.h });
            let cross_gap = if vertical { H_CROSS_GAP } else { V_GAP };
            let depth_gap = if vertical { V_DEPTH_GAP } else { H_GAP };
            let sign = if matches!(dir, ResolvedSide::Right | ResolvedSide::Down) { 1.0 } else { -1.0 };

            let cross_center = cross_top + extent / 2.0;
            let center_depth = parent_center_depth + sign * ((parent_depth_size + my_depth_size) / 2.0 + depth_gap);
            let (center_x, center_y) = if vertical { (cross_center, center_depth) } else { (center_depth, cross_center) };
            positions.insert(id.to_string(), Pos { x: center_x - size.w / 2.0, y: center_y - size.h / 2.0 });

            let kids = index.placed_kids(node);
            let mut child_top = cross_top;
            let mut pushed = Vec::with_capacity(kids.len());
            for kid in &kids {
                let extent = extents_by_id.get(*kid).copied().unwrap_or(0.0);
                pushed.push((*kid, dir, center_depth, my_depth_size, child_top));
                child_top += extent + cross_gap;
            }
            // Reversed: the stack pops the first sibling first.
            stack.extend(pushed.into_iter().rev());
        }
    }

    Layout { positions, sides }
}

/// The cross-axis extent of every placed subtree, per orientation (memoized bottom-up).
fn extents<'a>(index: &Index<'a>, vertical: bool) -> HashMap<&'a str, f32> {
    let mut out: HashMap<&str, f32> = HashMap::new();
    let Some(root_id) = index.root else { return out };
    // Post-order over the reachable tree, without recursion.
    let mut stack: Vec<(&str, bool)> = vec![(root_id, false)];
    let mut visited = HashSet::new();
    while let Some((id, expanded)) = stack.pop() {
        let Some(node) = index.by_id.get(id) else { continue };
        let kids = index.placed_kids(node);
        if !expanded {
            if !visited.insert(id) {
                continue;
            }
            stack.push((id, true));
            for kid in kids.iter().rev() {
                stack.push((kid, false));
            }
            continue;
        }
        let cross_size = if vertical { width_of(node) } else { height_of(node) };
        let cross_gap = if vertical { H_CROSS_GAP } else { V_GAP };
        let mut value = cross_size;
        if !kids.is_empty() {
            let mut sum = 0.0;
            for kid in &kids {
                sum += out.get(kid).copied().unwrap_or(0.0) + cross_gap;
            }
            sum -= cross_gap;
            value = cross_size.max(sum);
        }
        out.insert(id, value);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use localmind_model::MindNode;

    /// A node without the boilerplate: `text` is the id, which is what the assertions show.
    fn node(id: &str, parent: Option<&str>) -> MindNode {
        MindNode::new(id, id, parent.map(str::to_string))
    }

    fn with(id: &str, parent: Option<&str>, extra: impl FnOnce(&mut MindNode)) -> MindNode {
        let mut n = node(id, parent);
        extra(&mut n);
        n
    }

    /// The box a card occupies, for the overlap assertions.
    fn card(nodes: &[MindNode], id: &str) -> (f32, f32, f32, f32) {
        let layout = layout_tree(nodes);
        let pos = layout.positions.get(id).expect("positioned");
        let node = nodes.iter().find(|n| n.id == id).expect("node");
        let size = node_size(node);
        (pos.x, pos.y, size.w, size.h)
    }

    #[test]
    fn a_width_of_zero_or_less_falls_back_to_the_default() {
        assert_eq!(node_size(&with("a", None, |n| n.width = Some(0.0))).w, NODE_W);
        assert_eq!(node_size(&with("a", None, |n| n.width = Some(-50.0))).w, NODE_W);
    }

    /// A card narrower than its own padding has no room for text, and the character count per line
    /// divides by that: without the floor of six, the height would be infinite and every position
    /// downstream a NaN.
    #[test]
    fn the_height_stays_finite_when_the_card_is_narrower_than_its_padding() {
        let size = node_size(&with("a", None, |n| {
            n.width = Some(30.0);
            n.text = "x".repeat(60);
        }));
        assert!(size.h.is_finite());
        assert!(size.h > NODE_H);
    }

    #[test]
    fn a_font_size_of_zero_measures_like_the_default() {
        let text = "a\nb\nc\nd\ne";
        let zero = node_size(&with("a", None, |n| {
            n.text = text.into();
            n.font_size = Some(0.0);
        }));
        let none = node_size(&with("a", None, |n| n.text = text.into()));
        assert_eq!(zero.h, none.h);
        assert!(zero.h > NODE_H);
    }

    /// Explicit line breaks are lines: measuring only the total length would give one line and the
    /// tall block would run into its neighbour.
    #[test]
    fn explicit_line_breaks_count_towards_the_height() {
        let one = node_size(&with("a", None, |n| n.text = "abcde".into()));
        let five = node_size(&with("a", None, |n| n.text = "a\nb\nc\nd\ne".into()));
        assert!(five.h > one.h);
    }

    #[test]
    fn the_sides_are_empty_without_a_root() {
        assert!(resolve_sides(&[]).is_empty());
        // A forest where everyone points at a parent: no root, no sides.
        assert!(resolve_sides(&[node("a", Some("x")), node("b", Some("a"))]).is_empty());
    }

    #[test]
    fn root_children_without_a_side_alternate_right_and_left() {
        let nodes = [node("r", None), node("a", Some("r")), node("b", Some("r")), node("c", Some("r")), node("d", Some("r"))];
        let sides = resolve_sides(&nodes);
        let got: Vec<_> = ["a", "b", "c", "d"].iter().map(|id| sides.get(*id).copied()).collect();
        assert_eq!(got, [Some(ResolvedSide::Right), Some(ResolvedSide::Left), Some(ResolvedSide::Right), Some(ResolvedSide::Left)]);
    }

    /// `up`/`down` are their own axes: counting them in the left/right load would push every
    /// automatic branch to one side of a map that has a single vertical branch.
    #[test]
    fn an_explicit_vertical_side_does_not_unbalance_left_and_right() {
        let nodes = [node("r", None), with("u", Some("r"), |n| n.side = Some(Side::Up)), node("a", Some("r")), node("b", Some("r"))];
        let sides = resolve_sides(&nodes);
        assert_eq!(sides.get("u"), Some(&ResolvedSide::Up));
        assert_eq!(sides.get("a"), Some(&ResolvedSide::Right));
        assert_eq!(sides.get("b"), Some(&ResolvedSide::Left));
    }

    /// If only the root's direct children got a side, the grandchildren would have no entry and the
    /// layout would drop the whole subtree.
    #[test]
    fn the_side_propagates_through_the_whole_subtree() {
        let nodes = [node("r", None), with("u", Some("r"), |n| n.side = Some(Side::Up)), node("filho", Some("u")), node("neto", Some("filho"))];
        let sides = resolve_sides(&nodes);
        assert_eq!(sides.get("filho"), Some(&ResolvedSide::Up));
        assert_eq!(sides.get("neto"), Some(&ResolvedSide::Up));
    }

    #[test]
    fn nothing_is_placed_without_a_root() {
        assert!(layout_tree(&[]).positions.is_empty());
        assert!(layout_tree(&[node("a", Some("inexistente"))]).positions.is_empty());
    }

    /// The root is the centre of the coordinate system: anchored at (0,0) instead of centred, the
    /// left and upper branches would sit half a card away from the right and lower ones.
    #[test]
    fn the_root_is_centred_on_the_origin() {
        let nodes = [node("r", None)];
        let size = node_size(&nodes[0]);
        let pos = layout_tree(&nodes).positions.get("r").copied().expect("root");
        assert_eq!((pos.x, pos.y), (-size.w / 2.0, -size.h / 2.0));
    }

    #[test]
    fn the_map_grows_to_negative_x_on_the_left_and_positive_on_the_right() {
        let nodes = [node("r", None), node("a", Some("r")), node("b", Some("r"))];
        let layout = layout_tree(&nodes);
        assert_eq!(layout.sides.get("a"), Some(&ResolvedSide::Right));
        assert!(layout.positions.get("a").expect("a").x > 0.0);
        assert!(layout.positions.get("b").expect("b").x < -NODE_W);
    }

    #[test]
    fn the_map_grows_to_negative_y_up_and_positive_down() {
        let nodes = [node("r", None), with("u", Some("r"), |n| n.side = Some(Side::Up)), with("d", Some("r"), |n| n.side = Some(Side::Down))];
        let layout = layout_tree(&nodes);
        assert!(layout.positions.get("u").expect("u").y < -NODE_H);
        assert!(layout.positions.get("d").expect("d").y > 0.0);
    }

    /// The block of siblings is centred on the root: stacking from zero would grow the map
    /// downwards only, with the root at the top instead of in the middle.
    #[test]
    fn the_sibling_block_is_centred_on_the_roots_cross_axis() {
        let nodes = [node("r", None), with("a", Some("r"), |n| n.side = Some(Side::Right)), with("b", Some("r"), |n| n.side = Some(Side::Right))];
        let layout = layout_tree(&nodes);
        let center = |id: &str| {
            let pos = layout.positions.get(id).expect("placed");
            let node = nodes.iter().find(|n| n.id == id).expect("node");
            pos.y + node_size(node).h / 2.0
        };
        assert!((center("a") + center("b")).abs() < 0.01);
    }

    /// The band a sibling reserves is its subtree's, not its card's: measuring only the card lets a
    /// tall node run into the one below it.
    #[test]
    fn siblings_of_different_heights_do_not_overlap() {
        let nodes = [
            node("r", None),
            with("alto", Some("r"), |n| {
                n.side = Some(Side::Right);
                n.text = "l1\nl2\nl3\nl4\nl5\nl6".into();
            }),
            with("baixo", Some("r"), |n| n.side = Some(Side::Right)),
        ];
        let (_, y_alto, _, h_alto) = card(&nodes, "alto");
        let (_, y_baixo, _, _) = card(&nodes, "baixo");
        assert!(y_alto + h_alto <= y_baixo);
    }

    /// The same rule one level up: a sibling with a big subtree needs the subtree's band.
    #[test]
    fn the_band_is_the_whole_subtree_not_the_siblings_card() {
        let nodes = [
            node("r", None),
            with("pai", Some("r"), |n| n.side = Some(Side::Right)),
            node("f1", Some("pai")),
            node("f2", Some("pai")),
            node("f3", Some("pai")),
            with("vizinho", Some("r"), |n| n.side = Some(Side::Right)),
        ];
        let (_, y_f3, _, h_f3) = card(&nodes, "f3");
        let (_, y_vizinho, _, _) = card(&nodes, "vizinho");
        assert!(y_f3 + h_f3 <= y_vizinho);
    }

    /// Collapsing hides the children from the drawing, but their side stays resolved: expanding
    /// again must not recompute a different direction.
    #[test]
    fn a_collapsed_node_hides_its_descendants_but_keeps_their_side() {
        let nodes = [node("r", None), with("pai", Some("r"), |n| n.collapsed = Some(true)), node("filho", Some("pai")), node("neto", Some("filho"))];
        let layout = layout_tree(&nodes);
        assert!(layout.positions.contains_key("pai"));
        assert!(!layout.positions.contains_key("filho"));
        assert!(!layout.positions.contains_key("neto"));
        assert_eq!(layout.sides.get("neto"), Some(&ResolvedSide::Right));
    }

    #[test]
    fn a_collapsed_root_still_shows_itself() {
        let nodes = [with("r", None, |n| n.collapsed = Some(true)), node("a", Some("r"))];
        let layout = layout_tree(&nodes);
        assert!(layout.positions.contains_key("r"));
        assert!(!layout.positions.contains_key("a"));
    }

    /// Duplicated root: the layout takes the FIRST one, exactly like the exports. Two different
    /// answers for "which one is the root" made the screen and the exported file disagree in the
    /// original app.
    #[test]
    fn a_duplicated_root_uses_the_first_one() {
        let nodes = [node("R1", None), node("R2", None), node("C", Some("R1"))];
        let layout = layout_tree(&nodes);
        assert!(layout.positions.contains_key("R1"));
        assert!(layout.positions.contains_key("C"));
        assert!(!layout.positions.contains_key("R2"));
    }

    /// A hand-edited file can point nodes at each other in a loop, leaving them unreachable from
    /// the root. The layout has to come back with something instead of hanging.
    #[test]
    fn a_cycle_in_the_file_does_not_hang_the_layout() {
        let mut a = node("a", Some("b"));
        a.parent_id = Some("b".into());
        let mut b = node("b", Some("a"));
        b.parent_id = Some("a".into());
        let nodes = [node("r", None), node("kid", Some("r")), a, b];
        let layout = layout_tree(&nodes);
        assert!(layout.positions.contains_key("r"));
        assert!(layout.positions.contains_key("kid"));
        assert!(!layout.positions.contains_key("a"));
        assert!(!layout.positions.contains_key("b"));
    }

    /// A very deep chain is still laid out: the traversal uses an explicit stack, so the app does
    /// not overflow the thread's stack on a file it did not create.
    #[test]
    fn a_very_deep_chain_is_laid_out_without_recursion() {
        let mut nodes = vec![node("n0", None)];
        for i in 1..5_000 {
            nodes.push(node(&format!("n{i}"), Some(&format!("n{}", i - 1))));
        }
        let layout = layout_tree(&nodes);
        assert_eq!(layout.positions.len(), 5_000);
    }

    #[test]
    fn the_bounds_cover_every_placed_card_and_nothing_else() {
        let nodes = [node("r", None), node("a", Some("r"))];
        let layout = layout_tree(&nodes);
        let (origin, size) = layout.bounds(&nodes).expect("bounds");
        for node in &nodes {
            let pos = layout.positions.get(&node.id).expect("placed");
            let card = node_size(node);
            assert!(pos.x >= origin.x - 0.01 && pos.y >= origin.y - 0.01);
            assert!(pos.x + card.w <= origin.x + size.w + 0.01);
            assert!(pos.y + card.h <= origin.y + size.h + 0.01);
        }
    }
}
