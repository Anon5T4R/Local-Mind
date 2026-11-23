//! The command palette (Ctrl+K or the search icon of the top bar): every menu command, by the same name, run through
//! the same code the menu runs. The box, the filter and the keys are the suite's `craft-palette`.

use craft_icons::ThemeMode;
use craft_palette::Command;
use egui::{Key, KeyboardShortcut, Modifiers};

use super::LocalApp;
use crate::edit;
use crate::i18n;
use crate::tl;

fn shortcut(ctx: &egui::Context, modifiers: Modifiers, key: Key) -> String {
    ctx.format_shortcut(&KeyboardShortcut::new(modifiers, key))
}

impl LocalApp {
    /// What the palette lists, in menu order.
    pub(super) fn commands(&self, ctx: &egui::Context) -> Vec<Command> {
        let (file, edit_menu, view, ai, help) = (tl!("File"), tl!("Edit"), tl!("View"), tl!("AI"), tl!("Help"));
        let selected = self.editor.selected.is_some();
        let ai_ready = selected && !self.editor.ai.busy();
        let mut list = vec![
            Command::new("file.new", tl!("New map"), file).shortcut(shortcut(ctx, Modifiers::COMMAND, Key::N)),
            Command::new("file.open", tl!("Open…"), file).shortcut(shortcut(ctx, Modifiers::COMMAND, Key::O)),
            Command::new("file.save", tl!("Save"), file).shortcut(shortcut(ctx, Modifiers::COMMAND, Key::S)),
            Command::new("file.save_as", tl!("Save As…"), file).shortcut(shortcut(ctx, Modifiers::COMMAND | Modifiers::SHIFT, Key::S)),
        ];
        for (index, kind) in edit::ExportKind::ALL.into_iter().enumerate() {
            let label = i18n::tr_fmt("Export: {format}", &[("format", tl!(kind.label()))]);
            list.push(Command::new(format!("file.export.{index}"), label, file));
        }
        list.extend([
            Command::new("file.exit", tl!("Exit"), file),
            Command::new("edit.undo", tl!("Undo"), edit_menu).shortcut(shortcut(ctx, Modifiers::COMMAND, Key::Z)).enabled(self.editor.can_undo()),
            Command::new("edit.redo", tl!("Redo"), edit_menu)
                .shortcut(shortcut(ctx, Modifiers::COMMAND | Modifiers::SHIFT, Key::Z))
                .enabled(self.editor.can_redo()),
            Command::new("edit.text", tl!("Edit text"), edit_menu).shortcut("F2").enabled(selected),
            Command::new("edit.child", tl!("New child"), edit_menu).shortcut("Tab").enabled(selected),
            Command::new("edit.sibling", tl!("New sibling"), edit_menu).shortcut("Enter").enabled(selected),
            Command::new("edit.note", tl!("Note…"), edit_menu).enabled(selected),
            Command::new("edit.move", tl!("Move to…"), edit_menu).enabled(selected),
            Command::new("edit.delete", tl!("Delete node"), edit_menu).shortcut("Delete").enabled(selected),
            Command::new("edit.settings", tl!("Settings…"), edit_menu),
            Command::new("view.fit", tl!("Fit to window"), view),
            Command::new("view.zoom_in", tl!("Zoom in"), view),
            Command::new("view.zoom_out", tl!("Zoom out"), view),
        ]);
        for mode in ThemeMode::EVERY {
            let label = i18n::tr_fmt("Theme: {mode}", &[("mode", tl!(mode.label()))]);
            list.push(Command::new(format!("view.theme.{}", mode.id()), label, view));
        }
        list.extend([
            Command::new("ai.generate", tl!("Generate map…"), ai),
            Command::new("ai.expand", tl!("Expand"), ai).enabled(ai_ready),
            Command::new("ai.expand_deep", tl!("Expand deep"), ai).enabled(ai_ready),
            Command::new("ai.explain", tl!("Explain"), ai).enabled(ai_ready),
            Command::new("ai.rewrite", tl!("Rewrite…"), ai).enabled(ai_ready),
            Command::new("ai.chat", tl!("Chat"), ai).enabled(!self.editor.ai.busy()),
            Command::new("ai.connections", tl!("Connections…"), ai),
            Command::new("help.keys", tl!("Keyboard and mouse"), help),
            Command::new("help.updates", tl!("Check for Updates…"), help),
            Command::new("help.about", tl!("About Local Mind"), help),
        ]);
        list
    }

    /// Runs what the palette returned, through the same calls as the menus.
    pub(crate) fn run_command(&mut self, ctx: &egui::Context, id: &str) {
        let selected = self.editor.selected.clone();
        match id {
            "file.new" => self.new_map(),
            "file.open" => self.ask_open(ctx),
            "file.save" => self.save(ctx),
            "file.save_as" => self.ask_save_as(ctx),
            "file.exit" => self.request_close(ctx),
            "edit.undo" => self.editor.undo(),
            "edit.redo" => self.editor.redo(),
            "edit.settings" => self.settings_open = true,
            "view.fit" => self.editor.fit(),
            "view.zoom_in" => self.editor.zoom_step(1.2),
            "view.zoom_out" => self.editor.zoom_step(1.0 / 1.2),
            "ai.generate" => self.editor.ai.generate_topic = Some(String::new()),
            "ai.chat" => {
                self.editor.ai.chat_open = true;
                self.editor.ai.load();
            }
            "ai.connections" => {
                self.editor.ai.load();
                self.editor.ai.settings_open = true;
            }
            "help.keys" => self.editor.help_open = true,
            "help.updates" => {
                // The outcome shows in the About window, like the menu's check.
                self.about_open = true;
                self.updater.check_now(ctx);
            }
            "help.about" => self.about_open = true,
            _ => {}
        }
        if let Some(index) = id.strip_prefix("file.export.").and_then(|index| index.parse::<usize>().ok())
            && let Some(kind) = edit::ExportKind::ALL.into_iter().nth(index)
        {
            self.ask_export(ctx, kind);
        }
        if let Some(mode) = id.strip_prefix("view.theme.").map(ThemeMode::from_id) {
            self.set_mode(ctx, mode);
        }
        let Some(node) = selected else { return };
        match id {
            "edit.text" => self.editor.begin_edit(&node),
            "edit.child" => self.editor.add_child(&node),
            "edit.sibling" => self.editor.add_sibling(&node),
            "edit.note" => self.editor.open_note(&node),
            "edit.move" => self.editor.open_move_to(&node),
            "edit.delete" => self.editor.delete(&node),
            "ai.expand" => self.editor.ai_expand(ctx, &node),
            "ai.expand_deep" => self.editor.ai_deep_expand(ctx, &node),
            "ai.explain" => self.editor.ai_explain(ctx, &node),
            "ai.rewrite" => self.editor.ai.rewrite = Some((node.clone(), String::new())),
            _ => {}
        }
    }
}
