//! The window: the menu bar, the toolbar strip, the map canvas, and the About window that fronts
//! the update check.
//!
//! The frame (title bar, menu strip, update prompt, file pickers) is the one the suite shares;
//! the app's own work is [`LocalApp::page`] (the mind map) and the file commands behind the menus.

use std::path::PathBuf;

use craft_update::updater::{Action, Status, Updater};

use crate::canvas::{self, CanvasView};
use crate::doc::Document;
use crate::i18n;
use crate::pickers::{Picked, Pickers};
use crate::prefs::Prefs;
use crate::suite_entry::APP;
use crate::tl;

/// The extensions the open/save dialogs offer. `json` is what the original app also accepted, and
/// its maps are plain JSON, so a `.json` a user renamed by hand still opens.
const MAP_EXTENSIONS: &[&str] = &["tmind", "json"];

/// What a pending file-picker answer is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pending {
    Open,
    SaveAs,
}

pub struct LocalApp {
    updater: Updater,
    pickers: Pickers,
    /// Who draws the window frame (probed before the window was created), so an app-drawn frame
    /// gets its header bar with the desktop's button layout.
    frame: craft_window::Frame,
    about_open: bool,
    /// The settings the app remembers between runs (the interface language).
    prefs: Prefs,
    doc: Document,
    view: CanvasView,
    selected: Option<String>,
    pending: Option<Pending>,
    /// The last failure, shown until dismissed: a map that could not be read must say so.
    error: Option<String>,
    /// The canvas rect of the last frame, for the zoom buttons and Fit.
    canvas_rect: egui::Rect,
    /// The title currently on the window, so it is only sent when it changes.
    window_title: String,
}

impl LocalApp {
    pub fn new(cc: &eframe::CreationContext<'_>, frame: craft_window::Frame, initial: Option<PathBuf>) -> Self {
        crate::fonts::install(&cc.egui_ctx);
        let prefs = Prefs::load();
        i18n::set_language(&prefs.language);

        let mut app = Self {
            updater: Updater::new(&APP, env!("CARGO_PKG_VERSION")),
            pickers: Pickers::default(),
            frame,
            about_open: false,
            prefs,
            doc: Document::untitled(tl!("An untitled map"), tl!("Central idea")),
            view: CanvasView::default(),
            selected: None,
            pending: None,
            error: None,
            canvas_rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1.0, 1.0)),
            window_title: String::new(),
        };
        app.selected = app.doc.root_id();
        if let Some(path) = initial {
            app.open_path(path);
        }
        app
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
                if ui.button(tl!("Open.")).clicked() {
                    self.ask_open(ui.ctx());
                    ui.close();
                }
                ui.separator();
                if ui.button(tl!("Save")).clicked() {
                    self.save(ui.ctx());
                    ui.close();
                }
                if ui.button(tl!("Save As.")).clicked() {
                    self.ask_save_as(ui.ctx());
                    ui.close();
                }
                ui.separator();
                if ui.button(tl!("Quit Local Mind")).clicked() {
                    quit = true;
                    ui.close();
                }
            });
            ui.menu_button(tl!("View"), |ui| {
                if ui.button(tl!("Fit to window")).clicked() {
                    self.view.needs_fit = true;
                    ui.close();
                }
                if ui.button(tl!("Zoom in")).clicked() {
                    self.zoom_step(1.2);
                    ui.close();
                }
                if ui.button(tl!("Zoom out")).clicked() {
                    self.zoom_step(1.0 / 1.2);
                    ui.close();
                }
            });
            ui.menu_button(tl!("Help"), |ui| {
                if ui.button(tl!("Check for Updates.")).clicked() {
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
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        if ui.button(tl!("New map")).clicked() {
            self.new_map();
        }
        if ui.button(tl!("Open.")).clicked() {
            self.ask_open(ui.ctx());
        }
        if ui.button(tl!("Save")).clicked() {
            self.save(ui.ctx());
        }
        if ui.button(tl!("Save As.")).clicked() {
            self.ask_save_as(ui.ctx());
        }
        ui.separator();
        let label = self.doc.file_label(tl!("An untitled map"));
        let dirty = if self.doc.dirty { " *" } else { "" };
        ui.label(format!("{label}{dirty}"))
            .on_hover_text(self.doc.path.as_ref().map_or_else(|| tl!("An untitled map").to_string(), |p| p.display().to_string()));

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(tl!("Fit to window")).clicked() {
                self.view.needs_fit = true;
            }
            if ui.button("+").on_hover_text(tl!("Zoom in")).clicked() {
                self.zoom_step(1.2);
            }
            ui.label(format!("{:.0}%", self.view.zoom * 100.0));
            if ui.button("-").on_hover_text(tl!("Zoom out")).clicked() {
                self.zoom_step(1.0 / 1.2);
            }
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

        if self.doc.map.root().is_none() {
            ui.label(tl!("Nothing open."));
            ui.add_space(6.0);
            if ui.button(tl!("Open.")).clicked() {
                self.ask_open(ui.ctx());
            }
            return;
        }

        let out = canvas::show(ui, &self.doc.map, &mut self.view, self.selected.as_deref());
        self.canvas_rect = out.rect;
        if let Some(id) = out.clicked {
            self.selected = Some(id);
        } else if out.clicked_empty {
            self.selected = None;
        }
    }

    fn zoom_step(&mut self, factor: f32) {
        self.view.zoom_around(factor, self.canvas_rect.center(), self.canvas_rect);
    }

    fn new_map(&mut self) {
        self.doc = Document::untitled(tl!("An untitled map"), tl!("Central idea"));
        self.selected = self.doc.root_id();
        self.view = CanvasView::default();
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
        let name = localmind_io::suggested_name(&self.doc.map.title);
        self.pending = Some(Pending::SaveAs);
        if !self.pickers.ask_save_file(ctx, tl!("Save the mind map"), &name, Some((tl!("Mind map"), MAP_EXTENSIONS))) {
            log::debug!("a file picker is already open");
            self.pending = None;
        }
    }

    /// Save, asking for a path the first time.
    fn save(&mut self, ctx: &egui::Context) {
        match self.doc.path.clone() {
            Some(path) => self.save_to(&path),
            None => self.ask_save_as(ctx),
        }
    }

    fn save_to(&mut self, path: &std::path::Path) {
        match self.doc.save(path) {
            Ok(()) => self.error = None,
            Err(e) => self.error = Some(i18n::tr_fmt("the map could not be saved: {error}", &[("error", &e.to_string())])),
        }
    }

    fn open_path(&mut self, path: PathBuf) {
        match Document::open(&path) {
            Ok(doc) => {
                self.doc = doc;
                self.selected = self.doc.root_id();
                self.view = CanvasView::default();
                self.error = None;
            }
            Err(e) => self.error = Some(i18n::tr_fmt("the map could not be opened: {error}", &[("error", &e.to_string())])),
        }
    }

    /// The window title says which map this is and whether it has unsaved edits.
    fn sync_window_title(&mut self, ctx: &egui::Context) {
        let label = self.doc.file_label(tl!("An untitled map"));
        let dirty = if self.doc.dirty { "*" } else { "" };
        let title = format!("{} — {label}{dirty}", APP.name);
        if title != self.window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.window_title = title;
        }
    }

    /// The keyboard: the file commands, and nothing the canvas needs.
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

    fn language_picker(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(tl!("Language"));
            let current = i18n::code();
            egui::ComboBox::from_id_salt("language").selected_text(i18n::name_of(current)).show_ui(ui, |ui| {
                for (code, name) in i18n::languages() {
                    if ui.selectable_label(code == current, name).clicked() && code != current {
                        self.prefs.language = code.to_string();
                        i18n::set_lang(i18n::resolve(&self.prefs.language));
                        self.prefs.save();
                    }
                }
            });
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
                    window_button(ui, *button);
                }
                self.menus(ui);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for button in layout.right.iter().rev() {
                        window_button(ui, *button);
                    }
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
                ui.add_space(8.0);
                self.language_picker(ui);
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let checking = matches!(self.updater.status(), Status::Checking);
                    if ui.add_enabled(!checking, egui::Button::new(tl!("Check for Updates."))).clicked() {
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
}

impl eframe::App for LocalApp {
    /// Work that is not drawing: the update check, the pickers' answers and the keyboard.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.updater.check_at_start(ctx);
        self.updater.tick(ctx);
        for picked in self.pickers.take() {
            match picked {
                Picked::File(path) => match self.pending.take() {
                    Some(Pending::SaveAs) => self.save_to(&path),
                    _ => self.open_path(path),
                },
            }
        }
        self.shortcuts(ctx);
        self.take_dropped_files(ctx);
        self.sync_window_title(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        match self.frame.clone() {
            craft_window::Frame::App(layout) => self.header_bar(ui, &layout),
            // The desktop draws the frame; the menus still need a strip inside the window.
            craft_window::Frame::Desktop => {
                egui::Panel::top("menu_bar").show(ui, |ui| self.menus(ui));
            }
        }

        egui::CentralPanel::default_margins().show(ui, |ui| self.page(ui));

        if self.about_open {
            self.about(ui);
        }

        // The update prompt and its progress, drawn last so it sits above the window's content.
        match self.updater.show(ui.ctx()) {
            Action::None => {}
            Action::OpenUrl(url) => {
                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
            }
            // The update is installed: close through the normal path, and the helper restarts us.
            Action::Close => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
        }
    }
}

/// One window button of the app-drawn header bar.
fn window_button(ui: &mut egui::Ui, button: craft_window::Button) {
    let (label, tooltip) = match button {
        craft_window::Button::Minimize => ("-", tl!("Minimize")),
        craft_window::Button::Maximize => ("[]", tl!("Maximize")),
        craft_window::Button::Close => ("x", tl!("Close")),
    };
    if ui.button(label).on_hover_text(tooltip).clicked() {
        let command = match button {
            craft_window::Button::Minimize => egui::ViewportCommand::Minimized(true),
            craft_window::Button::Maximize => {
                let maximized = ui.input(|i| i.viewport().maximized).unwrap_or(false);
                egui::ViewportCommand::Maximized(!maximized)
            }
            craft_window::Button::Close => egui::ViewportCommand::Close,
        };
        ui.ctx().send_viewport_cmd(command);
    }
}
