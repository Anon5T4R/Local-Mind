//! The commands Local Mindmap offers an agent.
//!
//! The list is the map app's own: the palette's commands (`src/app/commands.rs`) plus `doc.inspect`,
//! the read side of the same document (what the canvas shows: the cards and their ids). Nothing here
//! is a new capability; commands that need the window say `appOnly` and the headless session refuses
//! them, pointing at `--control`/`--connect`.

use serde_json::{Map, Value, json};

/// One command of the app.
pub struct Spec {
    /// The id the palette and the control channel use.
    pub id: &'static str,
    /// The English label the palette shows (the i18n key).
    pub label: &'static str,
    /// The menu it is listed under.
    pub menu: &'static str,
    /// The window is needed: the viewport, a dialog, the AI panel, the updater.
    pub app_only: bool,
}

/// Every command, in the palette's order.
pub const SPECS: &[Spec] = &[
    Spec { id: "file.new", label: "New map", menu: "File", app_only: false },
    Spec { id: "file.open", label: "Open…", menu: "File", app_only: false },
    Spec { id: "file.save", label: "Save", menu: "File", app_only: false },
    Spec { id: "file.save_as", label: "Save As…", menu: "File", app_only: false },
    Spec { id: "file.export.0", label: "Export: Markdown", menu: "File", app_only: false },
    Spec { id: "file.export.1", label: "Export: HTML", menu: "File", app_only: false },
    Spec { id: "file.export.2", label: "Export: SVG", menu: "File", app_only: false },
    Spec { id: "file.export.3", label: "Export: PNG", menu: "File", app_only: false },
    Spec { id: "doc.inspect", label: "Inspect the map", menu: "File", app_only: false },
    Spec { id: "file.exit", label: "Exit", menu: "File", app_only: true },
    Spec { id: "edit.undo", label: "Undo", menu: "Edit", app_only: false },
    Spec { id: "edit.redo", label: "Redo", menu: "Edit", app_only: false },
    Spec { id: "edit.text", label: "Edit text", menu: "Edit", app_only: false },
    Spec { id: "edit.child", label: "New child", menu: "Edit", app_only: false },
    Spec { id: "edit.sibling", label: "New sibling", menu: "Edit", app_only: false },
    Spec { id: "edit.note", label: "Note…", menu: "Edit", app_only: false },
    Spec { id: "edit.move", label: "Move to…", menu: "Edit", app_only: false },
    Spec { id: "edit.delete", label: "Delete node", menu: "Edit", app_only: false },
    Spec { id: "edit.settings", label: "Settings…", menu: "Edit", app_only: true },
    Spec { id: "view.fit", label: "Fit to window", menu: "View", app_only: true },
    Spec { id: "view.zoom_in", label: "Zoom in", menu: "View", app_only: true },
    Spec { id: "view.zoom_out", label: "Zoom out", menu: "View", app_only: true },
    Spec { id: "view.theme.system", label: "Theme: System", menu: "View", app_only: true },
    Spec { id: "view.theme.light", label: "Theme: Light", menu: "View", app_only: true },
    Spec { id: "view.theme.dark", label: "Theme: Dark", menu: "View", app_only: true },
    Spec { id: "ai.generate", label: "Generate map…", menu: "AI", app_only: true },
    Spec { id: "ai.expand", label: "Expand", menu: "AI", app_only: true },
    Spec { id: "ai.expand_deep", label: "Expand deep", menu: "AI", app_only: true },
    Spec { id: "ai.explain", label: "Explain", menu: "AI", app_only: true },
    Spec { id: "ai.rewrite", label: "Rewrite…", menu: "AI", app_only: true },
    Spec { id: "ai.chat", label: "Chat", menu: "AI", app_only: true },
    Spec { id: "ai.connections", label: "Connections…", menu: "AI", app_only: true },
    Spec { id: "help.keys", label: "Keyboard and mouse", menu: "Help", app_only: true },
    Spec { id: "help.updates", label: "Check for Updates…", menu: "Help", app_only: true },
    Spec { id: "help.about", label: "About Local Mindmap", menu: "Help", app_only: true },
];

/// The spec of `id`, if the app has that command.
pub fn spec(id: &str) -> Option<&'static Spec> {
    SPECS.iter().find(|s| s.id == id)
}

/// Does the app have the command `id`?
pub fn has(id: &str) -> bool {
    spec(id).is_some()
}

/// The parameters of a command as a JSON Schema.
pub fn params(id: &str) -> Value {
    let text = |what: &str| json!({"type": "string", "description": what});
    let only = |props: Value| json!({"type": "object", "properties": props, "additionalProperties": false});
    let required = |mut schema: Value, keys: &[&str]| {
        schema["required"] = json!(keys);
        schema
    };
    let side = json!({"type": "string", "enum": ["left", "right", "up", "down"], "description": "Direction from the root (only meaningful for the root's children)"});
    match id {
        "file.new" => only(json!({
            "title": text("Title of the map (default: the app's untitled name)"),
            "root": text("Text of the central idea (default: the app's)"),
        })),
        "file.open" => required(only(json!({"path": text("The .tmind map to open")})), &["path"]),
        "file.save" => only(json!({"path": text("Where to save (default: the map's own file; a map never saved needs one)")})),
        "file.save_as" => required(only(json!({"path": text("Where to save the map; that file becomes the map's")})), &["path"]),
        "file.export.0" | "file.export.1" | "file.export.2" | "file.export.3" => {
            required(only(json!({"path": text("File to write the export to (the format is the command's)")})), &["path"])
        }
        "doc.inspect" => only(json!({"limit": {"type": "integer", "minimum": 0, "description": "Most nodes to list (default 500; 0 lists all)"}})),
        "edit.child" => only(json!({
            "parent": text("Node to add the child under (default: the root)"),
            "text": text("Text of the new node (default: the app's new-node text)"),
        })),
        "edit.sibling" => required(
            only(
                json!({"id": text("Existing node to add a sibling after"), "text": text("Text of the new node (default: the app's new-node text)")}),
            ),
            &["id"],
        ),
        "edit.text" => required(only(json!({"id": text("Node to rename"), "text": text("The new text (not empty)")})), &["id", "text"]),
        "edit.note" => required(only(json!({"id": text("Node"), "note": text("The note; leave it out to remove the note")})), &["id"]),
        "edit.move" => required(only(json!({"id": text("Node to move"), "parent": text("New parent"), "side": side})), &["id", "parent"]),
        "edit.delete" => required(only(json!({"id": text("Node to remove, with its subtree")})), &["id"]),
        _ => only(json!({})),
    }
}

/// The catalogue as `command_list` and `commands` return it, in palette order.
pub fn list() -> Vec<Value> {
    SPECS.iter().map(entry).collect()
}

fn entry(s: &'static Spec) -> Value {
    json!({"id": s.id, "label": s.label, "menu": s.menu, "appOnly": s.app_only, "params": params(s.id)})
}

/// The catalogue kept by `filter` (case-insensitive id, label or menu substring), and only the
/// ids and labels when `brief`. An empty `filter` keeps everything.
pub fn search(filter: Option<&str>, brief: bool) -> Vec<Value> {
    let needle = filter.map(str::to_lowercase).filter(|n| !n.is_empty());
    list()
        .into_iter()
        .filter(|c| match &needle {
            None => true,
            Some(n) => ["id", "label", "menu"].iter().any(|k| c.get(*k).and_then(Value::as_str).is_some_and(|v| v.to_lowercase().contains(n))),
        })
        .map(|c| {
            if brief {
                match c {
                    Value::Object(mut o) => {
                        o.retain(|k, _| k == "id" || k == "label");
                        Value::Object(o)
                    }
                    other => other,
                }
            } else {
                c
            }
        })
        .collect()
}

/// The parameters of one command as an object, refusing a key the command does not read.
pub fn arguments(id: &str, params: &Value) -> Result<Map<String, Value>, String> {
    let map = match params {
        Value::Null => Map::new(),
        Value::Object(m) => m.clone(),
        _ => return Err(format!("`{id}`: params must be a JSON object")),
    };
    let schema = crate::automation::catalogue::params(id);
    let allowed: Vec<&str> = schema
        .as_object()
        .and_then(|o| o.get("properties"))
        .and_then(Value::as_object)
        .map(|p| p.keys().map(String::as_str).collect())
        .unwrap_or_default();
    if let Some(key) = map.keys().find(|k| !allowed.contains(&k.as_str())) {
        let expected = if allowed.is_empty() { "it takes no parameters".to_string() } else { format!("its parameters are {}", allowed.join(", ")) };
        return Err(format!("`{id}`: unknown parameter `{key}`: {expected}"));
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_palette_command_is_listed_once() {
        let mut ids: Vec<&str> = SPECS.iter().map(|s| s.id).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "a command id is listed twice");
        assert!(has("edit.child") && has("file.export.3") && !has("edit.attach"));
        assert!(spec("ai.chat").is_some_and(|s| s.app_only) && !spec("edit.delete").is_some_and(|s| s.app_only));
    }

    #[test]
    fn search_filters_by_id_label_or_menu_and_brief_keeps_the_names() {
        assert_eq!(search(None, false).len(), SPECS.len());
        assert_eq!(search(Some("THEME"), false).len(), 3);
        let file = search(Some("File"), false);
        assert!(file.iter().all(|c| c["menu"] == "File"));
        assert_eq!(search(Some("delete"), true), vec![json!({"id": "edit.delete", "label": "Delete node"})]);
        assert!(search(Some("zzz"), false).is_empty());
        assert_eq!(search(Some(""), false).len(), SPECS.len(), "an empty filter keeps everything");
    }

    #[test]
    fn parameters_are_checked_against_the_schema() {
        assert_eq!(arguments("file.open", &json!({"path": "a.tmind"})).expect("a known key").len(), 1);
        let e = arguments("file.open", &json!({"file": "a.tmind"})).expect_err("a typo is refused");
        assert!(e.contains("unknown parameter `file`") && e.contains("path"), "{e}");
        let e = arguments("edit.undo", &json!({"by": 1})).expect_err("it takes none");
        assert!(e.contains("no parameters"), "{e}");
        assert!(arguments("edit.undo", &Value::Null).is_ok());
        assert!(arguments("edit.undo", &json!([1])).is_err());
    }
}
