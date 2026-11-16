//! The AI surface: connections, the actions, the status strip and the chat.
//!
//! Everything the user configures is a [`local_ai::Connection`] (provider preset, base URL, model)
//! plus a key that lives in the OS keychain. The actions turn the map into a [`Request`], the model
//! answers on a worker thread (so the window keeps painting), and the answer becomes nodes through
//! [`localmind_ai::actions::interpret`] - always as one undo step.
//!
//! No test opens a socket: [`Ai::backend`] is a trait object, and the UI tests inject a mock. The
//! keychain is only touched by the connection dialog.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};

use local_ai::{Connection, Msg};

use localmind_ai::Backend;
use localmind_ai::actions::{self, Request};

use crate::edit::Editor;

/// The app identity the connections file and the keychain entries belong to.
pub const AI_APP_ID: &str = "LocalMind";
/// The project URL the OpenRouter headers advertise.
pub const AI_APP_URL: &str = "https://github.com/Anon5T4R/Local-Mind";

/// What a worker thread reports back.
enum Event {
    Delta(String),
    Done(Result<String, String>),
}

/// The request in flight (an action, or the chat when `request` is `None`).
struct Running {
    request: Option<Request>,
    rx: Receiver<Event>,
    /// The status line (the action and the node it is about).
    status: String,
    /// The answer so far, for the strip.
    partial: String,
    /// Set by the Stop button: the answer is dropped when it arrives.
    cancelled: Arc<AtomicBool>,
    /// When the worker started, so the strip can show how long the wait has been.
    started: std::time::Instant,
}

/// One model list fetch.
struct Fetching {
    rx: Receiver<Result<Vec<String>, String>>,
}

/// A connection being edited (new or existing).
pub struct Draft {
    pub id: String,
    pub provider: String,
    pub label: String,
    pub base_url: String,
    pub model: String,
    /// The key typed in this dialog; "Save key" writes it to the keychain.
    pub key: String,
    pub key_saved: bool,
}

/// One chat turn.
#[derive(Clone, Debug, PartialEq)]
pub struct ChatTurn {
    pub role: Role,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

/// The chat panel.
#[derive(Default)]
pub struct Chat {
    pub open: bool,
    pub messages: Vec<ChatTurn>,
    pub input: String,
    /// The answer being streamed.
    pub streaming: String,
    pub busy: bool,
}

impl Chat {
    /// The conversation as the client's messages: the chat system turn plus every exchange.
    pub fn history(&self, lang: &str) -> Vec<Msg> {
        let mut out = vec![Msg::System(actions::chat_system(lang))];
        out.extend(self.messages.iter().map(|turn| match turn.role {
            Role::User => Msg::User(turn.text.clone()),
            Role::Assistant => Msg::Assistant(turn.text.clone()),
        }));
        out
    }
}

/// The AI state: who it talks to, what dialog is open, and what is in flight.
pub struct Ai {
    pub backend: Arc<dyn Backend>,
    pub connections: Vec<Connection>,
    /// The connection the actions use.
    pub active: Option<String>,
    pub panel: bool,
    pub draft: Option<Draft>,
    /// Whether each connection has a key in the keychain, by id.
    pub keys: std::collections::BTreeMap<String, bool>,
    running: Option<Running>,
    fetching: Option<Fetching>,
    /// The model list fetched for the draft.
    pub models: Vec<String>,
    pub chat: Chat,
    /// The generate-map dialog: its topic, and the shape it remembers.
    pub generate_topic: Option<String>,
    pub generate_shape: (u8, u8),
    /// Whether a generated map has its branches coloured by order (the app's palette walk).
    pub generate_color: bool,
    /// The rewrite dialog: the node and the instruction.
    pub rewrite: Option<(String, String)>,
    /// The last error, shown in the banner.
    pub error: Option<String>,
}

impl Ai {
    pub fn new(backend: Arc<dyn Backend>) -> Self {
        Self {
            backend,
            connections: Vec::new(),
            active: None,
            panel: false,
            draft: None,
            keys: Default::default(),
            running: None,
            fetching: None,
            models: Vec::new(),
            chat: Chat::default(),
            generate_topic: None,
            generate_shape: (2, 4),
            generate_color: true,
            rewrite: None,
            error: None,
        }
    }

    /// The identity for the connections file and the keychain.
    pub fn app(&self) -> local_ai::App {
        local_ai::App { id: AI_APP_ID, url: AI_APP_URL }
    }

    /// Reads the saved connections (a missing file is no connections).
    pub fn load(&mut self) {
        match Connection::load_all(&self.app()) {
            Ok(connections) => {
                self.connections = connections;
                self.active = self.active.clone().or_else(|| self.connections.first().map(|c| c.id.clone()));
                self.refresh_keys();
            }
            Err(error) => self.error = Some(format!("the AI connections could not be read: {error}")),
        }
    }

    /// Writes the connections (the keys stay in the keychain).
    pub fn save(&mut self) {
        if let Err(error) = Connection::save_all(&self.app(), &self.connections) {
            self.error = Some(format!("the AI connections could not be saved: {error}"));
        }
    }

    /// The connection the actions use: the active one, else the first saved one.
    pub fn connection(&self) -> Option<&Connection> {
        let id = self.active.as_deref().or_else(|| self.connections.first().map(|c| c.id.as_str()))?;
        self.connections.iter().find(|c| c.id == id).or_else(|| self.connections.first())
    }

    /// Whether an answer is in flight.
    pub fn busy(&self) -> bool {
        self.running.is_some()
    }

    /// The answer so far, for the strip.
    pub fn partial(&self) -> &str {
        self.running.as_ref().map(|running| running.partial.as_str()).unwrap_or("")
    }

    fn status(&self) -> Option<&str> {
        self.running.as_ref().map(|running| running.status.as_str())
    }

    /// How long the current request has been running, in seconds (0 when idle).
    fn elapsed_secs(&self) -> u64 {
        self.running.as_ref().map(|running| running.started.elapsed().as_secs()).unwrap_or(0)
    }

    /// Whether a connection has a key (the cache is filled when the panel opens).
    pub fn key_of(&self, id: &str) -> bool {
        self.keys.get(id).copied().unwrap_or(false)
    }

    /// Re-reads the keychain for every connection.
    pub fn refresh_keys(&mut self) {
        let app = self.app();
        for connection in &self.connections {
            let has = connection.has_key(&app);
            self.keys.insert(connection.id.clone(), has);
        }
    }
}

impl Default for Ai {
    fn default() -> Self {
        Self::new(Arc::new(localmind_ai::LiveBackend::new(AI_APP_ID, AI_APP_URL)))
    }
}

impl Editor {
    // ---- commands ---------------------------------------------------------------------------

    pub fn ai_generate(&mut self, ctx: &egui::Context, topic: String, depth: u8, breadth: u8) {
        self.ai_start(ctx, Some(Request::GenerateMap { topic, depth: u32::from(depth), breadth: u32::from(breadth) }));
    }

    pub fn ai_expand(&mut self, ctx: &egui::Context, node: &str) {
        self.ai_start(ctx, Some(Request::Expand { node: node.to_string(), count: 5 }));
    }

    pub fn ai_deep_expand(&mut self, ctx: &egui::Context, node: &str) {
        self.ai_start(ctx, Some(Request::DeepExpand { node: node.to_string() }));
    }

    pub fn ai_explain(&mut self, ctx: &egui::Context, node: &str) {
        self.ai_start(ctx, Some(Request::Explain { node: node.to_string() }));
    }

    pub fn ai_rewrite(&mut self, ctx: &egui::Context, node: &str, instruction: String) {
        if instruction.trim().is_empty() {
            return;
        }
        self.ai_start(ctx, Some(Request::Rephrase { node: node.to_string(), instruction }));
    }

    /// Sends the chat input (streamed into a bubble, never interpreted as nodes).
    pub fn ai_send_chat(&mut self, ctx: &egui::Context) {
        let text = self.ai.chat.input.trim().to_string();
        if text.is_empty() || self.ai.busy() {
            return;
        }
        if self.ai.connection().is_none() {
            self.ai.error = Some(crate::tl!("No AI connection yet. Open AI > Connections… and add one.").to_string());
            return;
        }
        self.ai.chat.messages.push(ChatTurn { role: Role::User, text });
        self.ai.chat.input.clear();
        self.ai.chat.streaming.clear();
        self.ai.chat.busy = true;
        let history = self.ai.chat.history(crate::i18n::code());
        self.ai_spawn(ctx, history, None, crate::tl!("Chat").to_string());
    }

    /// Starts one request: the messages come from the action (or from the chat), the answer streams
    /// into the state and is applied when it ends.
    fn ai_start(&mut self, ctx: &egui::Context, request: Option<Request>) {
        if self.ai.busy() {
            return;
        }
        let Some(request) = request else { return };
        if self.ai.connection().is_none() {
            self.ai.error = Some(crate::tl!("No AI connection yet. Open AI > Connections… and add one.").to_string());
            return;
        }
        let messages = match actions::messages(&self.doc.map, &request, crate::i18n::code()) {
            Ok(messages) => messages,
            Err(error) => {
                self.ai.error = Some(error);
                return;
            }
        };
        let status = request.status(&self.doc.map);
        self.ai_spawn(ctx, messages, Some(request), status);
    }

    fn ai_spawn(&mut self, ctx: &egui::Context, messages: Vec<Msg>, request: Option<Request>, status: String) {
        let Some(connection) = self.ai.connection().cloned() else { return };
        let (tx, rx) = std::sync::mpsc::channel();
        let backend = self.ai.backend.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancelled = cancel.clone();
        let worker_ctx = ctx.clone();
        std::thread::Builder::new()
            .name("localmind-ai".into())
            .spawn(move || {
                let mut on_delta = |delta: &str| {
                    if cancelled.load(Ordering::Relaxed) {
                        return;
                    }
                    let _ = tx.send(Event::Delta(delta.to_string()));
                    worker_ctx.request_repaint();
                };
                let result = backend.stream(&connection, &messages, &mut on_delta);
                let _ = tx.send(Event::Done(result));
                worker_ctx.request_repaint();
            })
            .ok();
        self.ai.error = None;
        self.ai.running = Some(Running { request, rx, status, partial: String::new(), cancelled: cancel, started: std::time::Instant::now() });
    }

    /// Stops showing the answer. The request itself runs to its end in the worker (the shared client
    /// has no abort), so the result is discarded here instead - a declared limitation.
    pub fn ai_cancel(&mut self) {
        if let Some(running) = self.ai.running.take() {
            running.cancelled.store(true, Ordering::Relaxed);
        }
        self.ai.chat.busy = false;
        self.ai.chat.streaming.clear();
    }

    /// Polls the worker and applies what it finished. Called every frame.
    pub fn ai_tick(&mut self) {
        let Some(running) = self.ai.running.as_mut() else { return };
        let mut finished = None;
        let mut deltas = String::new();
        loop {
            match running.rx.try_recv() {
                Ok(Event::Delta(delta)) => {
                    running.partial.push_str(&delta);
                    deltas.push_str(&delta);
                }
                Ok(Event::Done(result)) => {
                    finished = Some(result);
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    finished = Some(Err("the AI worker stopped".into()));
                    break;
                }
            }
        }
        let request = running.request.clone();
        if !deltas.is_empty() && self.ai.chat.busy {
            self.ai.chat.streaming.push_str(&deltas);
        }
        if let Some(result) = finished {
            self.ai_finish_if_done(result, request);
        }
    }

    fn ai_finish_if_done(&mut self, result: Result<String, String>, request: Option<Request>) {
        let Some(running) = self.ai.running.take() else { return };
        if running.cancelled.load(Ordering::Relaxed) {
            self.ai.chat.busy = false;
            self.ai.chat.streaming.clear();
            return;
        }
        let answer = match result {
            Ok(answer) => answer,
            Err(error) => {
                self.ai.error = Some(error);
                self.ai.chat.busy = false;
                self.ai.chat.streaming.clear();
                return;
            }
        };
        if request.is_none() {
            // The chat: the answer is a bubble, not nodes.
            self.ai.chat.messages.push(ChatTurn { role: Role::Assistant, text: answer });
            self.ai.chat.streaming.clear();
            self.ai.chat.busy = false;
            return;
        }
        let Some(request) = request else { return };
        match actions::interpret(&request, &answer) {
            Ok(outcome) => self.apply_ai_outcome(&request, outcome),
            Err(error) => {
                self.ai.error = Some(error);
                self.ai.chat.busy = false;
            }
        }
    }

    fn apply_ai_outcome(&mut self, request: &Request, outcome: localmind_ai::Outcome) {
        match outcome {
            localmind_ai::Outcome::NewMap { title, nodes } => self.replace_map(title, nodes, self.ai.generate_color),
            localmind_ai::Outcome::Nodes { parent, nodes } => self.add_ai_nodes(&parent, nodes),
            localmind_ai::Outcome::Text(text) => match request {
                Request::Explain { node } => {
                    let node = node.clone();
                    self.add_ai_child(&node, &text);
                }
                Request::Rephrase { node, .. } => {
                    let node = node.clone();
                    self.apply_ai_text(&node, &text);
                }
                _ => self.ai.error = Some("the model answered a text for a request that expected nodes".to_string()),
            },
        }
    }

    /// Inserts an answer in the chat as one node, or as a subtree parsed from the outline.
    pub fn ai_insert_chat(&mut self, text: &str, as_branch: bool) {
        let Some(parent) = self.selected.clone().or_else(|| self.doc.root_id()) else { return };
        if as_branch {
            let nodes = actions::parse_outline(text, &parent);
            if nodes.is_empty() {
                self.ai_insert_chat(text, false);
            } else {
                self.add_ai_nodes(&parent, nodes);
            }
        } else {
            let label: String = text.lines().next().unwrap_or(text).chars().take(120).collect();
            let label = label.trim().to_string();
            if !label.is_empty() {
                self.add_ai_child(&parent, &label);
            }
        }
    }

    /// Adds or updates the connection being edited and activates it.
    pub fn ai_save_connection(&mut self, draft: Draft) {
        let connection = Connection {
            id: draft.id.clone(),
            provider: draft.provider.clone(),
            label: draft.label.clone(),
            base_url: draft.base_url.trim().to_string(),
            model: draft.model.trim().to_string(),
        };
        match self.ai.connections.iter_mut().find(|existing| existing.id == draft.id) {
            Some(existing) => *existing = connection,
            None => self.ai.connections.push(connection),
        }
        self.ai.active = Some(draft.id.clone());
        self.ai.draft = None;
        self.ai.save();
        self.ai.refresh_keys();
    }

    /// Writes the key of the draft's connection to the keychain.
    pub fn ai_save_key(&mut self, draft: &mut Draft) {
        let app = self.ai.app();
        let connection = Connection {
            id: draft.id.clone(),
            provider: draft.provider.clone(),
            label: draft.label.clone(),
            base_url: draft.base_url.clone(),
            model: draft.model.clone(),
        };
        if draft.key.trim().is_empty() {
            if let Err(error) = connection.clear_key(&app) {
                self.ai.error = Some(format!("the key could not be removed: {error}"));
            }
            draft.key_saved = false;
        } else if let Err(error) = connection.set_key(&app, draft.key.trim()) {
            self.ai.error = Some(format!("the key could not be saved: {error}"));
        } else {
            draft.key.clear();
            draft.key_saved = true;
        }
        self.ai.refresh_keys();
    }

    /// Asks the provider for its model list (on a worker thread).
    pub fn ai_fetch_models(&mut self, ctx: &egui::Context, draft: &Draft) {
        if self.ai.fetching.is_some() {
            return;
        }
        let connection = Connection {
            id: draft.id.clone(),
            provider: draft.provider.clone(),
            label: draft.label.clone(),
            base_url: draft.base_url.clone(),
            model: draft.model.clone(),
        };
        let (tx, rx) = std::sync::mpsc::channel();
        let backend = self.ai.backend.clone();
        let worker_ctx = ctx.clone();
        std::thread::Builder::new()
            .name("localmind-ai-models".into())
            .spawn(move || {
                let _ = tx.send(backend.models(&connection));
                worker_ctx.request_repaint();
            })
            .ok();
        self.ai.fetching = Some(Fetching { rx });
    }

    /// Polls the model fetch. Called every frame.
    pub fn ai_tick_models(&mut self) {
        let Some(fetching) = self.ai.fetching.as_mut() else { return };
        match fetching.rx.try_recv() {
            Ok(Ok(models)) => {
                self.ai.models = models;
                self.ai.fetching = None;
            }
            Ok(Err(error)) => {
                self.ai.error = Some(error);
                self.ai.fetching = None;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => self.ai.fetching = None,
        }
    }
}

/// The AI menus, dialogs and chat. Called by the editor every frame (the status strip is drawn
/// before the canvas, so it stays visible).
pub fn ui(ui: &mut egui::Ui, editor: &mut Editor) {
    connections_panel(ui, editor);
    generate_dialog(ui, editor);
    rewrite_dialog(ui, editor);
}

pub fn menu(ui: &mut egui::Ui, editor: &mut Editor) {
    let ctx = ui.ctx().clone();
    ui.menu_button(crate::tl!("AI"), |ui| {
        if ui.button(crate::tl!("Generate map…")).clicked() {
            editor.ai.generate_topic = Some(String::new());
            ui.close();
        }
        ui.separator();
        let selected = editor.selected.clone();
        let enabled = selected.is_some() && !editor.ai.busy();
        if ui.add_enabled(enabled, egui::Button::new(crate::tl!("Expand"))).clicked()
            && let Some(node) = &selected
        {
            editor.ai_expand(&ctx, node);
            ui.close();
        }
        if ui.add_enabled(enabled, egui::Button::new(crate::tl!("Expand deep"))).clicked()
            && let Some(node) = &selected
        {
            editor.ai_deep_expand(&ctx, node);
            ui.close();
        }
        if ui.add_enabled(enabled, egui::Button::new(crate::tl!("Explain"))).clicked()
            && let Some(node) = &selected
        {
            editor.ai_explain(&ctx, node);
            ui.close();
        }
        if ui.add_enabled(enabled, egui::Button::new(crate::tl!("Rewrite…"))).clicked()
            && let Some(node) = &selected
        {
            editor.ai.rewrite = Some((node.clone(), String::new()));
            ui.close();
        }
        ui.separator();
        if ui.add_enabled(!editor.ai.busy(), egui::Button::new(crate::tl!("Chat"))).clicked() {
            editor.ai.chat.open = true;
            ui.close();
        }
        if ui.button(crate::tl!("Connections…")).clicked() {
            editor.ai.panel = true;
            editor.ai.refresh_keys();
            ui.close();
        }
    });
}

/// The error banner and the in-flight status. Drawn **before** the canvas (which takes all the
/// remaining space), otherwise it lands below the canvas and is clipped: the user waits with no
/// spinner and no error. Called from the editor, once per frame.
pub(crate) fn status_strip(ui: &mut egui::Ui, editor: &mut Editor) {
    if let Some(error) = editor.ai.error.clone() {
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(ui.visuals().error_fg_color, error);
            if ui.button(crate::tl!("Dismiss")).clicked() {
                editor.ai.error = None;
            }
        });
    }
    let Some(status) = editor.ai.status().map(str::to_string) else { return };
    let elapsed = editor.ai.elapsed_secs();
    ui.horizontal(|ui| {
        ui.spinner();
        ui.label(status);
        // The seconds on the strip are the answer to "is it doing anything?"; without them a slow
        // model is indistinguishable from a hang.
        ui.label(egui::RichText::new(format!("{elapsed}s")).weak());
        let tail = editor.ai.partial().chars().rev().take(80).collect::<String>().chars().rev().collect::<String>();
        if !tail.trim().is_empty() {
            ui.label(egui::RichText::new(tail.replace('\n', " ")).weak());
        }
        if ui.button(crate::tl!("Stop")).clicked() {
            editor.ai_cancel();
        }
    });
    if elapsed >= 15 {
        ui.label(egui::RichText::new(crate::tl!("The model is taking a while…")).weak());
    }
}

fn connections_panel(ui: &mut egui::Ui, editor: &mut Editor) {
    if !editor.ai.panel {
        return;
    }
    let ctx = ui.ctx().clone();
    let mut open = true;
    egui::Window::new(crate::tl!("AI connections"))
        .open(&mut open)
        .collapsible(false)
        // Auto-sized: a resizable window keeps its first size and would clip the connection form.
        .resizable(false)
        .default_width(560.0)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(&ctx, |ui| connections_body(ui, editor));
    editor.ai.panel = open;
}

fn connections_body(ui: &mut egui::Ui, editor: &mut Editor) {
    ui.label(
        egui::RichText::new(crate::tl!(
            "The key stays in the operating system's keychain. The map leaves this computer only when you ask the model something."
        ))
        .weak(),
    );
    ui.add_space(6.0);
    let mut activate = None;
    let mut edit = None;
    let mut remove = None;
    for connection in editor.ai.connections.clone() {
        ui.horizontal(|ui| {
            let active = editor.ai.active.as_deref() == Some(connection.id.as_str());
            if ui.radio(active, format!("{} — {}", connection.label, connection.model)).clicked() {
                activate = Some(connection.id.clone());
            }
            if editor.ai.key_of(&connection.id) {
                ui.label(egui::RichText::new(crate::tl!("key saved")).weak());
            }
            if ui.button(crate::tl!("Edit")).clicked() {
                edit = Some(connection.clone());
            }
            if ui.button(crate::tl!("Remove")).clicked() {
                remove = Some(connection.id.clone());
            }
        });
    }
    if let Some(id) = activate {
        editor.ai.active = Some(id);
    }
    if let Some(connection) = edit {
        editor.ai.draft = Some(Draft {
            id: connection.id.clone(),
            provider: connection.provider.clone(),
            label: connection.label.clone(),
            base_url: connection.base_url.clone(),
            model: connection.model.clone(),
            key: String::new(),
            key_saved: editor.ai.key_of(&connection.id),
        });
        editor.ai.models.clear();
    }
    if let Some(id) = remove {
        editor.ai.connections.retain(|connection| connection.id != id);
        if editor.ai.active.as_deref() == Some(id.as_str()) {
            editor.ai.active = editor.ai.connections.first().map(|c| c.id.clone());
        }
        editor.ai.save();
        editor.ai.refresh_keys();
    }
    if editor.ai.draft.is_none() && ui.button(crate::tl!("Add connection")).clicked() {
        let provider = local_ai::PROVIDERS.iter().find(|p| p.id == "gemini").or_else(|| local_ai::PROVIDERS.first());
        let (id, label, base_url) = provider.map(|p| (p.id, p.label, p.base_url)).unwrap_or(("custom", "Custom", ""));
        editor.ai.draft = Some(Draft {
            id: localmind_model::new_id(),
            provider: id.to_string(),
            label: label.to_string(),
            base_url: base_url.to_string(),
            model: String::new(),
            key: String::new(),
            key_saved: false,
        });
        editor.ai.models.clear();
    }
    if let Some(mut draft) = editor.ai.draft.take() {
        ui.separator();
        match draft_form(ui, editor, &mut draft) {
            DraftEvent::Open => editor.ai.draft = Some(draft),
            DraftEvent::Cancel => editor.ai.draft = None,
            // Already saved by the form (which cleared the draft and wrote the connection).
            DraftEvent::Saved => {}
        }
    }
}

enum DraftEvent {
    Open,
    Cancel,
    Saved,
}

fn draft_form(ui: &mut egui::Ui, editor: &mut Editor, draft: &mut Draft) -> DraftEvent {
    let mut event = DraftEvent::Open;
    ui.horizontal(|ui| {
        ui.label(crate::tl!("Provider"));
        egui::ComboBox::from_id_salt("ai-provider").selected_text(draft.label.clone()).show_ui(ui, |ui| {
            for provider in local_ai::PROVIDERS {
                if ui.selectable_label(draft.provider == provider.id, provider.label).clicked() {
                    draft.provider = provider.id.to_string();
                    draft.label = provider.label.to_string();
                    if !provider.base_url.is_empty() {
                        draft.base_url = provider.base_url.to_string();
                    }
                }
            }
        });
    });
    ui.horizontal(|ui| {
        ui.label(crate::tl!("Base URL"));
        ui.add(egui::TextEdit::singleline(&mut draft.base_url).desired_width(360.0));
    });
    ui.horizontal(|ui| {
        ui.label(crate::tl!("Model"));
        ui.add(egui::TextEdit::singleline(&mut draft.model).desired_width(220.0));
        if editor.ai.fetching.is_some() {
            ui.spinner();
        } else if ui.button(crate::tl!("Fetch models")).clicked() {
            let snapshot = Draft {
                id: draft.id.clone(),
                provider: draft.provider.clone(),
                label: draft.label.clone(),
                base_url: draft.base_url.clone(),
                model: draft.model.clone(),
                key: String::new(),
                key_saved: draft.key_saved,
            };
            editor.ai_fetch_models(ui.ctx(), &snapshot);
        }
        if !editor.ai.models.is_empty() {
            let selected = if draft.model.is_empty() { "…" } else { draft.model.as_str() };
            egui::ComboBox::from_id_salt("ai-model").selected_text(selected).show_ui(ui, |ui| {
                for model in editor.ai.models.clone() {
                    if ui.selectable_label(draft.model == model, &model).clicked() {
                        draft.model = model;
                    }
                }
            });
        }
    });
    ui.horizontal(|ui| {
        ui.label(crate::tl!("API key"));
        ui.add(egui::TextEdit::singleline(&mut draft.key).password(true).desired_width(280.0));
        if draft.key_saved {
            ui.label(egui::RichText::new(crate::tl!("key saved")).weak());
        }
        if ui.add_enabled(!draft.key.is_empty(), egui::Button::new(crate::tl!("Save key"))).clicked() {
            editor.ai_save_key(draft);
        }
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let can_save = !draft.model.trim().is_empty() && !draft.base_url.trim().is_empty();
        if ui.add_enabled(can_save, egui::Button::new(crate::tl!("Save"))).clicked() {
            let done = Draft {
                id: draft.id.clone(),
                provider: draft.provider.clone(),
                label: draft.label.clone(),
                base_url: draft.base_url.clone(),
                model: draft.model.clone(),
                key: String::new(),
                key_saved: draft.key_saved,
            };
            editor.ai_save_connection(done);
            event = DraftEvent::Saved;
        }
        if ui.button(crate::tl!("Cancel")).clicked() {
            event = DraftEvent::Cancel;
        }
    });
    event
}

fn generate_dialog(ui: &mut egui::Ui, editor: &mut Editor) {
    let Some(mut topic) = editor.ai.generate_topic.clone() else { return };
    let ctx = ui.ctx().clone();
    let mut open = true;
    let mut generate = false;
    let (mut depth, mut breadth) = editor.ai.generate_shape;
    egui::Window::new(crate::tl!("Generate map"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(420.0)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(&ctx, |ui| {
            ui.label(egui::RichText::new(crate::tl!("A topic, or a whole text to organize.")).weak());
            ui.add(egui::TextEdit::multiline(&mut topic).desired_rows(5).desired_width(f32::INFINITY));
            ui.horizontal(|ui| {
                ui.label(crate::tl!("Depth"));
                ui.add(egui::DragValue::new(&mut depth).range(1..=4));
                ui.label(crate::tl!("Breadth"));
                ui.add(egui::DragValue::new(&mut breadth).range(2..=8));
            });
            let mut color = editor.ai.generate_color;
            if ui.checkbox(&mut color, crate::tl!("Color branches")).changed() {
                editor.ai.generate_color = color;
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(!topic.trim().is_empty() && !editor.ai.busy(), egui::Button::new(crate::tl!("Generate"))).clicked() {
                    generate = true;
                }
                if ui.button(crate::tl!("Cancel")).clicked() {
                    editor.ai.generate_topic = None;
                }
            });
        });
    editor.ai.generate_shape = (depth, breadth);
    if !open {
        editor.ai.generate_topic = None;
        return;
    }
    if generate {
        editor.ai.generate_topic = None;
        editor.ai_generate(&ctx, topic, depth, breadth);
    } else if editor.ai.generate_topic.is_some() {
        editor.ai.generate_topic = Some(topic);
    }
}

fn rewrite_dialog(ui: &mut egui::Ui, editor: &mut Editor) {
    let Some((node, mut instruction)) = editor.ai.rewrite.clone() else { return };
    let ctx = ui.ctx().clone();
    let mut open = true;
    let mut apply = false;
    let subject = editor.doc.map.node(&node).map(|n| n.text.clone()).unwrap_or_default();
    egui::Window::new(crate::tl!("Rewrite"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(420.0)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(&ctx, |ui| {
            ui.label(egui::RichText::new(subject.clone()).weak());
            ui.add(egui::TextEdit::multiline(&mut instruction).desired_rows(3).desired_width(f32::INFINITY));
            ui.horizontal_wrapped(|ui| {
                for preset in [crate::tl!("shorter"), crate::tl!("more technical"), crate::tl!("as a question"), crate::tl!("fix the wording")] {
                    if ui.button(preset).clicked() {
                        instruction = preset.to_string();
                    }
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(!instruction.trim().is_empty() && !editor.ai.busy(), egui::Button::new(crate::tl!("Apply"))).clicked() {
                    apply = true;
                }
                if ui.button(crate::tl!("Cancel")).clicked() {
                    editor.ai.rewrite = None;
                }
            });
        });
    if !open {
        editor.ai.rewrite = None;
        return;
    }
    if apply {
        editor.ai.rewrite = None;
        editor.ai_rewrite(&ctx, &node, instruction);
    } else if editor.ai.rewrite.is_some() {
        editor.ai.rewrite = Some((node, instruction));
    }
}

/// The chat, as a side panel of the map area.
pub fn chat_panel(ui: &mut egui::Ui, editor: &mut Editor) {
    if !editor.ai.chat.open {
        return;
    }
    let ctx = ui.ctx().clone();
    egui::Panel::right("ai-chat").show(ui, |ui| {
        ui.set_min_width(320.0);
        ui.horizontal(|ui| {
            ui.strong(crate::tl!("Chat"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(crate::tl!("Close")).clicked() {
                    editor.ai.chat.open = false;
                }
                if ui.button(crate::tl!("Clear conversation")).clicked() {
                    editor.ai.chat.messages.clear();
                    editor.ai.chat.streaming.clear();
                }
            });
        });
        ui.separator();
        egui::ScrollArea::vertical().stick_to_bottom(true).max_height((ui.available_height() - 120.0).max(120.0)).show(ui, |ui| {
            for turn in editor.ai.chat.messages.clone() {
                let label = match turn.role {
                    Role::User => crate::tl!("You"),
                    Role::Assistant => crate::tl!("AI"),
                };
                ui.label(egui::RichText::new(label).weak());
                ui.label(&turn.text);
                if turn.role == Role::Assistant {
                    ui.horizontal(|ui| {
                        if ui.small_button(crate::tl!("As a node")).clicked() {
                            editor.ai_insert_chat(&turn.text, false);
                        }
                        if ui.small_button(crate::tl!("As a branch")).clicked() {
                            editor.ai_insert_chat(&turn.text, true);
                        }
                    });
                }
                ui.add_space(6.0);
            }
            if !editor.ai.chat.streaming.is_empty() {
                ui.label(egui::RichText::new(editor.ai.chat.streaming.clone()).italics());
            }
        });
        ui.separator();
        let busy = editor.ai.chat.busy;
        ui.add_enabled_ui(!busy, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut editor.ai.chat.input)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY)
                    .hint_text(crate::tl!("Message the AI…")),
            );
        });
        ui.horizontal(|ui| {
            if busy {
                if ui.button(crate::tl!("Stop")).clicked() {
                    editor.ai_cancel();
                }
            } else if ui.add_enabled(!editor.ai.chat.input.trim().is_empty(), egui::Button::new(crate::tl!("Send"))).clicked() {
                editor.ai_send_chat(&ctx);
            }
        });
    });
}
