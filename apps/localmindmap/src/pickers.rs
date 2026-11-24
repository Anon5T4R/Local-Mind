//! The native file picker, off the UI thread.
//!
//! A synchronous `rfd::FileDialog` on the UI thread re-enters winit's event handler on Windows and
//! can abort the app, so the mold is local-pdf's: `rfd::AsyncFileDialog` driven by `pollster` on a
//! worker thread, one picker at a time, the answer drained on the next frame (like
//! `updater.tick`). rfd 0.17 keeps its default backends: the native dialog on Windows, the XDG
//! portal through zbus (pure Rust) on Linux, a Zenity fallback when there is no portal.
//!
//! Opening a picker never blocks the frame, and a cancelled dialog is silence.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

/// A choice, ready to be consumed on a later frame.
pub enum Picked {
    /// A file chosen in [`Pickers::ask_file`] or [`Pickers::ask_save_file`].
    File(PathBuf),
}

/// The picker in flight, plus the choices waiting for the frame that consumes them.
#[derive(Default)]
pub struct Pickers {
    showing: Arc<AtomicBool>,
    done: Arc<Mutex<Vec<Picked>>>,
}

impl Pickers {
    /// Whether a picker is open (or its answer still waits for the next frame).
    pub fn busy(&self) -> bool {
        self.showing.load(Ordering::SeqCst)
    }

    /// Asks for one file. `title` is the dialog's title (already translated), `filter` the
    /// extensions it offers and the label of that filter. The answer arrives in [`Self::take`] on a
    /// later frame; `false` = a picker is already open (or the worker thread could not start).
    pub fn ask_file(&self, ctx: &egui::Context, title: &str, filter: Option<(&str, &[&str])>) -> bool {
        let mut dialog = rfd::AsyncFileDialog::new().set_title(title);
        if let Some((label, extensions)) = filter {
            dialog = dialog.add_filter(label, extensions);
        }
        self.run(ctx, move || pollster::block_on(dialog.pick_file()).map(|handle| Picked::File(handle.path().to_path_buf())))
    }

    /// Asks for a file to write. `file_name` is the name the dialog suggests; the answer arrives
    /// in [`Self::take`] on a later frame, exactly like [`Self::ask_file`].
    pub fn ask_save_file(&self, ctx: &egui::Context, title: &str, file_name: &str, filter: Option<(&str, &[&str])>) -> bool {
        let mut dialog = rfd::AsyncFileDialog::new().set_title(title).set_file_name(file_name);
        if let Some((label, extensions)) = filter {
            dialog = dialog.add_filter(label, extensions);
        }
        self.run(ctx, move || pollster::block_on(dialog.save_file()).map(|handle| Picked::File(handle.path().to_path_buf())))
    }

    /// Runs `dialog` on a worker thread; its result is queued, and the frame that asked is woken up.
    fn run(&self, ctx: &egui::Context, dialog: impl FnOnce() -> Option<Picked> + Send + 'static) -> bool {
        if self.busy() {
            return false;
        }
        self.showing.store(true, Ordering::SeqCst);
        let showing = self.showing.clone();
        let done = self.done.clone();
        let ctx = ctx.clone(); // an Arc inside: cheap; the thread needs 'static
        std::thread::Builder::new()
            .name("file-picker".into())
            .spawn(move || {
                if let Some(picked) = dialog() {
                    done.lock().unwrap_or_else(PoisonError::into_inner).push(picked);
                }
                showing.store(false, Ordering::SeqCst);
                ctx.request_repaint(); // wakes the frame that consumes the answer
            })
            .is_ok()
    }

    /// The choices ready so far; the frame that asks clears the queue.
    pub fn take(&self) -> Vec<Picked> {
        std::mem::take(&mut *self.done.lock().unwrap_or_else(PoisonError::into_inner))
    }
}
