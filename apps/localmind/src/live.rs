//! The window's side of the control channel (`localmind --control PORT`): the same commands the
//! headless session runs, executed on the map the window has open, so an agent's `edit.child` adds
//! a card the user sees.
//!
//! Each frame [`LocalApp::pump_control`] answers what arrived. The commands are the shared ones
//! (`automation::session`) over the app's own editor; the window's effects follow from that (the
//! canvas re-lays the map out every frame and the title follows the dirty flag).

use craft_mcp::automation::Roots;
use serde_json::{Map, Value, json};

use crate::app::LocalApp;
use crate::automation::catalogue;
use crate::automation::session;
use crate::edit::{Editor, ExportKind};

impl LocalApp {
    /// Answer the control channel's pending requests with this frame's state, if the app was
    /// started with `--control`.
    pub(crate) fn pump_control(&mut self, ctx: &egui::Context) {
        // Taken out so the dispatch can borrow the whole app; the channel goes back afterwards.
        let Some(mut control) = self.control.take() else { return };
        control.pump(|method, params| self.live_call(ctx, method, params));
        self.control = Some(control);
        // A request may have arrived while the window was idle: keep answering while the channel
        // is open (it exists only in the `--control` mode).
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }

    /// One control-channel method of the running app.
    fn live_call(&mut self, ctx: &egui::Context, method: &str, params: &Value) -> Result<Value, String> {
        match method {
            "command.execute" => {
                let id = params.get("command").and_then(Value::as_str).ok_or("command.execute: missing string `command`")?;
                let args = params.get("params").cloned().unwrap_or(Value::Null);
                self.live_exec(ctx, id, &args)
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
                "backend": "window",
                "state": session::state(&self.editor),
            })),
            other => Err(format!("unknown method `{other}`: this app answers command.execute, command.list and app.info")),
        }
    }

    /// Run one command on the open map, then apply the window's effects.
    fn live_exec(&mut self, ctx: &egui::Context, id: &str, params: &Value) -> Result<Value, String> {
        if !catalogue::has(id) {
            return Err(format!("unknown command `{id}`: command_list lists what this app has"));
        }
        let args = catalogue::arguments(id, params)?;
        let outcome = match id {
            "file.new" => self.map(|editor, _| session::new_map(editor, opt(&args, "title"), opt(&args, "root"))),
            "file.open" => {
                let path = arg(&args, id, "path")?;
                self.map(|editor, roots| session::open(editor, roots, path))
            }
            "file.save" => self.map(|editor, roots| session::save(editor, roots, opt(&args, "path"))),
            "file.save_as" => {
                let path = arg(&args, id, "path")?;
                self.map(|editor, roots| session::save(editor, roots, Some(path)))
            }
            "file.export.0" | "file.export.1" | "file.export.2" | "file.export.3" => {
                let Some(kind) = export_kind(id) else { return Err(format!("`{id}` has no export format")) };
                let path = arg(&args, id, "path")?;
                self.map(|editor, roots| session::export(editor, roots, kind, path))
            }
            "doc.inspect" => {
                let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(session::INSPECT_DEFAULT);
                Ok(session::inspect(&self.editor, limit))
            }
            "edit.undo" => self.map(|editor, _| Ok(session::undo(editor))),
            "edit.redo" => self.map(|editor, _| Ok(session::redo(editor))),
            "edit.child" => self.map(|editor, _| session::add_child(editor, opt(&args, "parent"), opt(&args, "text"))),
            "edit.sibling" => {
                let node = arg(&args, id, "id")?;
                self.map(|editor, _| session::add_sibling(editor, node, opt(&args, "text")))
            }
            "edit.text" => {
                let (node, text) = (arg(&args, id, "id")?, arg(&args, id, "text")?);
                self.map(|editor, _| session::rename(editor, node, text))
            }
            "edit.note" => {
                let node = arg(&args, id, "id")?;
                self.map(|editor, _| session::set_note(editor, node, opt(&args, "note")))
            }
            "edit.move" => {
                let (node, parent) = (arg(&args, id, "id")?, arg(&args, id, "parent")?);
                self.map(|editor, _| session::move_node(editor, node, parent, opt(&args, "side")))
            }
            "edit.delete" => {
                let node = arg(&args, id, "id")?;
                self.map(|editor, _| session::delete(editor, node))
            }
            _ => {
                // What is left needs the window: the viewport, the dialogs, the AI panel, the
                // updater, exit. The palette's own dispatch, so the behaviour cannot drift.
                self.run_command(ctx, id);
                Ok(json!({"command": id, "state": session::state(&self.editor)}))
            }
        };
        // A load or a save clears the error line, as the app's own file paths do.
        if outcome.is_ok() {
            self.error = None;
        }
        outcome
    }

    /// Run `f` on the app's editor with the automation roots in hand: the fields are borrowed apart
    /// so both are reachable.
    fn map<R>(&mut self, f: impl FnOnce(&mut Editor, Option<&Roots>) -> R) -> R {
        let Self { editor, roots, .. } = self;
        f(editor, roots.as_ref())
    }
}

/// A required string parameter.
fn arg<'a>(args: &'a Map<String, Value>, id: &str, key: &str) -> Result<&'a str, String> {
    args.get(key).and_then(Value::as_str).ok_or_else(|| format!("`{id}`: `{key}` must be a string"))
}

/// An optional string parameter.
fn opt<'a>(args: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

/// The export format of `file.export.N`.
fn export_kind(id: &str) -> Option<ExportKind> {
    let index: usize = id.strip_prefix("file.export.")?.parse().ok()?;
    ExportKind::ALL.get(index).copied()
}
