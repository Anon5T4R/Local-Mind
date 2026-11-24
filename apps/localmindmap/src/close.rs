//! The unsaved-changes guard on window close.
//!
//! Closing a window with a dirty map must not lose the user's work: the close is cancelled, a modal
//! offers Save / Don't save / Cancel, and only a Save that actually wrote the file (or an explicit
//! discard) lets the close through. The state machine is separate from the window so it is testable
//! without an `eframe::Frame`; [`prompt`] is the modal itself, drawn with egui's `Modal`.

use crate::tl;

/// What the user picked in the prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Save,
    Discard,
    Cancel,
}

/// What the caller has to do after a choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Keep the window open (Cancel), or a failed save.
    Stay,
    /// Close now (Don't save).
    Close,
    /// Save first; close only if the save succeeds.
    SaveThenClose,
}

/// The close handshake: whether the prompt is showing, whether the close has been allowed, and
/// whether a close is waiting for a save (`Save` picked, file not written yet).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Guard {
    confirming: bool,
    allowed: bool,
    after_save: bool,
}

impl Guard {
    /// The prompt is on screen.
    pub fn is_confirming(&self) -> bool {
        self.confirming
    }

    /// The close has been allowed (the window is about to go away). Used by the tests.
    #[cfg(test)]
    pub fn is_allowed(&self) -> bool {
        self.allowed
    }

    /// A close was requested. Returns true when it must be cancelled and the prompt shown; false
    /// lets the close proceed (a clean map, or one already allowed).
    pub fn request(&mut self, dirty: bool) -> bool {
        if self.allowed || !dirty {
            return false;
        }
        self.confirming = true;
        true
    }

    /// The user picked an action in the prompt.
    pub fn choose(&mut self, choice: Choice) -> Outcome {
        self.confirming = false;
        match choice {
            Choice::Save => {
                self.after_save = true;
                Outcome::SaveThenClose
            }
            Choice::Discard => {
                self.allowed = true;
                Outcome::Close
            }
            Choice::Cancel => Outcome::Stay,
        }
    }

    /// The save the user asked for finished. Returns true when the window should now close.
    pub fn saved(&mut self, ok: bool) -> bool {
        if !self.after_save {
            return false;
        }
        self.after_save = false;
        if ok {
            self.allowed = true;
            true
        } else {
            false
        }
    }

    /// The save-as dialog was cancelled (or never opened): the pending close is dropped.
    pub fn picker_cancelled(&mut self) {
        self.after_save = false;
    }
}

/// The modal: Save / Don't save / Cancel. Returns the choice made this frame, `None` while it is
/// still open. A modal (not a floating window) so the map cannot be edited behind it.
pub fn prompt(ui: &mut egui::Ui) -> Option<Choice> {
    let mut choice = None;
    egui::Modal::new(ui.make_persistent_id("unsaved-changes")).show(ui.ctx(), |ui| {
        ui.set_max_width(420.0);
        ui.label(tl!("The map has unsaved changes."));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button(tl!("Save")).clicked() {
                choice = Some(Choice::Save);
            }
            if ui.button(tl!("Don't save")).clicked() {
                choice = Some(Choice::Discard);
            }
            if ui.button(tl!("Cancel")).clicked() {
                choice = Some(Choice::Cancel);
            }
        });
    });
    choice
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::Document;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    #[test]
    fn a_clean_map_closes_without_a_prompt() {
        let mut guard = Guard::default();
        assert!(!guard.request(false));
        assert!(!guard.is_confirming());
    }

    #[test]
    fn a_dirty_map_cancels_the_close_and_asks() {
        let mut guard = Guard::default();
        assert!(guard.request(true), "the close is cancelled");
        assert!(guard.is_confirming(), "the prompt is shown");
        assert_eq!(guard.choose(Choice::Cancel), Outcome::Stay);
        assert!(!guard.is_confirming());
        assert!(!guard.is_allowed(), "Cancel keeps the window open");
    }

    #[test]
    fn discarding_closes() {
        let mut guard = Guard::default();
        guard.request(true);
        assert_eq!(guard.choose(Choice::Discard), Outcome::Close);
        assert!(guard.is_allowed());
    }

    #[test]
    fn a_failed_save_keeps_the_window_open() {
        let mut guard = Guard::default();
        guard.request(true);
        assert_eq!(guard.choose(Choice::Save), Outcome::SaveThenClose);
        assert!(!guard.saved(false), "the file could not be written: do not close");
        assert!(!guard.is_allowed());
    }

    #[test]
    fn a_cancelled_save_as_drops_the_pending_close() {
        let mut guard = Guard::default();
        guard.request(true);
        guard.choose(Choice::Save);
        guard.picker_cancelled();
        assert!(!guard.saved(true), "there is no close waiting after a cancelled save-as");
    }

    /// The save writes the map to the file before the guard allows the close.
    #[test]
    fn saving_writes_the_file_before_closing() {
        let mut doc = Document::untitled("Teste", "Raiz");
        let root = doc.map.root_id().expect("a root").to_string();
        doc.map.set_text(&root, "Nova raiz");
        doc.dirty = true;
        let path = std::env::temp_dir().join(format!("localmindmap-close-{}.tmind", std::process::id()));
        doc.save(&path).expect("the map is written");
        assert!(path.exists(), "the file exists before the close");
        doc.dirty = false;

        let mut guard = Guard::default();
        assert!(guard.request(true), "the dirty map asked first");
        assert_eq!(guard.choose(Choice::Save), Outcome::SaveThenClose);
        assert!(guard.saved(true), "the save wrote the file: close now");
        let _ = std::fs::remove_file(&path);
    }

    /// The modal offers the three actions and reports the click (a real egui click, via kittest).
    #[test]
    fn the_prompt_offers_the_three_choices() {
        struct State {
            choice: Option<Choice>,
        }
        let mut harness = Harness::builder().with_size(egui::vec2(800.0, 600.0)).build_ui_state(
            |ui, state: &mut State| {
                if let Some(choice) = prompt(ui) {
                    state.choice = Some(choice);
                }
            },
            State { choice: None },
        );
        harness.run_steps(2);
        assert!(harness.query_by_label("Save").is_some());
        assert!(harness.query_by_label("Don't save").is_some());
        assert!(harness.query_by_label("Cancel").is_some());
        if crate::legacy::env_var_os("QA_RENDER").is_some() {
            let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/qa-f3");
            if std::fs::create_dir_all(&directory).is_ok()
                && let Ok(image) = harness.render()
            {
                let _ = image.save(directory.join("close-prompt.png"));
            }
        }
        harness.get_by_label("Don't save").click();
        harness.run_steps(2);
        assert_eq!(harness.state().choice, Some(Choice::Discard));
    }
}
