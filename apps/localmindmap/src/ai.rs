//! The AI surface: connections, the actions, the map as the chat's context, and the chat itself.
//!
//! Everything the user configures is a [`local_ai::Connection`] (provider preset, base URL, model)
//! plus a key that lives in the OS keychain. The actions turn the map into a [`Request`], the model
//! answers on a worker thread (so the window keeps painting), and the answer becomes nodes through
//! [`localmindmap_ai::actions::interpret`] - always as one undo step.
//!
//! The chat panel and the connections form are `craft-ai-ui` (the suite's AI surface, shared with
//! the other apps). What the app adds is what only the app knows: the system prompt, and the map -
//! its outline travels as the panel's context, so the model can answer about the map the user is
//! looking at instead of talking in the abstract.
//!
//! No test opens a socket: [`Ai::backend`] is a trait object, and the UI tests inject a mock. The
//! keychain is only touched by the connection form.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};

use ai_ui::ContextKind;
use craft_ai_ui as ai_ui;
use local_ai::{Connection, Msg};

use localmindmap_ai::Backend;
use localmindmap_ai::actions::{self, Request};

use crate::edit::Editor;

/// The app identity the connections file and the keychain entries belong to.
pub const AI_APP_ID: &str = "LocalMindmap";
/// The project URL the OpenRouter headers advertise.
pub const AI_APP_URL: &str = "https://github.com/Styx-Studios/Local-Mindmap";
/// The id of the saved connection, which is also its keychain account (`ai-connection-<id>`).
///
/// It keeps the spelling the app wrote before the rename on purpose: a connection saved by Local
/// Mind is filed under `localmind` both in `ai-connections.json` and in the keychain, and the copy
/// of the keys has to land on the entry the app reads afterwards. What changed is the keychain
/// *service*, `AI_APP_ID`; the shape of the saved connection did not.
pub const AI_CONNECTION_ID: &str = "localmind";

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

/// The AI state: who it talks to, what dialog is open, and what is in flight.
pub struct Ai {
    pub backend: Arc<dyn Backend>,
    /// The connection in use, from the suite's store (one, like the other apps of the suite).
    connection: Option<Connection>,
    /// Whether the store was read already (once: the panel is drawn every frame).
    loaded: bool,
    /// Whether the connections form is open.
    pub settings_open: bool,
    connections: ai_ui::ConnectionsPanel,
    /// The chat: the suite's panel, with the map as its context.
    panel: ai_ui::ChatPanel,
    /// Whether the chat is open (it takes width from the map area).
    pub chat_open: bool,
    /// The interface language the system prompt was written for, so it is only rebuilt on a change.
    system_lang: String,
    /// The outline last handed to the panel: the map is only read again when it changes.
    context: Option<String>,
    /// The models the provider answered with, filling the form's list.
    fetching: Option<Fetching>,
    running: Option<Running>,
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
        let mut panel = ai_ui::ChatPanel::default();
        // The last 8 turns and one budget for everything that leaves: the map's outline plus the
        // conversation. The map gets the larger share (it is what the user asks about).
        panel.set_policy(ai_ui::WindowPolicy { turns: ai_ui::Turns::LastN(8), total_budget_chars: localmindmap_ai::MAP_BUDGET + 12_000 });
        Self {
            backend,
            connection: None,
            loaded: false,
            settings_open: false,
            connections: ai_ui::ConnectionsPanel::default(),
            panel,
            chat_open: false,
            system_lang: String::new(),
            context: None,
            fetching: None,
            running: None,
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

    /// Reads the saved connection, if there is one (once: the panel is drawn every frame).
    pub fn load(&mut self) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        match Connection::load_all(&self.app()) {
            Ok(connections) => self.connection = connections.into_iter().next(),
            Err(error) => self.error = Some(format!("the AI connections could not be read: {error}")),
        }
    }

    /// The connection in use, replaced by the tests (the form saves through the keychain).
    #[cfg(test)]
    pub fn use_connection(&mut self, connection: Option<Connection>) {
        self.connection = connection;
        self.loaded = true;
    }

    /// The connection the calls use.
    pub fn connection(&self) -> Option<&Connection> {
        self.connection.as_ref()
    }

    /// Whether an answer is in flight.
    pub fn busy(&self) -> bool {
        self.running.is_some()
    }

    /// The turns the chat roll shows (the tests watch the send).
    #[cfg(test)]
    pub fn chat_turns(&self) -> Vec<(ai_ui::Role, String)> {
        self.panel.lines().iter().map(|line| (line.role, line.text.clone())).collect()
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

    /// Writes the connection (the key stays in the keychain) and keeps it in use.
    fn save(&mut self, draft: ai_ui::ConnectionDraft, key: String) {
        let connection = connection_of(&draft);
        if let Err(error) = connection.set_key(&self.app(), &key) {
            self.connections.failed(error);
            return;
        }
        match Connection::save_all(&self.app(), std::slice::from_ref(&connection)) {
            Ok(()) => {
                self.connection = Some(connection);
                self.panel.clear();
                self.connections.saved();
                self.settings_open = false;
            }
            Err(error) => self.connections.failed(error),
        }
    }

    /// Asks the provider for its model list, on a worker thread.
    fn fetch_models(&mut self, draft: ai_ui::ConnectionDraft, key: String) {
        if self.fetching.is_some() {
            return;
        }
        let connection = connection_of(&draft);
        if let Err(error) = connection.set_key(&self.app(), &key) {
            self.connections.failed(error);
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let backend = self.backend.clone();
        std::thread::Builder::new()
            .name("localmindmap-ai-models".into())
            .spawn(move || {
                let _ = tx.send(backend.models(&connection));
            })
            .ok();
        self.fetching = Some(Fetching { rx });
    }

    /// The connections form: the suite's dialog, with this app's store and keychain behind it.
    pub fn settings(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            self.connections.open = false;
            return;
        }
        if !self.connections.open {
            self.connections.set_presets(presets());
            self.connections.open_with(self.connection.as_ref().map(draft_of));
        }
        let labels = form_labels();
        match self.connections.show(ctx, &labels) {
            ai_ui::ConnectionsIntent::None => {}
            ai_ui::ConnectionsIntent::Cancel => {
                self.connections.open = false;
                self.settings_open = false;
            }
            ai_ui::ConnectionsIntent::Save(draft, key) => self.save(draft, key),
            ai_ui::ConnectionsIntent::LoadModels(draft, key) => self.fetch_models(draft, key),
        }
    }
}

impl Default for Ai {
    fn default() -> Self {
        Self::new(Arc::new(localmindmap_ai::LiveBackend::new(AI_APP_ID, AI_APP_URL)))
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

    /// Asks a question the chat did not type: the tests drive the whole send path with it.
    #[cfg(test)]
    pub fn ai_ask(&mut self, ctx: &egui::Context, question: impl Into<String>) {
        // The answer is a bubble: the panel opens so it is seen.
        self.ai.chat_open = true;
        prepare(self);
        if self.ai.panel.ask(question) {
            self.ai_send_chat(ctx);
        }
    }

    /// Sends the chat input (the panel already recorded the turn; the answer is a bubble, never
    /// interpreted as nodes).
    pub fn ai_send_chat(&mut self, ctx: &egui::Context) {
        if self.ai.busy() {
            return;
        }
        let Some(lines) = self.ai.panel.take_request() else { return };
        if self.ai.connection().is_none() {
            // The typed question stays in the panel, with the reason and Retry beside it.
            self.ai.panel.finish(Err(crate::tl!("No AI connection yet. Open AI > Connections… and add one.").to_string()));
            return;
        }
        let messages = lines
            .into_iter()
            .map(|line| match line.role {
                ai_ui::Role::System => Msg::System(line.text),
                ai_ui::Role::User => Msg::User(line.text),
                ai_ui::Role::Assistant => Msg::Assistant(line.text),
            })
            .collect();
        self.ai_spawn(ctx, messages, None, crate::tl!("Chat").to_string());
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
            .name("localmindmap-ai".into())
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
        self.ai.panel.stopped();
    }

    /// Polls the worker and applies what it finished. Called every frame.
    pub fn ai_tick(&mut self) {
        let mut deltas = String::new();
        let mut finished = None;
        {
            let Some(running) = self.ai.running.as_mut() else { return };
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
        }
        let request = self.ai.running.as_ref().and_then(|running| running.request.clone());
        let chat = self.ai.running.as_ref().is_some_and(|running| running.request.is_none());
        if !deltas.is_empty() && chat {
            self.ai.panel.push(&deltas);
        }
        if let Some(result) = finished {
            self.ai_finish_if_done(result, request);
        }
    }

    fn ai_finish_if_done(&mut self, result: Result<String, String>, request: Option<Request>) {
        let Some(running) = self.ai.running.take() else { return };
        if request.is_none() {
            // The chat: the answer is a bubble, not nodes.
            self.ai.panel.finish(result);
            return;
        }
        if running.cancelled.load(Ordering::Relaxed) {
            return;
        }
        let answer = match result {
            Ok(answer) => answer,
            Err(error) => {
                self.ai.error = Some(error);
                return;
            }
        };
        let Some(request) = request else { return };
        match actions::interpret(&request, &answer) {
            Ok(outcome) => self.apply_ai_outcome(&request, outcome),
            Err(error) => self.ai.error = Some(error),
        }
    }

    fn apply_ai_outcome(&mut self, request: &Request, outcome: localmindmap_ai::Outcome) {
        match outcome {
            localmindmap_ai::Outcome::NewMap { title, nodes } => self.replace_map(title, nodes, self.ai.generate_color),
            localmindmap_ai::Outcome::Nodes { parent, nodes } => self.add_ai_nodes(&parent, nodes),
            localmindmap_ai::Outcome::Text(text) => match request {
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

    /// Polls the model fetch. Called every frame.
    pub fn ai_tick_models(&mut self) {
        let Some(fetching) = self.ai.fetching.as_mut() else { return };
        match fetching.rx.try_recv() {
            Ok(Ok(models)) => {
                self.ai.connections.models = models;
                self.ai.fetching = None;
            }
            Ok(Err(error)) => {
                self.ai.connections.failed(error);
                self.ai.fetching = None;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => self.ai.fetching = None,
        }
    }
}

/// The AI menus and dialogs. Called by the editor every frame (the status strip is drawn before the
/// canvas, so it stays visible).
pub fn ui(ui: &mut egui::Ui, editor: &mut Editor) {
    let ctx = ui.ctx().clone();
    editor.ai.settings(&ctx);
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
            editor.ai.chat_open = true;
            editor.ai.load();
            ui.close();
        }
        if ui.button(crate::tl!("Connections…")).clicked() {
            editor.ai.load();
            editor.ai.settings_open = true;
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

/// Hands the panel what only this app can say: the system prompt (once per language) and the map's
/// outline as its context (only when the outline changed).
fn prepare(editor: &mut Editor) {
    let lang = crate::i18n::code();
    if editor.ai.system_lang != lang {
        editor.ai.panel.set_system(actions::chat_system(lang));
        editor.ai.system_lang = lang.to_string();
    }
    let outline = localmindmap_ai::map_outline(&editor.doc.map, localmindmap_ai::MAP_BUDGET);
    if editor.ai.context.as_deref() != Some(outline.text.as_str()) {
        let label = crate::i18n::tr_fmt(crate::tl!("the map · nodes: {nodes}"), &[("nodes", &outline.nodes.to_string())]);
        let context = if outline.text.trim().is_empty() {
            None
        } else {
            Some(ai_ui::ContextSource::new(ContextKind::Custom, label, outline.text.clone(), localmindmap_ai::MAP_BUDGET, ai_ui::Side::Head))
        };
        editor.ai.panel.set_context(context);
        editor.ai.context = Some(outline.text);
    }
}

/// The chat, as a side panel of the map area: the suite's panel over this app's chat plumbing.
pub fn chat_panel(ui: &mut egui::Ui, editor: &mut Editor) {
    if !editor.ai.chat_open {
        return;
    }
    let ctx = ui.ctx().clone();
    prepare(editor);
    let model = editor.ai.connection().map(|connection| connection.model.clone()).unwrap_or_default();
    let ready = editor.ai.connection().is_some_and(|connection| connection.has_key(&editor.ai.app()));
    let labels = chat_labels(&model, ready);
    egui::Panel::right("ai-chat").resizable(true).default_size(360.0).max_size(560.0).show(ui, |ui| {
        // The suite's panel keeps one button for the last answer; the map takes it as a branch by
        // default, and this row offers the single-node form beside it.
        if editor.ai.panel.last_answer().is_some()
            && ui.small_button(crate::tl!("Add the last answer as a node")).clicked()
            && let Some(answer) = editor.ai.panel.last_answer().map(str::to_owned)
        {
            editor.ai_insert_chat(&answer, false);
        }
        match editor.ai.panel.show(ui, &labels) {
            ai_ui::ChatAction::None => {}
            ai_ui::ChatAction::Send => editor.ai_send_chat(&ctx),
            ai_ui::ChatAction::Retry => {
                if editor.ai.panel.retry() {
                    editor.ai_send_chat(&ctx);
                }
            }
            ai_ui::ChatAction::Stop => editor.ai_cancel(),
            ai_ui::ChatAction::Clear => editor.ai.panel.clear(),
            ai_ui::ChatAction::Insert => {
                if let Some(answer) = editor.ai.panel.last_answer().map(str::to_owned) {
                    editor.ai_insert_chat(&answer, true);
                }
            }
            ai_ui::ChatAction::Settings => editor.ai.settings_open = true,
            ai_ui::ChatAction::Close => editor.ai.chat_open = false,
        }
    });
}

/// The chat labels, in the app's language.
fn chat_labels<'a>(model: &'a str, ready: bool) -> ai_ui::ChatLabels<'a> {
    ai_ui::ChatLabels {
        title: crate::tl!("Chat"),
        not_configured: if ready { "" } else { crate::tl!("No AI connection yet") },
        configure: crate::tl!("Settings"),
        settings: crate::tl!("Settings"),
        clear: crate::tl!("Clear conversation"),
        close: crate::tl!("Close"),
        send: crate::tl!("Send"),
        stop: crate::tl!("Stop"),
        insert: crate::tl!("As a branch"),
        retry: crate::tl!("Retry"),
        dismiss: crate::tl!("Dismiss"),
        empty: crate::tl!("Ask about the map, or about anything the model can help with."),
        input_hint: crate::tl!("Message the AI…"),
        chip: &chip,
        model,
        suggestions: &[],
        to_end: crate::tl!("To the end"),
    }
}

/// The context chip: what actually goes to the model.
fn chip(info: &ai_ui::ContextInfo) -> String {
    crate::i18n::tr_fmt(
        crate::tl!("reading: {label} · {sent} of {total} chars · {turns} of {kept} turns"),
        &[
            ("label", &info.label),
            ("sent", &info.sent.to_string()),
            ("total", &info.total.to_string()),
            ("turns", &info.turns.to_string()),
            ("kept", &info.kept.to_string()),
        ],
    )
}

fn form_labels<'a>() -> ai_ui::ConnectionsLabels<'a> {
    ai_ui::ConnectionsLabels {
        title: crate::tl!("AI connections"),
        provider: crate::tl!("Provider"),
        base_url: crate::tl!("Base URL"),
        model: crate::tl!("Model"),
        api_key: crate::tl!("API key"),
        list_models: crate::tl!("Fetch models"),
        save: crate::tl!("Save"),
        cancel: crate::tl!("Cancel"),
        hint_url: crate::tl!("Empty uses the provider's own address."),
        hint_key: crate::tl!("The key stays in the operating system's keychain. The map leaves this computer only when you ask the model something."),
        close: crate::tl!("Close"),
        needs_key: crate::tl!("Paste the API key first: this provider asks for one."),
        needs_url: crate::tl!("Choose a provider or type its address first."),
        needs_model: crate::tl!("Choose a model first (Fetch models lists them)."),
    }
}

fn presets() -> Vec<ai_ui::Preset> {
    local_ai::PROVIDERS.iter().map(|p| ai_ui::Preset { id: p.id.to_owned(), label: p.label.to_owned(), base_url: p.base_url.to_owned() }).collect()
}

fn draft_of(connection: &Connection) -> ai_ui::ConnectionDraft {
    ai_ui::ConnectionDraft { provider: connection.provider.clone(), base_url: connection.base_url.clone(), model: connection.model.clone() }
}

fn connection_of(draft: &ai_ui::ConnectionDraft) -> Connection {
    let label = local_ai::provider(&draft.provider).map(|p| p.label.to_owned()).unwrap_or_else(|| draft.provider.clone());
    Connection {
        id: AI_CONNECTION_ID.to_owned(),
        provider: draft.provider.clone(),
        label,
        base_url: draft.base_url.trim().to_string(),
        model: draft.model.trim().to_string(),
    }
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
            let hint = ui.label(egui::RichText::new(crate::tl!("A topic, or a whole text to organize.")).weak());
            ui.add(egui::TextEdit::multiline(&mut topic).desired_rows(5).desired_width(f32::INFINITY)).labelled_by(hint.id);
            ui.horizontal(|ui| {
                let depth_label = ui.label(crate::tl!("Depth"));
                ui.add(egui::DragValue::new(&mut depth).range(1..=4)).labelled_by(depth_label.id);
                let breadth_label = ui.label(crate::tl!("Breadth"));
                ui.add(egui::DragValue::new(&mut breadth).range(2..=8)).labelled_by(breadth_label.id);
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
            let hint = ui.label(egui::RichText::new(subject.clone()).weak());
            ui.add(egui::TextEdit::multiline(&mut instruction).desired_rows(3).desired_width(f32::INFINITY)).labelled_by(hint.id);
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
