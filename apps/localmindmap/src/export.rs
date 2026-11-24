//! Exporting the map: Markdown, HTML and (when the rasterizer lands) PNG.
//!
//! Markdown and HTML are ports of the original app's `export.ts`, including the fixes it paid for:
//! the root's note is written out (the walk only visits children), every user string is escaped
//! (a `<script>` in a node is text, not code), the colour lands in an attribute escaped, and a leaf
//! does not emit an empty `<ul>`. HTML takes the current theme's colours, so the exported page looks
//! like the map the user was looking at.

use localmindmap_layout::layout_tree;
use localmindmap_model::MindMap;

use crate::theme::Colors;

/// The map as a Markdown nested-bullet outline (the format also reads back as a list).
pub fn markdown(map: &MindMap) -> String {
    let Some(root) = map.root() else { return String::new() };
    let mut lines: Vec<String> = vec![format!("# {}", root.text), String::new()];
    if root.note.as_deref().is_some_and(|note| !note.trim().is_empty()) {
        lines.push(format!("_{}_", root.note.as_deref().unwrap_or_default()));
        lines.push(String::new());
    }
    fn walk(map: &MindMap, parent: &str, depth: usize, out: &mut Vec<String>) {
        for child in map.children(parent) {
            let note = child.note.as_deref().filter(|note| !note.trim().is_empty());
            let suffix = note.map_or_else(String::new, |note| format!(" - _{note}_"));
            out.push(format!("{}- {}{suffix}", "  ".repeat(depth), child.text));
            walk(map, &child.id, depth + 1, out);
        }
    }
    walk(map, &root.id, 0, &mut lines);
    format!("{}\n", lines.join("\n"))
}

/// The map as a self-contained HTML page with a nested list.
pub fn html(map: &MindMap, colors: &Colors, lang: &str) -> String {
    let Some(root) = map.root() else { return "<!doctype html><title></title>".to_string() };
    fn walk(map: &MindMap, parent: &str, html: &mut String) {
        let children = map.children(parent);
        if children.is_empty() {
            return;
        }
        html.push_str("<ul>");
        for child in children {
            let color = child
                .color
                .as_deref()
                .filter(|color| !color.trim().is_empty())
                .map_or_else(String::new, |color| format!(" style=\"border-color:{}\"", escape(color)));
            let note = child
                .note
                .as_deref()
                .filter(|note| !note.trim().is_empty())
                .map_or_else(String::new, |note| format!("<span class=\"note\">{}</span>", escape(note)));
            html.push_str(&format!("<li><span class=\"t\"{color}>{}</span>{note}", escape(&child.text)));
            walk(map, &child.id, html);
            html.push_str("</li>");
        }
        html.push_str("</ul>");
    }
    let mut list = String::new();
    walk(map, &root.id, &mut list);

    let root_note = root
        .note
        .as_deref()
        .filter(|note| !note.trim().is_empty())
        .map_or_else(String::new, |note| format!("<p class=\"note\">{}</p>", escape(note)));
    format!(
        "<!doctype html>\n<html lang=\"{lang}\"><head><meta charset=\"utf-8\">\n<title>{title}</title>\n<style>\n\
         body{{background:{bg};color:{text};font-family:Segoe UI,system-ui,sans-serif;padding:32px;line-height:1.5}}\n\
         h1{{color:{accent}}}\n\
         ul{{list-style:none;border-left:1px solid {border};margin-left:10px;padding-left:18px}}\n\
         li{{margin:6px 0}}\n\
         .t{{display:inline-block;background:{panel};border:1px solid {border};border-left:4px solid {accent2};border-radius:8px;padding:4px 10px}}\n\
         .note{{color:{muted};font-size:.85em;margin-left:8px}}\n\
         </style></head>\n<body><h1>{title}</h1>{root_note}{list}</body></html>\n",
        bg = hex(colors.canvas),
        text = hex(colors.text),
        accent = hex(colors.accent),
        accent2 = hex(colors.accent_second),
        panel = hex(colors.panel),
        border = hex(colors.border),
        muted = hex(colors.muted),
        title = escape(&root.text),
        root_note = root_note,
        list = list,
    )
}

/// The map as SVG: the same cards and curves the canvas draws, as vectors. It is what a mind map
/// wants to be pasted into a document, and it needs no rasterizer.
pub fn svg(map: &MindMap, colors: &Colors) -> String {
    let layout = layout_tree(&map.nodes);
    let Some((origin, size)) = layout.bounds(&map.nodes) else { return String::new() };
    let pad = 40.0;
    let width = size.w + 2.0 * pad;
    let height = size.h + 2.0 * pad;
    let mut out = String::new();
    out.push_str(&format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width:.0}\" height=\"{height:.0}\" viewBox=\"0 0 {width:.0} {height:.0}\">\n"
    ));
    out.push_str(&format!("<rect width=\"{width:.0}\" height=\"{height:.0}\" fill=\"{}\"/>\n", hex(colors.canvas)));
    // Links first, under the cards: from the parent's centre to the child's.
    out.push_str(&format!("<g fill=\"none\" stroke=\"{}\" stroke-width=\"1.5\">\n", hex(colors.border)));
    for node in &map.nodes {
        let (Some(parent_id), Some(child_pos)) = (node.parent_id.as_deref(), layout.positions.get(&node.id)) else { continue };
        let (Some(parent_pos), Some(parent)) = (layout.positions.get(parent_id), map.node(parent_id)) else { continue };
        let from = (parent_pos.x + width_of(parent) / 2.0 - origin.x + pad, parent_pos.y + height_of(parent) / 2.0 - origin.y + pad);
        let to = (child_pos.x + width_of(node) / 2.0 - origin.x + pad, child_pos.y + height_of(node) / 2.0 - origin.y + pad);
        out.push_str(&format!(
            "<path d=\"M{:.1} {:.1} C{:.1} {:.1} {:.1} {:.1} {:.1} {:.1}\"/>\n",
            from.0,
            from.1,
            (from.0 + to.0) / 2.0,
            from.1,
            (from.0 + to.0) / 2.0,
            to.1,
            to.0,
            to.1
        ));
    }
    out.push_str("</g>\n");
    for node in &map.nodes {
        let Some(pos) = layout.positions.get(&node.id) else { continue };
        let x = pos.x - origin.x + pad;
        let y = pos.y - origin.y + pad;
        let w = width_of(node);
        let h = height_of(node);
        let is_root = node.parent_id.is_none();
        let fill = if is_root { colors.panel.lerp_to_gamma(colors.accent, 0.18) } else { colors.panel };
        // The node's own colour when it is a real `#rrggbb`; the theme's accent otherwise, so a
        // hand-edited value never breaks the attribute.
        let bar = node_color(node).map_or_else(|| hex(if is_root { colors.accent } else { colors.accent_second }), hex);
        out.push_str(&format!(
            "<rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" rx=\"10\" fill=\"{}\" stroke=\"{}\"/>\n",
            hex(fill),
            hex(colors.border)
        ));
        out.push_str(&format!("<rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"5\" height=\"{h:.1}\" rx=\"2.5\" fill=\"{bar}\"/>\n"));
        // The text is split into lines the same way the layout estimates its height.
        let font = font_of(node);
        let line_height = font * localmindmap_layout::LINE_HEIGHT;
        let lines = wrap(&node.text, w - 30.0, font);
        let block = lines.len() as f32 * line_height;
        for (i, line) in lines.iter().enumerate() {
            let text_y = y + (h - block) / 2.0 + line_height * (i as f32 + 0.8);
            out.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{text_y:.1}\" font-family=\"Segoe UI, system-ui, sans-serif\" font-size=\"{font:.1}\" fill=\"{}\">{}</text>\n",
                x + 14.0,
                hex(colors.text),
                escape(line)
            ));
        }
    }
    out.push_str("</svg>\n");
    out
}

fn width_of(node: &localmindmap_model::MindNode) -> f32 {
    localmindmap_layout::width_of(node)
}
fn height_of(node: &localmindmap_model::MindNode) -> f32 {
    localmindmap_layout::node_size(node).h
}

fn font_of(node: &localmindmap_model::MindNode) -> f32 {
    localmindmap_layout::font_of(node)
}

/// A greedy word wrap at `max_width` for `font` (the same 0.56 average the layout uses).
fn wrap(text: &str, max_width: f32, font: f32) -> Vec<String> {
    let per_char = (font * 0.56).max(1.0);
    let max_chars = ((max_width / per_char).floor() as usize).max(6);
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if candidate.chars().count() > max_chars && !line.is_empty() {
                lines.push(line);
                line = word.to_string();
            } else {
                line = candidate;
            }
        }
        lines.push(line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn hex(color: egui::Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b())
}

/// A node's own `#rrggbb` colour, when it is a real one.
fn node_color(node: &localmindmap_model::MindNode) -> Option<egui::Color32> {
    let hex = node.color.as_deref()?.trim().strip_prefix('#')?;
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).ok();
    Some(egui::Color32::from_rgb(channel(0)?, channel(2)?, channel(4)?))
}

/// The four characters that turn user text into markup, and back into text.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;
    use localmindmap_model::{MindMap, MindNode};

    fn map(nodes: Vec<MindNode>) -> MindMap {
        MindMap { id: "m".into(), title: "m".into(), nodes, created_at: 0, updated_at: 0, extra: Default::default() }
    }

    fn node(id: &str, parent: Option<&str>) -> MindNode {
        MindNode::new(id, id, parent.map(str::to_string))
    }

    fn colors() -> Colors {
        Theme::Dark.colors()
    }

    #[test]
    fn markdown_is_empty_without_a_root() {
        assert_eq!(markdown(&map(vec![])), "");
        assert_eq!(markdown(&map(vec![node("a", Some("inexistente"))])), "");
    }

    /// The root's children are at depth zero (the root is the `#` title) and every level adds two
    /// spaces; an indented first bullet would be read as a code block.
    #[test]
    fn markdown_indents_from_zero_and_adds_two_spaces_per_level() {
        let md = markdown(&map(vec![node("R", None), node("C", Some("R")), node("G", Some("C"))]));
        assert_eq!(md, "# R\n\n- C\n  - G\n");
    }

    #[test]
    fn markdown_writes_a_note_only_when_there_is_one() {
        let mut with_note = node("com", Some("R"));
        with_note.note = Some("obs".into());
        let md = markdown(&map(vec![node("R", None), node("sem", Some("R")), with_note]));
        assert!(md.contains("\n- sem\n"));
        assert!(md.contains("- com - _obs_"));
        assert!(!md.contains("undefined"));
    }

    #[test]
    fn markdown_drops_an_orphan_whose_parent_is_gone() {
        let md = markdown(&map(vec![node("R", None), node("ok", Some("R")), node("orfao", Some("sumiu"))]));
        assert!(md.contains("- ok"));
        assert!(!md.contains("orfao"));
    }

    /// The `walk` only visits children, so the root's note used to be lost in the export.
    #[test]
    fn the_root_note_is_exported() {
        let mut root = node("R", None);
        root.note = Some("contexto".into());
        let md = markdown(&map(vec![root, node("C", Some("R"))]));
        assert!(md.contains("_contexto_"));
    }

    #[test]
    fn html_escapes_the_node_text() {
        let mut evil = node("x", Some("R"));
        evil.text = "<script>a & b</script>".into();
        let html = html(&map(vec![node("R", None), evil]), &colors(), "pt-BR");
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;a &amp; b&lt;/script&gt;"));
    }

    #[test]
    fn html_escapes_the_root_text_in_the_title_and_the_heading() {
        let mut root = node("R", None);
        root.text = "<b>t</b>".into();
        let html = html(&map(vec![root]), &colors(), "en");
        assert!(!html.contains("<b>t</b>"));
        assert!(html.contains("<title>&lt;b&gt;t&lt;/b&gt;</title>"));
    }

    /// The colour lands inside a double-quoted attribute: an unescaped quote would close it.
    #[test]
    fn html_escapes_the_quote_in_a_colour() {
        let mut evil = node("x", Some("R"));
        evil.color = Some("#fff\" onmouseover=\"evil()".into());
        let html = html(&map(vec![node("R", None), evil]), &colors(), "en");
        assert!(!html.contains("onmouseover=\"evil()\""));
        assert!(html.contains("&quot;"));
    }

    #[test]
    fn html_escapes_the_note_and_the_root_note() {
        let mut child = node("x", Some("R"));
        child.note = Some("<i>n</i>".into());
        let mut root = node("R", None);
        root.note = Some("<b>x</b>".into());
        let html = html(&map(vec![root, child]), &colors(), "en");
        assert!(!html.contains("<i>n</i>"));
        assert!(html.contains("&lt;i&gt;n&lt;/i&gt;"));
        assert!(html.contains("&lt;b&gt;x&lt;/b&gt;"));
    }

    #[test]
    fn html_does_not_emit_an_empty_list_for_a_leaf() {
        let html = html(&map(vec![node("R", None), node("x", Some("R"))]), &colors(), "en");
        assert!(!html.contains("<ul></ul>"));
    }

    #[test]
    fn html_without_a_root_is_a_minimal_document() {
        let html = html(&map(vec![]), &colors(), "en");
        assert!(html.starts_with("<!doctype html>"));
        assert!(!html.contains("<ul>"));
    }

    #[test]
    fn html_carries_the_theme_colours_and_the_language() {
        let html = html(&map(vec![node("R", None)]), &Theme::Light.colors(), "es-ES");
        assert!(html.contains("lang=\"es-ES\""));
        assert!(html.contains(&hex(Theme::Light.colors().canvas)));
    }

    /// Two roots: the export uses the first, the same one the layout draws.
    #[test]
    fn a_duplicated_root_uses_the_first_one() {
        let md = markdown(&map(vec![node("R1", None), node("R2", None), node("C", Some("R1"))]));
        assert!(md.starts_with("# R1"));
        let html = html(&map(vec![node("R1", None), node("R2", None), node("C", Some("R1"))]), &colors(), "en");
        assert!(html.contains("<h1>R1</h1>"));
    }

    #[test]
    fn svg_has_every_card_and_every_link() {
        let svg = svg(&map(vec![node("R", None), node("a", Some("R")), node("b", Some("a"))]), &colors());
        assert!(svg.starts_with("<?xml"));
        assert!(svg.ends_with("</svg>\n"));
        assert_eq!(svg.matches("<rect x=").count(), 6, "three cards + three accent bars");
        assert_eq!(svg.matches("<path d=").count(), 2, "one curve per link");
        assert_eq!(svg.matches("<text ").count(), 3);
    }

    #[test]
    fn svg_is_empty_without_a_root() {
        assert!(svg(&map(vec![node("a", Some("x"))]), &colors()).is_empty());
    }

    #[test]
    fn the_wrap_never_returns_no_lines() {
        assert_eq!(wrap("", 200.0, 13.5), [""]);
        assert_eq!(wrap("uma linha", 400.0, 13.5).len(), 1);
        assert!(wrap("palavra ".repeat(30).as_str(), 120.0, 13.5).len() > 1);
    }
}
