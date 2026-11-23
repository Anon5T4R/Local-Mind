//! The window: the menu bar, the toolbar strip, the map area (an [`Editor`]) and the About window
//! that fronts the update check.
//!
//! The frame (title bar, menu strip, update prompt, file pickers) is the one the suite shares; the
//! map itself, its editing state and its keyboard live in [`crate::edit`].

use std::path::{Path, PathBuf};

use craft_icons::{Icon, ThemeMode};
use craft_update::updater::{Action, Status, Updater};

use crate::close::{self, Outcome};
use crate::doc::Document;
use crate::edit::{self, Editor};
use crate::i18n;
use crate::pickers::{Picked, Pickers};
use crate::prefs::Prefs;
use crate::suite_entry::APP;
use crate::tl;

mod commands;

/// The extensions the open/save dialogs offer. `json` is what the original app also accepted, and
/// its maps are plain JSON, so a `.json` a user renamed by hand still opens.
const MAP_EXTENSIONS: &[&str] = &["tmind", "json"];

/// What a pending file-picker answer is for.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Pending {
    Open,
    SaveAs,
    /// Write the map in this export format to the chosen file.
    Export(edit::ExportKind),
}

/// The windows of the shell (not of the editor) a test can open.
#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(crate) enum Shell {
    About,
    Settings,
    Palette,
    Unsaved,
}

pub struct LocalApp {
    updater: Updater,
    pickers: Pickers,
    /// Who draws the window frame (probed before the window was created), so an app-drawn frame
    /// gets its header bar with the desktop's button layout.
    frame: craft_window::Frame,
    about_open: bool,
    /// The Settings window (language and theme), opened by the gear and Edit > Settings….
    settings_open: bool,
    /// The command palette (Ctrl+K, or the search icon of the top bar).
    palette: craft_palette::Palette,
    /// The settings the app remembers between runs (the interface language).
    prefs: Prefs,
    pub(crate) editor: Editor,
    pending: Option<Pending>,
    /// The unsaved-changes guard on window close.
    close: close::Guard,
    /// The last failure, shown until dismissed: a map that could not be read must say so.
    pub(crate) error: Option<String>,
    /// The title currently on the window, so it is only sent when it changes.
    window_title: String,
    /// The control channel of `--control PORT` (agents and scripts), when the app was started with it.
    pub(crate) control: Option<crate::automation::control::Control>,
    /// The automation roots given with `--control` (none: nothing is confined).
    pub(crate) roots: Option<craft_mcp::automation::Roots>,
}

impl LocalApp {
    /// Give the window the control channel and the roots it enforces (`--control PORT`).
    pub(crate) fn attach_control(&mut self, control: Option<crate::automation::control::Control>, roots: Option<craft_mcp::automation::Roots>) {
        self.control = control;
        self.roots = roots;
    }
    pub fn new(cc: &eframe::CreationContext<'_>, frame: craft_window::Frame, initial: Option<PathBuf>) -> Self {
        crate::fonts::install(&cc.egui_ctx);
        // Linux: the desktop's light/dark choice, which winit does not report.
        craft_desktop_theme::follow(&cc.egui_ctx);
        let prefs = Prefs::load();
        i18n::set_language(&prefs.language);
        // The SVG loader for the suite's icons (theme, settings).
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let mode = crate::theme::mode_from_pref(&prefs.theme);
        let theme = crate::theme::Theme::resolve(mode, cc.egui_ctx.system_theme() != Some(egui::Theme::Light));
        theme.apply(&cc.egui_ctx);

        let mut app = Self {
            updater: Updater::new(&APP, env!("CARGO_PKG_VERSION")),
            pickers: Pickers::default(),
            frame,
            about_open: false,
            settings_open: false,
            palette: craft_palette::Palette::default(),
            prefs,
            editor: Editor::new(edit::new_document()),
            pending: None,
            close: close::Guard::default(),
            error: None,
            window_title: String::new(),
            control: None,
            roots: None,
        };
        app.editor.set_theme(theme);
        app.editor.mode = mode;
        app.editor.ai.load();
        if let Some(path) = initial {
            app.open_path(path);
        }
        app
    }

    /// The palette the current mode shows right now (Automatic: dark when the OS is dark or says nothing).
    fn resolved_theme(&self, ctx: &egui::Context) -> crate::theme::Theme {
        crate::theme::Theme::resolve(self.editor.mode, ctx.system_theme() != Some(egui::Theme::Light))
    }

    /// Switches to `mode`: the editor, the egui style and the saved preference follow.
    fn set_mode(&mut self, ctx: &egui::Context, mode: ThemeMode) {
        self.editor.mode = mode;
        let theme = self.resolved_theme(ctx);
        self.editor.set_theme(theme);
        theme.apply(ctx);
        self.prefs.theme = mode.id().to_string();
        self.prefs.save();
    }

    /// The suite's controls at the right of the top bar: the command palette, the theme (Automatic/Light/Dark) and the
    /// gear that opens Settings. Called inside a right-to-left layout, so the gear is drawn first and ends up rightmost.
    fn shell_buttons(&mut self, ui: &mut egui::Ui) {
        // Off the window's edge, and a finger's width between them.
        ui.add_space(8.0);
        if craft_icons::ui::button(ui, Icon::Settings, tl!("Settings")).clicked() {
            self.settings_open = true;
        }
        ui.add_space(6.0);
        let mut mode = self.editor.mode;
        let tooltip = i18n::tr_fmt("Theme: {mode}", &[("mode", tl!(mode.label()))]);
        if craft_icons::ui::theme_button(ui, &mut mode, &tooltip) {
            self.set_mode(ui.ctx(), mode);
        }
        ui.add_space(6.0);
        let shortcut = ui.ctx().format_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::K));
        let search = i18n::tr_fmt("Search commands ({shortcut})", &[("shortcut", &shortcut)]);
        if craft_icons::ui::button(ui, Icon::Search, &search).clicked() {
            self.palette.open();
        }
    }

    /// The command palette, when open: runs what the user picked.
    fn palette(&mut self, ctx: &egui::Context) {
        if !self.palette.is_open() {
            return;
        }
        let commands = self.commands(ctx);
        let labels = craft_palette::Labels { placeholder: tl!("Search commands"), empty: tl!("No command matches.") };
        if let Some(id) = self.palette.show(ctx, &labels, &commands) {
            self.run_command(ctx, &id);
        }
    }

    /// The menu bar: the app's commands and Help.
    fn menus(&mut self, ui: &mut egui::Ui) {
        let mut quit = false;
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button(tl!("File"), |ui| {
                if ui.button(tl!("New map")).clicked() {
                    self.new_map();
                    ui.close();
                }
                if ui.button(tl!("Open…")).clicked() {
                    self.ask_open(ui.ctx());
                    ui.close();
                }
                ui.separator();
                if ui.button(tl!("Save")).clicked() {
                    self.save(ui.ctx());
                    ui.close();
                }
                if ui.button(tl!("Save As…")).clicked() {
                    self.ask_save_as(ui.ctx());
                    ui.close();
                }
                ui.menu_button(tl!("Export"), |ui| {
                    for kind in edit::ExportKind::ALL {
                        if ui.button(tl!(kind.label())).clicked() {
                            self.ask_export(ui.ctx(), kind);
                            ui.close();
                        }
                    }
                });
                ui.separator();
                if ui.button(tl!("Exit")).clicked() {
                    quit = true;
                    ui.close();
                }
            });
            // Edit and View live in the editor (they only touch it), so the headless tests can use
            // the same menus the window does.
            if let Some(mode) = edit::menus(ui, &mut self.editor) {
                self.set_mode(ui.ctx(), mode);
            }
            if std::mem::take(&mut self.editor.settings_requested) {
                self.settings_open = true;
            }
            crate::ai::menu(ui, &mut self.editor);
            ui.menu_button(tl!("Help"), |ui| {
                if ui.button(tl!("Keyboard and mouse")).clicked() {
                    self.editor.help_open = true;
                    ui.close();
                }
                if ui.button(tl!("Check for Updates…")).clicked() {
                    // The outcome shows in the About window, which is where a check belongs.
                    self.about_open = true;
                    self.updater.check_now(ui.ctx());
                    ui.close();
                }
                if ui.button(tl!("About Local Mind")).clicked() {
                    self.about_open = true;
                    ui.close();
                }
            });
        });
        if quit {
            self.request_close(ui.ctx());
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        // Every button says its shortcut on hover, and a disabled one says why it is off.
        let key = |ui: &egui::Ui, modifiers: egui::Modifiers, key: egui::Key| ui.ctx().format_shortcut(&egui::KeyboardShortcut::new(modifiers, key));
        let command = egui::Modifiers::COMMAND;
        if ui.button(tl!("New map")).on_hover_text(key(ui, command, egui::Key::N)).clicked() {
            self.new_map();
        }
        if ui.button(tl!("Open…")).on_hover_text(key(ui, command, egui::Key::O)).clicked() {
            self.ask_open(ui.ctx());
        }
        if ui.button(tl!("Save")).on_hover_text(key(ui, command, egui::Key::S)).clicked() {
            self.save(ui.ctx());
        }
        if ui.button(tl!("Save As…")).on_hover_text(key(ui, command | egui::Modifiers::SHIFT, egui::Key::S)).clicked() {
            self.ask_save_as(ui.ctx());
        }
        ui.separator();
        let undo = key(ui, command, egui::Key::Z);
        if ui
            .add_enabled(self.editor.can_undo(), egui::Button::new(tl!("Undo")))
            .on_hover_text(&undo)
            .on_disabled_hover_text(tl!("Nothing to undo."))
            .clicked()
        {
            self.editor.undo();
        }
        let redo = key(ui, command | egui::Modifiers::SHIFT, egui::Key::Z);
        if ui
            .add_enabled(self.editor.can_redo(), egui::Button::new(tl!("Redo")))
            .on_hover_text(&redo)
            .on_disabled_hover_text(tl!("Nothing to redo."))
            .clicked()
        {
            self.editor.redo();
        }
        ui.separator();
        let selected = self.editor.selected.clone();
        if ui
            .add_enabled(selected.is_some(), egui::Button::new(tl!("New child")))
            .on_hover_text("Tab")
            .on_disabled_hover_text(tl!("Select a card first."))
            .clicked()
            && let Some(id) = &selected
        {
            self.editor.add_child(id);
        }
        if ui
            .add_enabled(selected.is_some(), egui::Button::new(tl!("Delete node")))
            .on_hover_text(key(ui, egui::Modifiers::NONE, egui::Key::Delete))
            .on_disabled_hover_text(tl!("Select a card first."))
            .clicked()
            && let Some(id) = &selected
        {
            self.editor.delete(id);
        }
        ui.separator();

        let label = self.editor.doc.file_label(tl!("An untitled map"));
        let dirty = if self.editor.doc.dirty { " *" } else { "" };
        let path = self.editor.doc.path.as_ref().map_or_else(|| tl!("An untitled map").to_string(), |p| p.display().to_string());

        // The zoom controls first (right to left), then the map name in what is left: on a narrow window the name
        // shortens with "…" instead of running under the buttons.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(tl!("Fit to window")).clicked() {
                self.editor.fit();
            }
            if ui.button("+").on_hover_text(tl!("Zoom in")).clicked() {
                self.editor.zoom_step(1.2);
            }
            ui.label(format!("{:.0}%", self.editor.view.zoom * 100.0));
            if ui.button("-").on_hover_text(tl!("Zoom out")).clicked() {
                self.editor.zoom_step(1.0 / 1.2);
            }
            ui.add_space(8.0);
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(egui::Label::new(format!("{label}{dirty}")).truncate()).on_hover_text(path);
            });
        });
    }

    /// The map itself, filling the rest of the window.
    fn page(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| self.toolbar(ui));
        ui.add_space(4.0);
        ui.separator();

        if let Some(error) = self.error.clone() {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(ui.visuals().error_fg_color, error);
                if ui.button(tl!("Dismiss")).clicked() {
                    self.error = None;
                }
            });
            ui.separator();
        }

        if self.editor.doc.map.root().is_none() {
            ui.label(tl!("Nothing open."));
            ui.add_space(6.0);
            if ui.button(tl!("Open…")).clicked() {
                self.ask_open(ui.ctx());
            }
            return;
        }

        self.editor.ui(ui);
    }

    fn new_map(&mut self) {
        self.editor.replace(edit::new_document());
        self.pending = None;
        self.error = None;
    }

    fn ask_open(&mut self, ctx: &egui::Context) {
        self.pending = Some(Pending::Open);
        if !self.pickers.ask_file(ctx, tl!("Open a mind map."), Some((tl!("Mind map"), MAP_EXTENSIONS))) {
            log::debug!("a file picker is already open");
            self.pending = None;
        }
    }

    fn ask_save_as(&mut self, ctx: &egui::Context) {
        let name = localmind_io::suggested_name(&self.editor.doc.map.title);
        self.pending = Some(Pending::SaveAs);
        if !self.pickers.ask_save_file(ctx, tl!("Save the mind map"), &name, Some((tl!("Mind map"), MAP_EXTENSIONS))) {
            log::debug!("a file picker is already open");
            self.pending = None;
        }
    }

    /// Asks where to write an export, with the map's own name and the format's extension.
    fn ask_export(&mut self, ctx: &egui::Context, kind: edit::ExportKind) {
        let stem = localmind_io::suggested_name(&self.editor.doc.map.title);
        let stem = stem.strip_suffix(".tmind").unwrap_or(&stem).to_string();
        let name = format!("{stem}.{}", kind.extension());
        self.pending = Some(Pending::Export(kind));
        if !self.pickers.ask_save_file(ctx, tl!("Export the map"), &name, Some((tl!(kind.label()), &[kind.extension()]))) {
            log::debug!("a file picker is already open");
            self.pending = None;
        }
    }

    /// Writes the map in `kind` to `path`. The file is written atomically, like a map.
    fn export_to(&mut self, path: &Path, kind: edit::ExportKind) {
        let bytes = match self.editor.export(kind) {
            Ok(bytes) => bytes,
            Err(e) => {
                self.error = Some(i18n::tr_fmt("the map could not be exported: {error}", &[("error", &e)]));
                return;
            }
        };
        match localmind_io::save_bytes(&bytes, path) {
            Ok(()) => self.error = None,
            Err(e) => self.error = Some(i18n::tr_fmt("the map could not be exported: {error}", &[("error", &e.to_string())])),
        }
    }

    /// Save, asking for a path the first time.
    fn save(&mut self, ctx: &egui::Context) {
        match self.editor.doc.path.clone() {
            Some(path) => {
                self.save_to(&path);
            }
            None => self.ask_save_as(ctx),
        }
    }

    fn save_to(&mut self, path: &Path) -> bool {
        match self.editor.doc.save(path) {
            Ok(()) => {
                self.editor.mark_saved();
                self.error = None;
                true
            }
            Err(e) => {
                self.error = Some(i18n::tr_fmt("the map could not be saved: {error}", &[("error", &e.to_string())]));
                false
            }
        }
    }

    /// A close was requested (the window's button, File → Quit, or the update restart). A dirty map
    /// shows the unsaved-changes prompt instead of closing; the OS close is cancelled by the caller.
    fn request_close(&mut self, ctx: &egui::Context) {
        if self.close.request(self.editor.doc.dirty) {
            // The modal asks; the window stays.
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    /// "Save" in the unsaved-changes prompt: save to the known path, or ask for one first.
    fn save_for_close(&mut self, ctx: &egui::Context) {
        match self.editor.doc.path.clone() {
            Some(path) => {
                let ok = self.save_to(&path);
                if self.close.saved(ok) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            None => {
                self.ask_save_as(ctx);
                if self.pending != Some(Pending::SaveAs) {
                    // The picker could not start: there is no save and no close.
                    self.close.picker_cancelled();
                }
            }
        }
    }

    /// The unsaved-changes modal, drawn last so it sits above everything.
    fn close_prompt(&mut self, ui: &mut egui::Ui) {
        if !self.close.is_confirming() {
            return;
        }
        let Some(choice) = close::prompt(ui) else { return };
        let ctx = ui.ctx().clone();
        match self.close.choose(choice) {
            Outcome::Close => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Outcome::Stay => {}
            Outcome::SaveThenClose => self.save_for_close(&ctx),
        }
    }

    fn open_path(&mut self, path: PathBuf) {
        match Document::open(&path) {
            Ok(doc) => {
                self.editor.replace(doc);
                self.error = None;
            }
            Err(e) => self.error = Some(i18n::tr_fmt("the map could not be opened: {error}", &[("error", &e.to_string())])),
        }
    }

    /// The window title says which map this is and whether it has unsaved edits.
    fn sync_window_title(&mut self, ctx: &egui::Context) {
        let label = self.editor.doc.file_label(tl!("An untitled map"));
        let dirty = if self.editor.doc.dirty { "*" } else { "" };
        let title = format!("{} — {label}{dirty}", APP.name);
        if title != self.window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.window_title = title;
        }
    }

    /// The file commands. The map's own keys (arrows, Tab, Enter, undo) are the editor's.
    fn shortcuts(&mut self, ctx: &egui::Context) {
        let (new, open, save, save_as) = ctx.input(|i| {
            let command = i.modifiers.command;
            (
                command && i.key_pressed(egui::Key::N),
                command && i.key_pressed(egui::Key::O),
                command && i.key_pressed(egui::Key::S) && !i.modifiers.shift,
                command && i.key_pressed(egui::Key::S) && i.modifiers.shift,
            )
        });
        if craft_palette::Palette::shortcut_pressed(ctx) {
            self.palette.toggle();
        }
        if new {
            self.new_map();
        }
        if open {
            self.ask_open(ctx);
        }
        if save_as {
            self.ask_save_as(ctx);
        } else if save {
            self.save(ctx);
        }
    }

    /// A map dropped on the window opens, like a file passed on the command line.
    fn take_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if let Some(path) = dropped.into_iter().next() {
            self.open_path(path);
        }
    }

    /// The language row of Settings: the label, then the selector (two grid cells).
    fn language_picker(&mut self, ui: &mut egui::Ui) {
        let label = ui.label(tl!("Language"));
        ui.horizontal(|ui| {
            let current = i18n::code();
            let combo = egui::ComboBox::from_id_salt("language").selected_text(i18n::name_of(current)).show_ui(ui, |ui| {
                for (code, name) in i18n::languages() {
                    if ui.selectable_label(code == current, name).clicked() && code != current {
                        self.prefs.language = code.to_string();
                        i18n::set_lang(i18n::resolve(&self.prefs.language));
                        self.prefs.save();
                    }
                }
            });
            combo.response.labelled_by(label.id);
            if self.prefs.language == i18n::AUTO {
                ui.label(tl!("(following the system)"));
            }
        });
    }

    /// The app-drawn title bar, when the desktop leaves the frame to the app (GNOME on Wayland;
    /// see `craft-window`).
    fn header_bar(&mut self, ui: &mut egui::Ui, layout: &craft_window::Layout) {
        egui::Panel::top("title_bar").show(ui, |ui| {
            craft_window::title_bar::empty_area(ui, ui.max_rect());
            ui.horizontal(|ui| {
                for button in &layout.left {
                    if window_button(ui, *button) {
                        self.request_close(ui.ctx());
                    }
                }
                self.menus(ui);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for button in layout.right.iter().rev() {
                        if window_button(ui, *button) {
                            self.request_close(ui.ctx());
                        }
                    }
                    self.shell_buttons(ui);
                });
            });
        });
    }

    fn about(&mut self, ui: &mut egui::Ui) {
        let mut open = self.about_open;
        egui::Window::new(tl!("About Local Mind"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ui.ctx(), |ui| {
                ui.heading(APP.name);
                ui.label(i18n::tr_fmt("version {version}", &[("version", env!("CARGO_PKG_VERSION"))]));
                ui.label(APP.tagline);
                ui.add_space(6.0);
                ui.hyperlink_to(APP.repo, APP.repo_url());
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let checking = matches!(self.updater.status(), Status::Checking);
                    if ui.add_enabled(!checking, egui::Button::new(tl!("Check for Updates…"))).clicked() {
                        self.updater.check_now(ui.ctx());
                    }
                    match self.updater.status() {
                        Status::Idle => {}
                        Status::Checking => {
                            ui.spinner();
                        }
                        Status::UpToDate => {
                            ui.label(tl!("You are on the newest version."));
                        }
                        Status::Failed(reason) => {
                            ui.label(i18n::tr_fmt("checking for updates failed: {reason}", &[("reason", reason)]));
                        }
                    };
                });
            });
        self.about_open = open;
    }

    /// Settings: the interface language and the theme (the suite keeps both here; the gear and Edit > Settings…
    /// open it).
    fn settings(&mut self, ui: &mut egui::Ui) {
        let mut open = self.settings_open;
        let mut mode = self.editor.mode;
        egui::Window::new(tl!("Settings"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ui.ctx(), |ui| {
                egui::Grid::new("settings").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                    self.language_picker(ui);
                    ui.end_row();
                    ui.label(tl!("Theme"));
                    ui.horizontal(|ui| {
                        for choice in ThemeMode::EVERY {
                            ui.selectable_value(&mut mode, choice, tl!(choice.label()));
                        }
                    });
                    ui.end_row();
                });
            });
        if mode != self.editor.mode {
            self.set_mode(ui.ctx(), mode);
        }
        self.settings_open = open;
    }
}

impl eframe::App for LocalApp {
    /// Work that is not drawing: the update check, the pickers' answers and the window commands.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.updater.check_at_start(ctx);
        self.updater.tick(ctx);

        // "System" follows the OS while the app runs.
        if self.editor.mode == ThemeMode::System {
            let theme = self.resolved_theme(ctx);
            if theme != self.editor.theme {
                self.editor.set_theme(theme);
                theme.apply(ctx);
            }
        }

        // The window wants to close: a dirty map cancels the close and shows the prompt.
        if ctx.input(|i| i.viewport().close_requested()) && self.close.request(self.editor.doc.dirty) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }

        for picked in self.pickers.take() {
            let pending = self.pending.take();
            match picked {
                Picked::File(path) => match pending {
                    Some(Pending::SaveAs) => {
                        let ok = self.save_to(&path);
                        if self.close.saved(ok) {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                    Some(Pending::Export(kind)) => self.export_to(&path, kind),
                    _ => self.open_path(path),
                },
            }
        }
        // A cancelled picker is silence: drop the pending action, and the close that was waiting
        // for that save (a save-as the user backed out of must not close the window later).
        if !self.pickers.busy()
            && let Some(cancelled) = self.pending.take()
            && cancelled == Pending::SaveAs
        {
            self.close.picker_cancelled();
        }

        self.shortcuts(ctx);
        self.take_dropped_files(ctx);
        self.sync_window_title(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }
}

impl LocalApp {
    /// The app as the headless tests build it: no `eframe` creation context, no saved preferences read or written
    /// (the caller's `prefs` are used as given; the tests never change the theme or the language, which save them).
    #[cfg(test)]
    pub(crate) fn for_test(frame: craft_window::Frame, prefs: Prefs, doc: Document, mode: ThemeMode) -> Self {
        let theme = crate::theme::Theme::resolve(mode, true);
        let mut editor = Editor::new(doc);
        editor.set_theme(theme);
        editor.mode = mode;
        Self {
            updater: Updater::new(&APP, env!("CARGO_PKG_VERSION")),
            pickers: Pickers::default(),
            frame,
            about_open: false,
            settings_open: false,
            palette: craft_palette::Palette::default(),
            prefs,
            editor,
            pending: None,
            close: close::Guard::default(),
            error: None,
            window_title: String::new(),
            control: None,
            roots: None,
        }
    }

    /// Which shell window to show, for the tests.
    #[cfg(test)]
    pub(crate) fn show_for_test(&mut self, what: Shell) {
        match what {
            Shell::About => self.about_open = true,
            Shell::Settings => self.settings_open = true,
            Shell::Palette => self.palette.open(),
            Shell::Unsaved => {
                self.editor.doc.dirty = true;
                self.request_close(&egui::Context::default());
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn language(&self) -> String {
        self.prefs.language.clone()
    }

    #[cfg(test)]
    pub(crate) fn theme(&self) -> crate::theme::Theme {
        self.editor.theme
    }

    #[cfg(test)]
    pub(crate) fn editor_mut(&mut self) -> &mut Editor {
        &mut self.editor
    }

    #[cfg(test)]
    pub(crate) fn draw_for_test(&mut self, ui: &mut egui::Ui) {
        self.draw(ui);
    }

    /// The whole window, without the `eframe::Frame` (which the headless tests do not have).
    fn draw(&mut self, ui: &mut egui::Ui) {
        // The control channel (`--control PORT`): answer what the agent asked, with the map's state.
        self.pump_control(ui.ctx());
        match self.frame.clone() {
            craft_window::Frame::App(layout) => self.header_bar(ui, &layout),
            // The desktop draws the frame; the menus still need a strip inside the window.
            craft_window::Frame::Desktop => {
                egui::Panel::top("menu_bar").show(ui, |ui| {
                    ui.horizontal(|ui| {
                        self.menus(ui);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| self.shell_buttons(ui));
                    });
                });
            }
        }

        egui::CentralPanel::default_margins().show(ui, |ui| self.page(ui));

        if self.about_open {
            self.about(ui);
        }
        if self.settings_open {
            self.settings(ui);
        }
        self.palette(ui.ctx());

        // The update prompt and its progress, drawn last so it sits above the window's content.
        match self.updater.show(ui.ctx()) {
            Action::None => {}
            Action::OpenUrl(url) => {
                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
            }
            // The update is installed: close through the normal path (the guard still asks if the
            // map is dirty), and the helper restarts us.
            Action::Close => self.request_close(ui.ctx()),
        }

        // The unsaved-changes modal is the last thing drawn, so it sits above everything.
        self.close_prompt(ui);
    }
}

/// One window button of the app-drawn header bar. Returns true when the close button was clicked:
/// the caller routes it through the unsaved-changes guard instead of closing blindly.
fn window_button(ui: &mut egui::Ui, button: craft_window::Button) -> bool {
    let (label, tooltip) = match button {
        craft_window::Button::Minimize => ("-", tl!("Minimize")),
        craft_window::Button::Maximize => ("[]", tl!("Maximize")),
        craft_window::Button::Close => ("x", tl!("Close")),
    };
    if ui.button(label).on_hover_text(tooltip).clicked() {
        match button {
            craft_window::Button::Minimize => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
            craft_window::Button::Maximize => {
                let maximized = ui.input(|i| i.viewport().maximized).unwrap_or(false);
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            }
            craft_window::Button::Close => return true,
        }
    }
    false
}
