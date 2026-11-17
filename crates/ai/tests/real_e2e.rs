//! The real-model end-to-end: a real provider, a real key, so `#[ignore]`.
//!
//! Two paths go out: one action (the map plus the node it is about) and the chat with the map's
//! outline as the context (what the panel sends). A free model works with the suite's zero-credit
//! key. The key comes from `OPENROUTER_API_KEY` or from `C:\Users\Hades\Desktop\ai-test.env`
//! (never printed). Run with
//! `AI_TEST_MODEL=nvidia/nemotron-3-ultra-550b-a55b:free cargo test -p localmind-ai --test real_e2e -- --ignored --nocapture`.

use local_ai::{App, Connection, Msg};
use localmind_ai::actions::{self, Request};
use localmind_model::MindMap;

fn app() -> App {
    App { id: "LocalMind", url: "https://github.com/Anon5T4R/Local-Mind" }
}

fn connection() -> Connection {
    Connection {
        id: "real".into(),
        provider: "openrouter".into(),
        label: "OpenRouter".into(),
        base_url: String::new(),
        model: std::env::var("AI_TEST_MODEL").unwrap_or_else(|_| "nvidia/nemotron-3-ultra-550b-a55b:free".into()),
    }
}

/// The key: the environment first, then the suite's `ai-test.env` on the Desktop.
fn real_key() -> String {
    if let Some(key) = std::env::var("OPENROUTER_API_KEY").ok().filter(|key| !key.trim().is_empty()) {
        return key.trim().to_string();
    }
    let path = std::path::PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default()).join("Desktop").join("ai-test.env");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("OPENROUTER_API_KEY or {}: {error}", path.display()));
    for line in text.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("OPENROUTER_API_KEY=") {
            return value.trim().trim_matches('"').to_string();
        }
    }
    panic!("OPENROUTER_API_KEY is in neither the environment nor {}", path.display());
}

fn map() -> MindMap {
    let mut map = MindMap::new("Ata", "Sepse");
    let root = map.nodes[0].id.clone();
    let recognition = map.add_child(&root, "Reconhecimento", None);
    map.add_child(&recognition, "Lactato", None);
    map.add_child(&root, "Conduta", None);
    map
}

/// One action against a real provider: the answer is read into the map's own outcome.
#[test]
#[ignore = "network + a real provider key"]
fn a_real_model_explains_a_node() {
    let key = real_key();
    let map = map();
    let request = Request::Explain { node: map.nodes[0].id.clone() };
    let messages = actions::messages(&map, &request, "pt-br").expect("messages");
    let mut pieces = 0;
    let answer = local_ai::chat_stream_with_key(&app(), &connection(), &key, &messages, &mut |_: &str| pieces += 1).expect("a real answer");
    println!("--- explain ({pieces} pieces) ---\n{answer}\n--------------");
    assert!(pieces > 1, "the answer streamed in pieces");
    let outcome = actions::interpret(&request, &answer).expect("the answer fits the contract");
    match outcome {
        localmind_ai::Outcome::Text(text) => assert!(text.split_whitespace().count() > 5, "real content: {text}"),
        other => panic!("an explanation, not nodes: {other:?}"),
    }
}

/// The chat with the map as context: the model has to answer about the map it was given.
#[test]
#[ignore = "network + a real provider key"]
fn a_real_model_chats_about_the_map() {
    let key = real_key();
    let map = map();
    let outline = localmind_ai::map_outline(&map, localmind_ai::MAP_BUDGET);
    assert!(outline.text.contains("- Sepse"), "{outline:?}");
    let history = vec![
        Msg::System(actions::chat_system("pt-br")),
        Msg::System(format!("the map\n\n{}", outline.text)),
        Msg::User("Em uma frase, do que fala este mapa?".into()),
    ];
    let mut pieces = 0;
    let answer = local_ai::chat_stream_with_key(&app(), &connection(), &key, &history, &mut |_: &str| pieces += 1).expect("a real answer");
    println!("--- chat ({pieces} pieces) ---\n{answer}\n--------------");
    assert!(pieces > 1, "the answer streamed in pieces");
    assert!(answer.to_lowercase().contains("sepse"), "the model read the map: {answer}");
}
