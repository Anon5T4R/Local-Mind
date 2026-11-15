//! The five actions and the chat: request -> messages, answer -> nodes.
//!
//! What a user asks for is one [`Request`]; what comes back is an [`Outcome`] the editor applies as
//! one undo step. The prompts carry the JSON contract, [`crate::extract`] reads the answer, and this
//! module turns it into nodes with ids of the app's own.

use local_ai::Msg;
use localmind_model::{MindMap, MindNode, new_id};

use crate::extract::extract_json;
use crate::prompts::{fill, for_lang};

/// What the user asked for.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    GenerateMap { topic: String, depth: u32, breadth: u32 },
    Expand { node: String, count: u32 },
    DeepExpand { node: String },
    Explain { node: String },
    Rephrase { node: String, instruction: String },
}

impl Request {
    /// The map node this request is about, `None` for a whole-map generation.
    pub fn node(&self) -> Option<&str> {
        match self {
            Request::GenerateMap { .. } => None,
            Request::Expand { node, .. } | Request::DeepExpand { node } | Request::Explain { node } | Request::Rephrase { node, .. } => Some(node),
        }
    }

    /// The status line while the model works.
    pub fn status(&self, map: &MindMap) -> String {
        let text = self.node().and_then(|id| map.node(id)).map(|node| node.text.clone()).unwrap_or_default();
        match self {
            Request::GenerateMap { topic, .. } => topic.clone(),
            Request::Expand { .. } => format!("Expand: {text}"),
            Request::DeepExpand { .. } => format!("Expand deep: {text}"),
            Request::Explain { .. } => format!("Explain: {text}"),
            Request::Rephrase { .. } => format!("Rewrite: {text}"),
        }
    }
}

/// What the answer became.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// A whole new map: the first node is the root (`parent_id: None`).
    NewMap { title: String, nodes: Vec<MindNode> },
    /// New nodes under `parent` (the first level) and below.
    Nodes { parent: String, nodes: Vec<MindNode> },
    /// One text: a rephrased node, or an explanation to add as a child.
    Text(String),
}

/// The conversation one request sends: a system turn and one user turn.
pub fn messages(map: &MindMap, request: &Request, lang: &str) -> Result<Vec<Msg>, String> {
    let prompts = for_lang(lang);
    let system = Msg::System(fill(prompts.system, &[("answer_in", prompts.answer_in)]));
    let user = match request {
        Request::GenerateMap { topic, depth, breadth } => {
            let topic = topic.trim();
            if topic.is_empty() {
                return Err("the topic is empty".into());
            }
            let task =
                if topic.chars().count() > 200 { fill(prompts.gen_long, &[("topic", topic)]) } else { fill(prompts.gen_short, &[("topic", topic)]) };
            let rules = fill(prompts.gen_rules, &[("breadth", &breadth.to_string()), ("depth", &depth.to_string())]);
            format!("{task}\n\n{rules}")
        }
        Request::Expand { node, count } => {
            let target = map.node(node).ok_or("the node is gone")?;
            let path = map.path_texts(node);
            let context = if path.len() > 1 { fill(prompts.expand_ctx, &[("path", &path.join(" > "))]) } else { String::new() };
            let existing = other_texts(map, node);
            let avoid = if existing.is_empty() {
                String::new()
            } else {
                let list: Vec<String> = existing.iter().map(|text| format!("\"{text}\"")).collect();
                fill(prompts.expand_avoid, &[("list", &list.join(", "))])
            };
            fill(prompts.expand_body, &[("ctx", &context), ("count", &count.to_string()), ("text", &target.text), ("avoid", &avoid)])
        }
        Request::DeepExpand { node } => {
            let target = map.node(node).ok_or("the node is gone")?;
            let path = map.path_texts(node);
            let existing = other_texts(map, node);
            let avoid = if existing.is_empty() {
                String::new()
            } else {
                let list: Vec<String> = existing.iter().map(|text| format!("\"{text}\"")).collect();
                fill(prompts.deep_avoid, &[("list", &list.join(", "))])
            };
            fill(prompts.deep_body, &[("path", &path.join(" > ")), ("text", &target.text), ("breadth", "4"), ("sub", "3"), ("avoid", &avoid)])
        }
        Request::Explain { node } => {
            let target = map.node(node).ok_or("the node is gone")?;
            let path = map.path_texts(node);
            fill(prompts.explain, &[("path", &path.join(" > ")), ("text", &target.text)])
        }
        Request::Rephrase { node, instruction } => {
            let target = map.node(node).ok_or("the node is gone")?;
            fill(prompts.rephrase, &[("text", &target.text), ("instruction", instruction.trim())])
        }
    };
    Ok(vec![system, Msg::User(user)])
}

/// The system turn of the chat panel.
pub fn chat_system(lang: &str) -> String {
    let prompts = for_lang(lang);
    fill(prompts.chat_system, &[("answer_in", prompts.answer_in)])
}

/// Turns one answer into what the editor applies.
pub fn interpret(request: &Request, answer: &str) -> Result<Outcome, String> {
    match request {
        Request::GenerateMap { topic, .. } => {
            let tree: Tree = extract_json(answer)?;
            if tree.text.trim().is_empty() {
                return Err("the model did not answer any node".into());
            }
            let nodes = tree_nodes(&tree);
            let title: String = topic.trim().chars().take(60).collect();
            Ok(Outcome::NewMap { title, nodes })
        }
        Request::Expand { node, .. } => {
            let items: Items = extract_json(answer)?;
            let nodes: Vec<MindNode> = items
                .items
                .iter()
                .map(|text| text.trim())
                .filter(|text| !text.is_empty())
                .map(|text| MindNode::new(new_id(), text, Some(node.clone())))
                .collect();
            if nodes.is_empty() {
                return Err("the model did not answer any subtopic".into());
            }
            Ok(Outcome::Nodes { parent: node.clone(), nodes })
        }
        Request::DeepExpand { node } => {
            let deep: Deep = extract_json(answer)?;
            let nodes = deep_nodes(&deep, node);
            if nodes.is_empty() {
                return Err("the model did not answer any subtopic".into());
            }
            Ok(Outcome::Nodes { parent: node.clone(), nodes })
        }
        Request::Explain { .. } | Request::Rephrase { .. } => {
            let text: Text = extract_json(answer)?;
            let trimmed = text.text.trim();
            if trimmed.is_empty() {
                return Err("the model answered an empty text".into());
            }
            Ok(Outcome::Text(trimmed.to_string()))
        }
    }
}

/// A markdown-ish outline as nodes: bullets, numbered lists, headings and indentation. Used by the
/// chat's "insert as branch".
pub fn parse_outline(text: &str, parent: &str) -> Vec<MindNode> {
    let mut out: Vec<MindNode> = Vec::new();
    // (id, depth) of the item that can be a parent.
    let mut stack: Vec<(String, usize)> = Vec::new();
    for raw in text.lines() {
        if raw.trim().is_empty() {
            continue;
        }
        let leading = raw.chars().take_while(|c| c.is_whitespace()).map(|c| if c == '\t' { 2 } else { 1 }).sum::<usize>();
        let mut content = raw.trim().to_string();
        let mut heading_depth = None;
        let hashes = content.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&hashes) && content[hashes..].starts_with(' ') {
            heading_depth = Some(hashes - 1);
            content = content[hashes..].trim_start().to_string();
        }
        content = strip_bullet(&content).to_string();
        content = content.replace("**", "").replace('`', "");
        let content = content.trim();
        if content.is_empty() {
            continue;
        }
        let depth = heading_depth.unwrap_or(leading / 2 + 1);
        while stack.last().is_some_and(|(_, d)| *d >= depth) {
            stack.pop();
        }
        let id = new_id();
        let node = MindNode::new(
            id.clone(),
            content.chars().take(120).collect::<String>(),
            Some(stack.last().map(|(id, _)| id.clone()).unwrap_or_else(|| parent.to_string())),
        );
        out.push(node);
        stack.push((id, depth));
    }
    out
}

/// Removes a leading bullet or numbering marker (`- `, `* `, `+ `, `1. `, `2) `). The marker needs
/// the space: `**bold**` starts with a star and is not a bullet.
fn strip_bullet(content: &str) -> &str {
    let mut chars = content.chars();
    match chars.next() {
        Some(marker @ ('-' | '*' | '+')) => {
            let rest = content[marker.len_utf8()..].trim_start();
            // Only a marker followed by whitespace (or nothing) is a bullet.
            if rest.len() != content.len() - marker.len_utf8() { rest } else { content }
        }
        Some(first) if first.is_ascii_digit() => {
            let digits = content.chars().take_while(|c| c.is_ascii_digit()).count();
            match content[digits..].chars().next() {
                Some('.' | ')') => content[digits + 1..].trim_start(),
                _ => content,
            }
        }
        _ => content,
    }
}

/// Every node's text except the one being expanded (global de-duplication context).
fn other_texts(map: &MindMap, node: &str) -> Vec<String> {
    map.nodes.iter().filter(|candidate| candidate.id != node).map(|candidate| candidate.text.clone()).collect()
}

/// `{text, children:[...]}`: a whole tree.
#[derive(Debug, serde::Deserialize)]
struct Tree {
    #[serde(default)]
    text: String,
    #[serde(default)]
    children: Vec<Tree>,
}

/// `{items:["..."]}`.
#[derive(Debug, serde::Deserialize)]
struct Items {
    #[serde(default)]
    items: Vec<String>,
}

/// `{text:"..."}`.
#[derive(Debug, serde::Deserialize)]
struct Text {
    #[serde(default)]
    text: String,
}

/// Two levels under the node: `{children:[{text, children:[{text}]}]}`.
#[derive(Debug, serde::Deserialize)]
struct Deep {
    #[serde(default)]
    children: Vec<DeepLevel>,
}

#[derive(Debug, serde::Deserialize)]
struct DeepLevel {
    #[serde(default)]
    text: String,
    #[serde(default)]
    children: Vec<Text>,
}

fn tree_nodes(tree: &Tree) -> Vec<MindNode> {
    let mut out = Vec::new();
    fn walk(tree: &Tree, parent: Option<&str>, out: &mut Vec<MindNode>) {
        let text = tree.text.trim();
        let id = new_id();
        out.push(MindNode::new(id.clone(), if text.is_empty() { "…" } else { text }, parent.map(str::to_string)));
        for child in &tree.children {
            walk(child, Some(&id), out);
        }
    }
    walk(tree, None, &mut out);
    out
}

fn deep_nodes(deep: &Deep, parent: &str) -> Vec<MindNode> {
    let mut out = Vec::new();
    for level in &deep.children {
        let text = level.text.trim();
        let id = new_id();
        out.push(MindNode::new(id.clone(), if text.is_empty() { "…" } else { text }, Some(parent.to_string())));
        for child in &level.children {
            let text = child.text.trim();
            if text.is_empty() {
                continue;
            }
            out.push(MindNode::new(new_id(), text, Some(id.clone())));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> MindMap {
        let mut map = MindMap::new("Mapa", "Sepse");
        let root = map.root_id().map(str::to_string).unwrap_or_default();
        let a = map.add_child(&root, "Reconhecimento", None);
        map.add_child(&a, "Lactato", None);
        map
    }

    fn first_child(map: &MindMap) -> String {
        let root = map.root_id().unwrap_or_default();
        map.children(root).first().map(|n| n.id.clone()).unwrap_or_default()
    }

    #[test]
    fn a_generate_request_sends_a_system_turn_and_the_rules() {
        let map = map();
        let request = Request::GenerateMap { topic: "Sepse".into(), depth: 2, breadth: 4 };
        let messages = messages(&map, &request, "pt-br").expect("messages");
        assert_eq!(messages.len(), 2);
        match &messages[0] {
            Msg::System(text) => assert!(text.contains("português")),
            other => panic!("expected a system turn, got {other:?}"),
        }
        match &messages[1] {
            Msg::User(text) => {
                assert!(text.contains("Sepse"));
                assert!(text.contains("\"text\":\"<tema central>\""));
                assert!(text.contains("cerca de 4 ramos"));
            }
            other => panic!("expected a user turn, got {other:?}"),
        }
    }

    /// A long topic is treated as content to organize, not as a title.
    #[test]
    fn a_long_topic_asks_for_the_content_to_be_organized() {
        let map = map();
        let long = "a".repeat(300);
        let messages = messages(&map, &Request::GenerateMap { topic: long.clone(), depth: 2, breadth: 3 }, "en").expect("messages");
        match &messages[1] {
            Msg::User(text) => assert!(text.contains("Organize the CONTENT"), "{text}"),
            other => panic!("{other:?}"),
        }
    }

    /// An expand request carries the path and the map's other texts, so the model does not repeat
    /// what is already there.
    #[test]
    fn an_expand_request_carries_context_and_what_to_avoid() {
        let map = map();
        let target = first_child(&map);
        let messages = messages(&map, &Request::Expand { node: target.clone(), count: 3 }, "en").expect("messages");
        let Msg::User(text) = &messages[1] else { panic!("no user turn") };
        assert!(text.contains("Sepse > Reconhecimento"), "{text}");
        assert!(text.contains("List 3 relevant"), "{text}");
        assert!(text.contains("\"Lactato\""), "the sibling is in the avoid list: {text}");
    }

    /// A node deleted while the model works is an error, not a panic.
    #[test]
    fn a_request_for_a_missing_node_is_an_error() {
        let map = map();
        assert!(messages(&map, &Request::Explain { node: "sumiu".into() }, "en").is_err());
        assert!(messages(&map, &Request::Rephrase { node: "sumiu".into(), instruction: "x".into() }, "en").is_err());
    }

    #[test]
    fn an_empty_topic_is_refused_before_the_request() {
        let map = map();
        assert!(messages(&map, &Request::GenerateMap { topic: "   ".into(), depth: 2, breadth: 2 }, "en").is_err());
    }

    #[test]
    fn a_generated_tree_becomes_a_map() {
        let answer = r#"{"text":"Sepse","children":[{"text":"Reconhecimento","children":[{"text":"Lactato"}]},{"text":"Conduta"}]}"#;
        let outcome = interpret(&Request::GenerateMap { topic: "Sepse".into(), depth: 2, breadth: 2 }, answer).expect("interpret");
        let Outcome::NewMap { title, nodes } = outcome else { panic!("expected a new map") };
        assert_eq!(title, "Sepse");
        assert_eq!(nodes.len(), 4);
        assert!(nodes[0].parent_id.is_none(), "the first node is the root");
        assert_eq!(nodes[1].parent_id.as_deref(), Some(nodes[0].id.as_str()));
        assert_eq!(nodes[2].parent_id.as_deref(), Some(nodes[1].id.as_str()));
        assert_eq!(nodes[3].parent_id.as_deref(), Some(nodes[0].id.as_str()));
        // Ids are fresh, so nothing collides with the map the answer replaced.
        let ids: std::collections::BTreeSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids.len(), nodes.len());
    }

    #[test]
    fn an_expand_answer_becomes_children_of_the_node() {
        let map = map();
        let target = first_child(&map);
        let outcome =
            interpret(&Request::Expand { node: target.clone(), count: 2 }, r#"{"items":["Hemoculturas","  ","Gasometria"]}"#).expect("interpret");
        let Outcome::Nodes { parent, nodes } = outcome else { panic!("expected nodes") };
        assert_eq!(parent, target);
        assert_eq!(nodes.len(), 2, "the blank item is dropped");
        assert!(nodes.iter().all(|n| n.parent_id.as_deref() == Some(target.as_str())));
    }

    #[test]
    fn a_deep_expand_answer_becomes_two_levels() {
        let map = map();
        let target = first_child(&map);
        let answer = r#"{"children":[{"text":"Exames","children":[{"text":"Lactato"},{"text":"Culturas"}]},{"text":"Conduta","children":[]}]}"#;
        let outcome = interpret(&Request::DeepExpand { node: target.clone() }, answer).expect("interpret");
        let Outcome::Nodes { nodes, .. } = outcome else { panic!("expected nodes") };
        assert_eq!(nodes.len(), 4, "two first-level items and the two grandchildren of the first");
        let exams = &nodes[0];
        assert_eq!(exams.parent_id.as_deref(), Some(target.as_str()));
        assert_eq!(nodes[1].parent_id.as_deref(), Some(exams.id.as_str()));
    }

    #[test]
    fn explain_and_rephrase_answer_a_text() {
        let map = map();
        let target = first_child(&map);
        let explained = interpret(&Request::Explain { node: target.clone() }, r#"{"text":"  Uma frase curta.  "}"#).expect("explain");
        assert_eq!(explained, Outcome::Text("Uma frase curta.".into()));
        let rephrased = interpret(&Request::Rephrase { node: target, instruction: "mais curto".into() }, "{\"text\":\"Triagem\"}").expect("rephrase");
        assert_eq!(rephrased, Outcome::Text("Triagem".into()));
    }

    /// A model that answers prose instead of JSON is an error that carries the answer.
    #[test]
    fn a_non_json_answer_is_an_error() {
        let map = map();
        let target = first_child(&map);
        let error = interpret(&Request::Expand { node: target, count: 2 }, "Claro! Aqui estão os itens:").expect_err("must fail");
        assert!(error.contains("Claro"), "{error}");
    }

    #[test]
    fn an_empty_answer_is_refused_per_action() {
        let map = map();
        let target = first_child(&map);
        assert!(interpret(&Request::Expand { node: target.clone(), count: 2 }, r#"{"items":[]}"#).is_err());
        assert!(interpret(&Request::Explain { node: target }, r#"{"text":"   "}"#).is_err());
        assert!(interpret(&Request::GenerateMap { topic: "t".into(), depth: 1, breadth: 1 }, r#"{"text":"  "}"#).is_err());
    }

    #[test]
    fn an_outline_becomes_a_subtree() {
        let nodes = parse_outline("# Título\n- Um\n  - Um A\n  - Um B\n- Dois\n1. Três\n**negrito** `código`", "pai");
        let texts: Vec<&str> = nodes.iter().map(|n| n.text.as_str()).collect();
        assert_eq!(texts, ["Título", "Um", "Um A", "Um B", "Dois", "Três", "negrito código"]);
        // Depth: heading = 0 -> child of `pai`; bullets at depth 1 -> children of the heading.
        assert_eq!(nodes[0].parent_id.as_deref(), Some("pai"));
        assert_eq!(nodes[1].parent_id.as_deref(), Some(nodes[0].id.as_str()));
        assert_eq!(nodes[2].parent_id.as_deref(), Some(nodes[1].id.as_str()));
        assert_eq!(nodes[4].parent_id.as_deref(), Some(nodes[0].id.as_str()));
    }

    #[test]
    fn an_empty_outline_has_no_nodes() {
        assert!(parse_outline("   \n\n", "pai").is_empty());
    }

    /// The chat's system prompt is the chat one, not the JSON-only one.
    #[test]
    fn the_chat_prompt_is_not_the_json_one() {
        let chat = chat_system("pt-br");
        assert!(chat.contains("mapas mentais"));
        assert!(!chat.contains("SOMENTE com JSON válido"));
    }
}
