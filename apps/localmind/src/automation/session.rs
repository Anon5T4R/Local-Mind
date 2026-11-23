//! Running Local Mind's commands without a window: one map open, edited through the editor the app
//! already uses.
//!
//! The same functions serve the headless session ([`Session`], behind `localmind mcp`,
//! `run --script` and `commands`) and the running window's control channel (`--control PORT`): the
//! app passes its own `Editor`, so `mcp --connect` and the headless server edit a map through the
//! same code the menu does — undo history, selection and the `.tmind` writer included.
//!
//! **Deny by default.** With `--automation-read-root`/`--automation-write-root`, every file a
//! command opens, saves or exports goes through [`craft_mcp::automation::Roots`]: a map outside the
//! read roots cannot be opened, a destination outside the write roots cannot be written. No roots
//! at all means no confinement (the user's own app).

use std::path::{Path, PathBuf};

use craft_mcp::automation::{Access, Roots};
use localmind_model::Side;
use serde_json::{Value, json};

use super::catalogue;
use crate::doc::Document;
use crate::edit::{self, Editor, ExportKind};

/// Most nodes `doc.inspect` lists unless asked for more (0 lists all).
pub const INSPECT_DEFAULT: u64 = 500;

/// A headless session: one editor (the map, its history and the theme the exports use), the
/// automation roots and the commands on top of them.
pub struct Session {
    editor: Editor,
    roots: Option<Roots>,
}

impl Session {
    pub fn new(roots: Option<Roots>) -> Self {
        Self { editor: Editor::new(edit::new_document()), roots }
    }

    /// Where this session's calls go, for the MCP server's instructions.
    pub fn describe(&self) -> String {
        match &self.roots {
            None => "headless".to_string(),
            Some(roots) => format!("headless, files confined: {}", roots.describe()),
        }
    }

    /// One control-channel method, as the running app answers them: `command.execute`,
    /// `command.list` and `app.info`.
    pub fn call(&mut self, method: &str, params: &Value) -> Result<Value, String> {
        match method {
            "command.execute" => {
                let command = params.get("command").and_then(Value::as_str).ok_or("command.execute: missing string `command`")?;
                let args = params.get("params").cloned().unwrap_or(Value::Null);
                self.exec(command, &args)
            }
            "command.list" => {
                let filter = params.get("filter").and_then(Value::as_str);
                let brief = params.get("brief").and_then(Value::as_bool).unwrap_or(false);
                Ok(Value::Array(catalogue::search(filter, brief)))
            }
            "app.info" => Ok(json!({
                "id": "localmind",
                "name": "Local Mind",
                "version": env!("CARGO_PKG_VERSION"),
                "state": state(&self.editor),
            })),
            other => Err(format!("unknown method `{other}`: this app answers command.execute, command.list and app.info")),
        }
    }

    /// Run one command by id.
    pub fn exec(&mut self, id: &str, params: &Value) -> Result<Value, String> {
        let Some(spec) = catalogue::spec(id) else {
            return Err(format!("unknown command `{id}`: command_list lists what this app has"));
        };
        if spec.app_only {
            return Err(format!(
                "`{id}` runs in the window: start the app with `localmind --control PORT` and use `mcp --connect PORT`, or run it from the palette"
            ));
        }
        let args = catalogue::arguments(id, params)?;
        let text =
            |key: &str| -> Result<&str, String> { args.get(key).and_then(Value::as_str).ok_or_else(|| format!("`{id}`: `{key}` must be a string")) };
        let optional = |key: &str| args.get(key).and_then(Value::as_str);
        let editor = &mut self.editor;
        match id {
            "file.new" => new_map(editor, optional("title"), optional("root")),
            "file.open" => open(editor, self.roots.as_ref(), text("path")?),
            "file.save" => save(editor, self.roots.as_ref(), optional("path")),
            "file.save_as" => save(editor, self.roots.as_ref(), Some(text("path")?)),
            "file.export.0" => export(editor, self.roots.as_ref(), ExportKind::Markdown, text("path")?),
            "file.export.1" => export(editor, self.roots.as_ref(), ExportKind::Html, text("path")?),
            "file.export.2" => export(editor, self.roots.as_ref(), ExportKind::Svg, text("path")?),
            "file.export.3" => export(editor, self.roots.as_ref(), ExportKind::Png, text("path")?),
            "doc.inspect" => {
                let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(INSPECT_DEFAULT);
                Ok(inspect(editor, limit))
            }
            "edit.undo" => Ok(undo(editor)),
            "edit.redo" => Ok(redo(editor)),
            "edit.child" => add_child(editor, optional("parent"), optional("text")),
            "edit.sibling" => add_sibling(editor, text("id")?, optional("text")),
            "edit.text" => rename(editor, text("id")?, text("text")?),
            "edit.note" => set_note(editor, text("id")?, optional("note")),
            "edit.move" => move_node(editor, text("id")?, text("parent")?, optional("side")),
            "edit.delete" => delete(editor, text("id")?),
            // Every other command is `appOnly` and was refused above; a new one lands here until
            // its arm exists, which the "every command runs somewhere" test catches.
            _ => Err(format!("`{id}` has no headless implementation")),
        }
    }
}

/// The map as replies describe it: its title, its file, whether it has unsaved edits, and the undo
/// history.
pub fn state(editor: &Editor) -> Value {
    let doc = &editor.doc;
    json!({
        "title": doc.map.title,
        "path": doc.path.as_deref().map(display),
        "dirty": doc.dirty,
        "count": doc.map.nodes.len(),
        "root": doc.root_id(),
        "canUndo": editor.can_undo(),
        "canRedo": editor.can_redo(),
    })
}

/// Start a new, empty map (`file.new`).
pub fn new_map(editor: &mut Editor, title: Option<&str>, root: Option<&str>) -> Result<Value, String> {
    let mut doc = edit::new_document();
    if let Some(title) = title {
        doc.map.title = title.to_string();
    }
    if let Some(root) = root {
        let id = doc.root_id().ok_or("the new map has no root")?;
        doc.map.set_text(&id, root);
    }
    editor.replace(doc);
    Ok(json!({"state": state(editor)}))
}

/// Read a map from disk (`file.open`), which becomes the map being edited.
pub fn open(editor: &mut Editor, roots: Option<&Roots>, path: &str) -> Result<Value, String> {
    let asked = clean(path);
    check(roots, &asked, Access::Read)?;
    let doc = Document::open(Path::new(&asked)).map_err(|e| format!("{asked}: {e}"))?;
    editor.replace(doc);
    Ok(json!({"state": state(editor)}))
}

/// Write the map (`file.save` / `file.save_as`): to `path`, or to the file it came from.
pub fn save(editor: &mut Editor, roots: Option<&Roots>, path: Option<&str>) -> Result<Value, String> {
    let target: PathBuf = match path {
        Some(path) => PathBuf::from(clean(path)),
        None => editor.doc.path.clone().ok_or("the map has no file yet: give `path` (Save As asks for one, too)")?,
    };
    let shown = display(&target);
    check(roots, &shown, Access::Write)?;
    editor.doc.save(&target).map_err(|e| format!("{shown}: {e}"))?;
    editor.mark_saved();
    Ok(json!({"path": shown, "state": state(editor)}))
}

/// Write the map in `kind` to `path` (`file.export.*`), atomically, as a map is written.
pub fn export(editor: &Editor, roots: Option<&Roots>, kind: ExportKind, path: &str) -> Result<Value, String> {
    let asked = clean(path);
    check(roots, &asked, Access::Write)?;
    let bytes = editor.export(kind).map_err(|e| format!("the map could not be exported: {e}"))?;
    localmind_io::save_bytes(&bytes, Path::new(&asked)).map_err(|e| format!("{asked}: {e}"))?;
    Ok(json!({"path": asked, "format": kind.extension(), "bytes": bytes.len()}))
}

/// The map's nodes, in file order, with the ids the edit commands take (`doc.inspect`). `limit` is
/// the most to list (0 lists all).
pub fn inspect(editor: &Editor, limit: u64) -> Value {
    let map = &editor.doc.map;
    let most = usize::try_from(limit).unwrap_or(usize::MAX);
    let taken = if most == 0 { map.nodes.len() } else { most.min(map.nodes.len()) };
    let nodes: Vec<Value> = map
        .nodes
        .iter()
        .take(taken)
        .map(|n| {
            let mut node = json!({
                "id": n.id,
                "text": n.text,
                "parentId": n.parent_id,
                "children": map.children(&n.id).len(),
            });
            if let Some(side) = n.side {
                node["side"] = json!(side_id(side));
            }
            if n.collapsed == Some(true) {
                node["collapsed"] = json!(true);
            }
            if let Some(note) = &n.note {
                node["note"] = json!(note);
            }
            if let Some(color) = &n.color {
                node["color"] = json!(color);
            }
            node
        })
        .collect();
    let mut out = state(editor);
    out["truncated"] = json!(taken < map.nodes.len());
    out["nodes"] = Value::Array(nodes);
    out
}

/// Add a child under `parent` (default: the root) and return the new node (`edit.child`).
pub fn add_child(editor: &mut Editor, parent: Option<&str>, text: Option<&str>) -> Result<Value, String> {
    let parent = match parent {
        Some(parent) => parent.to_string(),
        None => editor.doc.root_id().ok_or("the map has no root")?,
    };
    if editor.doc.map.node(&parent).is_none() {
        return Err(format!("no node `{parent}`: doc.inspect lists the ids"));
    }
    let before = editor.doc.map.nodes.len();
    editor.add_child(&parent);
    let created = editor.selected.clone().filter(|id| editor.doc.map.node(id).is_some() && editor.doc.map.nodes.len() > before);
    let Some(created) = created else { return Err(format!("the child under `{parent}` could not be added")) };
    if let Some(text) = text {
        rename(editor, &created, text)?;
    }
    Ok(json!({"id": created, "state": state(editor)}))
}

/// Add a sibling after `id` and return the new node (`edit.sibling`).
pub fn add_sibling(editor: &mut Editor, id: &str, text: Option<&str>) -> Result<Value, String> {
    let node = editor.doc.map.node(id).ok_or_else(|| format!("no node `{id}`: doc.inspect lists the ids"))?;
    if node.parent_id.is_none() {
        return Err(format!("`{id}` is the root: it has no siblings"));
    }
    let before = editor.doc.map.nodes.len();
    editor.add_sibling(id);
    let created = editor.selected.clone().filter(|id| editor.doc.map.node(id).is_some() && editor.doc.map.nodes.len() > before);
    let Some(created) = created else { return Err(format!("the sibling of `{id}` could not be added")) };
    if let Some(text) = text {
        rename(editor, &created, text)?;
    }
    Ok(json!({"id": created, "state": state(editor)}))
}

/// Set a node's text (`edit.text`), through the inline editor's own commit: trimmed, and one undo
/// step. An empty text is refused, where the inline field would just cancel the rename.
pub fn rename(editor: &mut Editor, id: &str, text: &str) -> Result<Value, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("`text` is empty: a rename needs text".into());
    }
    if editor.doc.map.node(id).is_none() {
        return Err(format!("no node `{id}`: doc.inspect lists the ids"));
    }
    editor.begin_edit(id);
    if let Some(editing) = editor.editing.as_mut() {
        editing.draft = text.to_string();
    }
    editor.commit_edit();
    let now = editor.doc.map.node(id).map(|n| n.text.clone()).unwrap_or_default();
    Ok(json!({"id": id, "text": now, "state": state(editor)}))
}

/// Set or remove a node's note (`edit.note`); an empty or absent note removes it.
pub fn set_note(editor: &mut Editor, id: &str, note: Option<&str>) -> Result<Value, String> {
    if editor.doc.map.node(id).is_none() {
        return Err(format!("no node `{id}`: doc.inspect lists the ids"));
    }
    let note = note.map(str::trim).filter(|n| !n.is_empty()).map(str::to_string);
    editor.set_note(id, note.clone());
    Ok(json!({"id": id, "note": note, "state": state(editor)}))
}

/// Re-parent a node (`edit.move`), as "Move to…" does.
pub fn move_node(editor: &mut Editor, id: &str, parent: &str, side: Option<&str>) -> Result<Value, String> {
    let side = match side {
        None => None,
        Some(name) => Some(side_from_id(name)?),
    };
    editor.move_node_to(id, parent, side);
    let moved = editor.doc.map.node(id).is_some_and(|n| n.parent_id.as_deref() == Some(parent));
    if !moved {
        return Err(format!("`{id}` was not moved under `{parent}`: both must exist, and a node cannot move into its own subtree"));
    }
    Ok(json!({"id": id, "parent": parent, "state": state(editor)}))
}

/// Remove a node and its subtree (`edit.delete`).
pub fn delete(editor: &mut Editor, id: &str) -> Result<Value, String> {
    if editor.doc.map.node(id).is_none() {
        return Err(format!("no node `{id}`: doc.inspect lists the ids"));
    }
    if editor.doc.map.node(id).is_some_and(|n| n.parent_id.is_none()) {
        return Err("the root cannot be deleted: a map needs one".into());
    }
    editor.delete(id);
    if editor.doc.map.node(id).is_some() {
        return Err(format!("`{id}` was not deleted"));
    }
    Ok(json!({"id": id, "state": state(editor)}))
}

/// Undo one step (`edit.undo`).
pub fn undo(editor: &mut Editor) -> Value {
    editor.undo();
    json!({"state": state(editor)})
}

/// Redo one step (`edit.redo`).
pub fn redo(editor: &mut Editor) -> Value {
    editor.redo();
    json!({"state": state(editor)})
}

/// A side as the model names it.
fn side_id(side: Side) -> &'static str {
    match side {
        Side::Left => "left",
        Side::Right => "right",
        Side::Up => "up",
        Side::Down => "down",
    }
}

fn side_from_id(id: &str) -> Result<Side, String> {
    match id {
        "left" => Ok(Side::Left),
        "right" => Ok(Side::Right),
        "up" => Ok(Side::Up),
        "down" => Ok(Side::Down),
        other => Err(format!("`side` is {other}: it is left, right, up or down")),
    }
}

/// `path` as given, cleaned of the quotes a "Copy as path" paste brings.
fn clean(path: &str) -> String {
    let text = path.trim();
    text.strip_prefix('"').and_then(|r| r.strip_suffix('"')).unwrap_or(text).trim().to_string()
}

/// Refuse `path` when it is outside its kind's automation roots (no roots: nothing is confined).
fn check(roots: Option<&Roots>, path: &str, access: Access) -> Result<(), String> {
    if path.is_empty() {
        return Err("empty path".into());
    }
    match roots {
        None => Ok(()),
        Some(roots) => roots.check(path, access),
    }
}

/// A path as replies and messages show it, without Windows' `\\?\` prefix.
fn display(path: &Path) -> String {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => rest.to_string(),
        _ => text.into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("localmind-auto-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    fn exec(s: &mut Session, id: &str, params: Value) -> Result<Value, String> {
        s.exec(id, &params)
    }

    #[test]
    fn a_new_map_grows_edits_and_undoes_through_the_editor() {
        let mut s = Session::new(None);
        let created = exec(&mut s, "file.new", json!({"title": "Plano", "root": "Central"})).unwrap();
        let root = created["state"]["root"].as_str().unwrap_or_default().to_string();
        assert_eq!(created["state"]["title"], "Plano");
        assert_eq!(created["state"]["count"], 1);

        let child = exec(&mut s, "edit.child", json!({"text": "Primeiro"})).unwrap();
        let child_id = child["id"].as_str().unwrap_or_default().to_string();
        let inspected = exec(&mut s, "doc.inspect", json!({})).unwrap();
        assert_eq!(inspected["count"], 2);
        assert_eq!(inspected["nodes"][1]["text"], "Primeiro");
        assert_eq!(inspected["nodes"][1]["parentId"], json!(root));

        exec(&mut s, "edit.text", json!({"id": child_id, "text": "  Renomeado  "})).unwrap();
        let inspected = exec(&mut s, "doc.inspect", json!({})).unwrap();
        assert_eq!(inspected["nodes"][1]["text"], "Renomeado", "the inline commit trims");
        exec(&mut s, "edit.note", json!({"id": child_id, "note": "uma nota"})).unwrap();
        assert_eq!(exec(&mut s, "doc.inspect", json!({})).expect("inspect")["nodes"][1]["note"], "uma nota");
        exec(&mut s, "edit.note", json!({"id": child_id})).unwrap();
        assert!(exec(&mut s, "doc.inspect", json!({})).expect("inspect")["nodes"][1].get("note").is_none());

        exec(&mut s, "edit.sibling", json!({"id": child_id, "text": "Irmão"})).unwrap();
        assert_eq!(exec(&mut s, "doc.inspect", json!({})).expect("inspect")["count"], 3);
        // The sibling and its text were two steps: undo the rename, then the node.
        exec(&mut s, "edit.undo", json!({})).unwrap();
        assert_eq!(exec(&mut s, "doc.inspect", json!({})).expect("inspect")["count"], 3);
        exec(&mut s, "edit.undo", json!({})).unwrap();
        assert_eq!(exec(&mut s, "doc.inspect", json!({})).expect("inspect")["count"], 2);
        exec(&mut s, "edit.redo", json!({})).unwrap();
        assert_eq!(exec(&mut s, "doc.inspect", json!({})).expect("inspect")["count"], 3);
        // The root cannot be deleted or cloned into its own subtree.
        assert!(exec(&mut s, "edit.delete", json!({"id": root})).unwrap_err().contains("root cannot be deleted"));
        assert!(exec(&mut s, "edit.move", json!({"id": root, "parent": child_id})).is_err());
        assert!(exec(&mut s, "edit.text", json!({"id": child_id, "text": "  "})).unwrap_err().contains("empty"));
        assert!(exec(&mut s, "edit.child", json!({"parent": "nope"})).unwrap_err().contains("no node"));
    }

    #[test]
    fn a_map_round_trips_through_the_file_and_exports() {
        let dir = scratch("files");
        let map = dir.join("mapa.tmind");
        let mut s = Session::new(None);
        exec(&mut s, "file.new", json!({"title": "Mapa", "root": "Raiz"})).unwrap();
        exec(&mut s, "edit.child", json!({"text": "Ramo"})).unwrap();
        assert!(exec(&mut s, "file.save", json!({})).unwrap_err().contains("no file yet"));
        let saved = exec(&mut s, "file.save_as", json!({"path": map.to_string_lossy()})).unwrap();
        assert_eq!(saved["state"]["dirty"], false);
        assert!(map.is_file());

        let mut other = Session::new(None);
        let opened = exec(&mut other, "file.open", json!({"path": map.to_string_lossy()})).unwrap();
        assert_eq!(opened["state"]["title"], "Mapa");
        assert_eq!(opened["state"]["count"], 2);
        let inspected = exec(&mut other, "doc.inspect", json!({"limit": 1})).unwrap();
        assert_eq!(inspected["truncated"], true);
        assert_eq!(inspected["nodes"].as_array().map(Vec::len), Some(1));

        for (index, ext) in [(0, "md"), (1, "html"), (2, "svg"), (3, "png")] {
            let out = dir.join(format!("mapa.{ext}"));
            let made = exec(&mut other, &format!("file.export.{index}"), json!({"path": out.to_string_lossy()})).unwrap();
            assert_eq!(made["format"], ext);
            let size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
            assert!(size > 0, "{ext} is empty");
        }
        // Saving the open map without a path writes its own file again.
        exec(&mut other, "edit.text", json!({"id": opened["state"]["root"].as_str().unwrap_or_default(), "text": "Outra raiz"})).unwrap();
        assert_eq!(exec(&mut other, "file.save", json!({})).expect("save")["state"]["dirty"], false);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_path_outside_the_roots_is_refused() {
        let inside = scratch("inside");
        let outside = scratch("outside");
        let map = inside.join("mapa.tmind");
        let mut writer = Session::new(None);
        exec(&mut writer, "file.new", json!({})).unwrap();
        exec(&mut writer, "file.save_as", json!({"path": map.to_string_lossy()})).unwrap();

        let roots = Roots::new(&[inside.as_path()], &[inside.as_path()]).unwrap().unwrap();
        let mut s = Session::new(Some(roots));
        let e = exec(&mut s, "file.open", json!({"path": outside.join("x.tmind").to_string_lossy()})).unwrap_err();
        assert!(e.contains("automation path rejected") && e.contains("outside the read root"), "{e}");
        exec(&mut s, "file.open", json!({"path": map.to_string_lossy()})).unwrap();
        let e = exec(&mut s, "file.save_as", json!({"path": outside.join("y.tmind").to_string_lossy()})).unwrap_err();
        assert!(e.contains("outside the write root"), "{e}");
        let e = exec(&mut s, "file.export.0", json!({"path": outside.join("y.md").to_string_lossy()})).unwrap_err();
        assert!(e.contains("outside the write root"), "{e}");
        assert!(!outside.join("y.md").exists());
        exec(&mut s, "file.export.0", json!({"path": inside.join("ok.md").to_string_lossy()})).unwrap();
        assert!(inside.join("ok.md").is_file());
        std::fs::remove_dir_all(inside).ok();
        std::fs::remove_dir_all(outside).ok();
    }

    #[test]
    fn window_only_commands_say_where_they_run() {
        let mut s = Session::new(None);
        for id in ["view.fit", "ai.chat", "help.about", "file.exit"] {
            let e = exec(&mut s, id, json!({})).unwrap_err();
            assert!(e.contains("runs in the window") && e.contains("--control"), "{id}: {e}");
        }
        assert!(exec(&mut s, "edit.attach", json!({})).is_err(), "an unknown id is refused");
        assert!(exec(&mut s, "edit.undo", json!({"steps": 1})).unwrap_err().contains("unknown parameter `steps`"));
    }

    #[test]
    fn every_command_runs_somewhere() {
        let mut s = Session::new(None);
        for spec in catalogue::SPECS {
            let result = exec(&mut s, spec.id, json!({}));
            if spec.app_only {
                assert!(result.as_ref().err().is_some_and(|e| e.contains("runs in the window")), "{}", spec.id);
            } else {
                assert!(!result.as_ref().err().is_some_and(|e| e.contains("no headless implementation")), "{}: {result:?}", spec.id);
            }
        }
    }

    #[test]
    fn the_control_methods_answer_command_list_and_app_info() {
        let mut s = Session::new(None);
        assert_eq!(s.call("command.list", &json!({"filter": "Edit"})).unwrap().as_array().map(Vec::len), Some(9));
        let info = s.call("app.info", &json!({})).unwrap();
        assert_eq!(info["id"], "localmind");
        assert!(s.call("engine.execute", &json!({})).unwrap_err().contains("unknown method"));
    }
}
