//! The map as the chat's context: the outline the model reads, bounded.
//!
//! The chat used to see only the system prompt, so it could not answer anything about the map the
//! user is looking at. This module turns the map into an indented outline of node titles - the root
//! first, each branch below it - within a character budget, so a big map never crowds the
//! conversation out of the request. `craft-ai-ui` carries it as the panel's context and displays
//! what was actually sent in the chip.

use std::collections::HashSet;

use localmind_model::MindMap;

/// How much of the map one chat request carries (about 2k tokens of text).
pub const MAP_BUDGET: usize = 8_000;

/// The outline, and what it covers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outline {
    /// The outline itself: one line per node, two spaces of indentation per level.
    pub text: String,
    /// How many nodes the outline names.
    pub nodes: usize,
    /// Whether the budget cut the map (the text then ends with a marker line).
    pub cut: bool,
}

/// The map as an indented outline of node titles: the root, then each branch below it, in the
/// map's own order.
///
/// Iterative on purpose: a file written by another tool can have a parent link that loops, and a
/// recursive walk would never return. A node already visited is skipped, and a node the map does
/// not hold ends its branch.
pub fn map_outline(map: &MindMap, budget: usize) -> Outline {
    let mut text = String::new();
    let mut nodes = 0;
    let mut cut = false;
    let Some(root) = map.root_id() else { return Outline { text, nodes, cut } };
    let mut seen: HashSet<&str> = HashSet::new();
    let mut stack: Vec<(&str, usize)> = vec![(root, 0)];
    while let Some((id, depth)) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        let Some(node) = map.node(id) else { continue };
        let title = one_line(&node.text);
        if title.is_empty() {
            continue;
        }
        let line = format!("{}- {title}\n", "  ".repeat(depth));
        if text.chars().count() + line.chars().count() > budget {
            cut = true;
            break;
        }
        text.push_str(&line);
        nodes += 1;
        // Reversed, so the first child is the first one popped (the map's own order).
        for child in map.children(id).into_iter().rev() {
            stack.push((child.id.as_str(), depth + 1));
        }
    }
    if cut {
        text.push_str("-\u{2026}\n");
    }
    Outline { text, nodes, cut }
}

/// A node's title as one line: a text the user (or the model) broke over lines is one node still.
fn one_line(text: &str) -> String {
    text.split(['\n', '\r']).map(str::trim).filter(|part| !part.is_empty()).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use localmind_model::MindMap;

    /// `raiz` with two branches, the first of them with two children.
    fn map() -> MindMap {
        let mut map = MindMap::new("Ata", "raiz");
        let root = map.nodes[0].id.clone();
        let a = map.add_child(&root, "primeiro", None);
        let b = map.add_child(&root, "segundo", None);
        map.add_child(&a, "primeiro A", None);
        map.add_child(&a, "primeiro B", None);
        let _ = b;
        map
    }

    /// The outline names the root first and indents each level, in the map's own order.
    #[test]
    fn the_outline_is_the_root_then_each_branch_indented() {
        let outline = map_outline(&map(), MAP_BUDGET);
        assert!(!outline.cut, "{outline:?}");
        assert_eq!(outline.nodes, 5);
        assert_eq!(outline.text, "- raiz\n  - primeiro\n    - primeiro A\n    - primeiro B\n  - segundo\n");
    }

    /// A line break inside a node's text does not become a second outline line.
    #[test]
    fn a_multi_line_title_stays_one_line() {
        let mut map = MindMap::new("Ata", "raiz\ncom quebra");
        let root = map.nodes[0].id.clone();
        map.add_child(&root, "  filho\n  com quebra  ", None);
        let outline = map_outline(&map, MAP_BUDGET);
        assert_eq!(outline.text, "- raiz com quebra\n  - filho com quebra\n");
    }

    /// A budget the map does not fit cuts it and says so, keeping whole lines.
    #[test]
    fn the_budget_cuts_the_outline_and_says_so() {
        let full = map_outline(&map(), MAP_BUDGET);
        let outline = map_outline(&map(), 20);
        assert!(outline.cut, "{outline:?}");
        assert!(outline.nodes < full.nodes);
        assert!(outline.text.ends_with("-\u{2026}\n"), "{:?}", outline.text);
        // The first line always fits, so the model still knows what the map is about.
        assert!(outline.text.starts_with("- raiz\n"));
        assert!(outline.text.chars().count() <= 20 + "-\u{2026}\n".chars().count());
    }

    /// A malformed file can repeat an id (two nodes pointing at each other's branches): the walk
    /// visits each id once instead of looping.
    #[test]
    fn a_repeated_id_is_walked_once() {
        let mut map = MindMap::new("Ata", "raiz");
        let root = map.nodes[0].id.clone();
        let a = map.add_child(&root, "a", None);
        let b = map.add_child(&a, "b", None);
        // A second node with the same id as `a`, hanging from `b`: without the visited set the
        // walk would keep going a -> b -> a.
        map.nodes.push(localmind_model::MindNode::new(a.clone(), "a de novo", Some(b.clone())));
        let outline = map_outline(&map, MAP_BUDGET);
        assert_eq!(outline.text, "- raiz\n  - a\n    - b\n");
        assert_eq!(outline.nodes, 3);
    }

    /// An empty map is no context at all.
    #[test]
    fn an_empty_map_is_an_empty_outline() {
        let mut map = MindMap::new("Ata", "raiz");
        map.nodes.clear();
        let outline = map_outline(&map, MAP_BUDGET);
        assert_eq!(outline, Outline { text: String::new(), nodes: 0, cut: false });
    }
}
