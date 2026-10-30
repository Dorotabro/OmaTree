//! The essential document lifecycle of the egui shell: guarded close, Save, Save As with overwrite
//! confirmation, autosave and external-modification handling. Everything persistent goes through
//! `omatree_core::document::Document` (`save`, `save_as`); this module only sequences questions.
//!
//! Close protocol (eframe 0.37): the window's close button, Alt+F4, the compositor and egui's own
//! quit shortcut all arrive as `ViewportInfo::close_requested()`. [`Host::logic`] runs from
//! `App::logic` (also while the window is minimised), and answers a request with
//! `ViewportCommand::CancelClose` unless closing is safe. A close the host itself decides on
//! (after Save, or a confirmed Discard) is sent as `ViewportCommand::Close`, which eframe feeds
//! back as another close request, so the host remembers that it was allowed (`quit`).

use super::*;
use omatree_core::document::with_default_extension;
use omatree_core::storage::StorageError;

/// Quiet period after the last edit before an existing notebook is saved (the Qt app uses 1 s).
pub const AUTOSAVE_DELAY: f64 = 1.0;

/// The one modal question that is open, if any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dialog {
    None,
    /// Close was requested with unsaved changes. `error` is why the last Save attempt failed.
    Unsaved {
        error: Option<String>,
    },
    /// Second, explicit step before unsaved changes are thrown away.
    ConfirmDiscard,
    /// Typing the path of a new file.
    SaveAs {
        path: String,
        error: Option<String>,
        /// Carry on with the pending leave (close, New, Open) once the notebook is saved.
        then: bool,
    },
    /// The chosen file exists; replacing it needs a yes.
    Overwrite {
        path: PathBuf,
        then: bool,
    },
    /// Move this note and everything below it to Trash?
    ConfirmDelete(NodeId),
    /// Something was refused (a rename); shown until acknowledged.
    Error(String),
    /// Trash and checkpoints (`recovery_ui.rs`).
    Recovery,
    /// The keyboard reference, F1 (`help.rs`).
    Shortcuts,
    /// Typing the path of a notebook to open (used when no native file dialog is available).
    Open {
        path: String,
        error: Option<String>,
    },
    /// A native file dialog is open in another window; nothing here reacts until it answers.
    Picking,
    /// The file changed behind the notebook's back and a save was refused (`reload.rs`).
    Conflict,
    /// Replace the notebook in memory, edits included, with what is on disk?
    ConfirmReload,
    /// Export note: the format question (`note_export.rs`). `images` is how many notebook
    /// images the note refers to.
    Export {
        format: note_export::ExportFormat,
        images: usize,
    },
    /// Typing the destination of an export, with the last refusal, if any.
    ExportPath {
        format: note_export::ExportFormat,
        path: String,
        error: Option<String>,
    },
    /// Compact Notebook (`compact_ui.rs`): its state is `Host::compact`.
    Compact,
    /// The export destination exists; replacing it needs a yes.
    ExportOverwrite {
        format: note_export::ExportFormat,
        path: PathBuf,
    },
}

pub(crate) fn is_conflict(e: &DocumentError) -> bool {
    matches!(e, DocumentError::Storage(StorageError::ExternallyModified))
}

impl Host {
    pub fn dialog(&self) -> &Dialog {
        &self.dialog
    }

    /// Whether the window may go away now (a Save or confirmed Discard decided it).
    pub fn quit_allowed(&self) -> bool {
        self.quit
    }

    /// Why autosave stopped (a conflict or a failed write), shown until a save succeeds.
    pub fn autosave_problem(&self) -> Option<&str> {
        self.autosave_error.as_deref()
    }

    pub fn autosave_blocked(&self) -> bool {
        self.autosave_blocked
    }

    pub(crate) fn dialog_open(&self) -> bool {
        self.dialog != Dialog::None
    }

    /// Per-frame work that must also run while the window is hidden: close requests and autosave.
    pub fn logic(&mut self, ctx: &egui::Context) {
        self.theme.update(ctx);
        self.poll_pick(ctx);
        let now = ctx.input(|i| i.time);
        if std::mem::take(&mut self.edited) {
            self.autosave_due = Some(now + AUTOSAVE_DELAY);
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.request_close(ctx) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        self.native_quit(ctx);
        if let Some(due) = self.autosave_due {
            if now >= due {
                self.autosave_due = None;
                self.autosave(ctx);
            } else {
                // Every frame's repaint request replaces the one before it, so the wake-up for
                // the countdown is asked for again on each frame until it fires.
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(due - now + 0.02));
            }
        }
    }

    /// Where an operating-system quit request is raised (see `native_quit.rs`): the platform hook
    /// stores `true` here and the next frame's [`Host::logic`] takes it.
    pub fn native_quit_inbox(&self) -> native_quit::Inbox {
        self.quit_inbox.clone()
    }

    /// Where the final answer to such a request goes: `true` lets the application terminate.
    pub fn set_native_quit_answer(&mut self, answer: native_quit::Answer) {
        self.quit_answer = answer;
    }

    /// An operating-system quit (macOS Command+Q, the Dock's Quit, log-out) is the same request as
    /// a click on the window's close button: it goes through [`Host::request_close`], the one
    /// guard, and nothing here decides anything. What is added is the answer to the system, sent
    /// exactly once for each request that was waiting: yes when the close was allowed (a clean
    /// notebook, a successful Save, a confirmed Discard), no when the question was cancelled or
    /// could not be asked (another question is open, a pending title is refused).
    fn native_quit(&mut self, ctx: &egui::Context) {
        if self
            .quit_inbox
            .swap(false, std::sync::atomic::Ordering::AcqRel)
        {
            // The request is handled one frame later. Text typed just before the quit key is
            // already queued but is only applied to the note when this frame's `ui` runs, after
            // `logic`: judging "dirty" now would miss the last keystrokes and quit without asking.
            // (Found on the real Mac: typing and pressing Command+Q within one frame quit with the
            // last characters unsaved.) A repeated request while the question is open keeps that
            // question, as a repeated click on the close button does, and never closes.
            self.quit_waiting = true;
            self.quit_deferred = true;
            ctx.request_repaint();
            return;
        }
        if std::mem::take(&mut self.quit_deferred) {
            self.request_close(ctx);
        }
        if !self.quit_waiting {
            return;
        }
        if self.quit {
            self.quit_waiting = false;
            (self.quit_answer)(true);
        } else if self.pending != Some(Leave::Close) {
            // Nothing is waiting for the answer any more: it was cancelled (or never asked).
            self.quit_waiting = false;
            (self.quit_answer)(false);
        }
    }

    /// A close request arrived. Returns whether the window may close. Repeated requests while a
    /// question is open keep that question and never close.
    pub fn request_close(&mut self, ctx: &egui::Context) -> bool {
        if self.quit {
            return true;
        }
        // A title still being typed is part of what is saved (or asked about).
        if !self.commit_title(ctx) {
            return false;
        }
        self.leave(ctx);
        if !self.is_dirty() {
            // Nothing to ask. Which notes are open in the tree is view state: it is written with
            // any save that happens, so give it one best-effort chance (a failure, for example a
            // conflict, must not keep the window open for something that is not content).
            if let Some(doc) = self.doc.as_mut() {
                let _ = doc.save();
            }
            self.quit = true;
            return true;
        }
        let yields = matches!(
            self.dialog,
            Dialog::Recovery
                | Dialog::Shortcuts
                | Dialog::Error(_)
                | Dialog::ConfirmDelete(_)
                | Dialog::Open { .. }
        );
        if self.dialog == Dialog::None || yields {
            // A dialog that is only a view or a question about something else gives way to
            // the one about the unsaved changes; questions already about leaving or saving
            // (and a file dialog in flight) keep the keyboard, and the window stays.
            self.pending = Some(Leave::Close);
            self.open_dialog(ctx, Dialog::Unsaved { error: None });
        } else {
            self.status = "Finish or cancel the open question before closing.".to_owned();
        }
        false
    }

    pub(crate) fn open_dialog(&mut self, ctx: &egui::Context, d: Dialog) {
        // Nothing typed behind the modal may reach the note; focus returns to where it was.
        if let Some(id) = ctx.memory(|m| m.focused()) {
            if id == Host::tree_id() {
                self.focus_back = Focus::Tree;
            } else if id == Host::search_id() {
                self.focus_back = Focus::Search;
            } else if id == self.title_id() {
                self.focus_back = Focus::Title;
            } else if self
                .selected
                .is_some_and(|n| id == key_of(self.notebook, n).widget_id())
            {
                self.focus_back = Focus::Editor;
            }
            ctx.memory_mut(|m| m.surrender_focus(id));
        }
        self.set_dialog(d);
    }

    pub(crate) fn set_dialog(&mut self, d: Dialog) {
        self.dialog = d;
        self.dialog_fresh = true;
    }

    pub(crate) fn close_dialog(&mut self) {
        self.dialog = Dialog::None;
        // A Compact Notebook review (and its tokens) does not outlive its dialog.
        self.compact = None;
        self.set_focus(self.focus_back);
        if self.is_dirty() && !self.autosave_blocked && self.autosave_due.is_none() {
            self.edited = true; // resume the autosave countdown
        }
    }

    pub(crate) fn finish_close(&mut self, ctx: &egui::Context) {
        self.quit = true;
        // A confirmed Discard (or a finished Save) is the last word: the countdown that an edit
        // left running must not write the notebook again while the window is going away.
        self.autosave_due = None;
        self.dialog = Dialog::None;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    // --- Save -------------------------------------------------------------------------------

    /// Flush the editor, then `Document::save`. The only place that writes in place.
    fn persist(&mut self, ctx: &egui::Context) -> Result<(), DocumentError> {
        self.leave(ctx);
        self.doc.as_mut().ok_or(DocumentError::NoPath)?.save()
    }

    fn saved(&mut self) {
        self.autosave_blocked = false;
        self.autosave_error = None;
        self.autosave_due = None;
    }

    fn note_save_failure(&mut self, e: &DocumentError) -> String {
        let msg = e.save_message();
        if is_conflict(e) {
            // Retrying cannot succeed until the user resolves it (Save As, or reopen).
            self.autosave_blocked = true;
        }
        self.autosave_error = Some(msg.clone());
        msg
    }

    /// Ctrl+S: write the notebook. A failure is reported and nothing is lost: the document stays
    /// dirty and the buffer unchanged.
    pub fn save(&mut self, ctx: &egui::Context) {
        if !self.commit_title(ctx) {
            return;
        }
        // A notebook with no file yet is saved by choosing one.
        if self.doc.as_ref().is_some_and(|d| !d.has_path()) {
            self.start_save_as(ctx, false);
            return;
        }
        match self.persist(ctx) {
            Ok(()) => {
                self.saved();
                self.status = "Saved".to_owned();
            }
            Err(e) => {
                self.status = self.note_save_failure(&e);
                if is_conflict(&e) {
                    self.open_conflict(ctx);
                }
            }
        }
    }

    fn autosave(&mut self, ctx: &egui::Context) {
        // An untitled notebook has no file to write to: nothing is saved behind the user's back.
        let untitled = self.doc.as_ref().is_some_and(|d| !d.has_path());
        if untitled || self.quit || self.autosave_blocked || self.dialog_open() || !self.is_dirty()
        {
            return;
        }
        match self.persist(ctx) {
            Ok(()) => {
                self.saved();
                self.status = "Autosaved".to_owned();
            }
            Err(e) => {
                // Reported once; nothing is retried until the next edit (or never, for a
                // conflict), and the document stays dirty.
                let first = self.autosave_error.is_none();
                let msg = self.note_save_failure(&e);
                if first {
                    self.status = format!("Autosave failed: {msg}");
                }
            }
        }
    }

    // --- The close question ------------------------------------------------------------------

    pub fn unsaved_save(&mut self, ctx: &egui::Context) {
        // No file yet: Save means choosing one, then carrying on.
        if self.doc.as_ref().is_some_and(|d| !d.has_path()) {
            self.start_save_as(ctx, true);
            return;
        }
        match self.persist(ctx) {
            Ok(()) => {
                self.saved();
                self.status = "Saved".to_owned();
                self.continue_pending(ctx);
            }
            Err(e) => {
                let msg = self.note_save_failure(&e);
                self.status = msg.clone();
                if is_conflict(&e) {
                    // The pending action (close, New, Open) waits while the user chooses.
                    self.set_dialog(Dialog::Conflict);
                } else {
                    self.set_dialog(Dialog::Unsaved { error: Some(msg) });
                }
            }
        }
    }

    pub fn unsaved_discard(&mut self) {
        self.set_dialog(Dialog::ConfirmDiscard);
    }

    pub fn confirm_discard(&mut self, ctx: &egui::Context) {
        if self.dialog == Dialog::ConfirmDiscard {
            self.continue_pending(ctx);
        }
    }

    /// Cancel / Escape in any dialog: back to editing, document untouched.
    pub fn cancel_dialog(&mut self) {
        // Whatever was waiting for the answer (close, New, Open) is dropped with it.
        self.pending = None;
        self.close_dialog();
    }

    // --- Save As -------------------------------------------------------------------------------

    pub fn start_save_as(&mut self, ctx: &egui::Context, then: bool) {
        if !self.commit_title(ctx) {
            return;
        }
        self.leave(ctx);
        if self.doc.is_none() {
            return;
        }
        let name = self.path.as_deref().and_then(Path::file_stem).map_or_else(
            || "Untitled".to_owned(),
            |s| format!("{} copy", s.to_string_lossy()),
        );
        if self.picker.is_some() {
            self.begin_pick(
                ctx,
                PickRequest {
                    kind: PickKind::Save { then },
                    dir: self
                        .path
                        .as_deref()
                        .and_then(Path::parent)
                        .map(Path::to_path_buf),
                    name: format!("{name}.omatree"),
                },
            );
            return;
        }
        self.open_typed_save_as(ctx, then);
    }

    /// Save As by typing the path (no native dialog, or the user chose to type).
    pub(crate) fn open_typed_save_as(&mut self, ctx: &egui::Context, then: bool) {
        let dir = self
            .path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        let name = self.path.as_deref().and_then(Path::file_stem).map_or_else(
            || "Untitled".to_owned(),
            |s| format!("{} copy", s.to_string_lossy()),
        );
        let suggested = dir.join(format!("{name}.omatree"));
        self.open_dialog(
            ctx,
            Dialog::SaveAs {
                path: suggested.display().to_string(),
                error: None,
                then,
            },
        );
    }

    /// Save As from the File menu, always by typing the path.
    pub fn start_save_as_typed(&mut self, ctx: &egui::Context) {
        if self.doc.is_none() || self.dialog_open() || !self.commit_title(ctx) {
            return;
        }
        self.leave(ctx);
        self.open_typed_save_as(ctx, false);
    }

    /// The text field of the Save As dialog.
    pub fn set_save_as_path(&mut self, text: &str) {
        if let Dialog::SaveAs { path, .. } = &mut self.dialog {
            *path = text.to_owned();
        }
    }

    fn target_of(&self, typed: &str) -> Option<PathBuf> {
        let typed = typed.trim();
        if typed.is_empty() {
            return None;
        }
        let p = with_default_extension(Path::new(typed));
        if p.is_absolute() {
            return Some(p);
        }
        let base = self
            .path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())?;
        Some(base.join(p))
    }

    /// Save (Enter) in the Save As dialog.
    pub fn submit_save_as(&mut self, ctx: &egui::Context) {
        let Dialog::SaveAs { path, then, .. } = self.dialog.clone() else {
            return;
        };
        self.submit_save_target(ctx, &path, then);
    }

    /// Save As to `typed` (what was typed, or the path a file dialog returned): the extension is
    /// added when missing, an existing file other than this notebook's own needs a yes, and
    /// `Document::save_as` does the rest (including refusing to overwrite a file that is not a
    /// notebook).
    pub(crate) fn submit_save_target(&mut self, ctx: &egui::Context, typed: &str, then: bool) {
        let Some(target) = self.target_of(typed) else {
            self.set_dialog(Dialog::SaveAs {
                path: typed.to_owned(),
                error: Some("Enter a file name.".to_owned()),
                then,
            });
            return;
        };
        let needs = self
            .doc
            .as_ref()
            .is_some_and(|d| d.save_as_needs_confirmation(&target));
        if needs {
            self.set_dialog(Dialog::Overwrite { path: target, then });
        } else {
            self.finish_save_as(ctx, target, false, then);
        }
    }

    pub fn confirm_overwrite(&mut self, ctx: &egui::Context) {
        if let Dialog::Overwrite { path, then } = self.dialog.clone() {
            self.finish_save_as(ctx, path, true, then);
        }
    }

    fn finish_save_as(
        &mut self,
        ctx: &egui::Context,
        target: PathBuf,
        overwrite: bool,
        then: bool,
    ) {
        self.leave(ctx);
        let Some(doc) = self.doc.as_mut() else { return };
        match doc.save_as(&target, overwrite) {
            Ok(()) => {
                self.path = Some(target.clone());
                self.saved();
                self.status = format!("Saved as {}", file_name(&target));
                if then {
                    self.continue_pending(ctx);
                } else {
                    self.close_dialog();
                }
            }
            Err(e) => {
                let msg = e.save_as_message();
                self.status = msg.clone();
                self.set_dialog(Dialog::SaveAs {
                    path: target.display().to_string(),
                    error: Some(msg),
                    then,
                });
            }
        }
    }

    // --- Drawing -------------------------------------------------------------------------------

    /// Draws the open question, if any, and applies the answer.
    pub(crate) fn show_dialog(&mut self, ctx: &egui::Context) {
        enum Answer {
            None,
            Save,
            SaveAs,
            Discard,
            ConfirmDiscard,
            Cancel,
            Submit(String),
            Overwrite,
            Delete(NodeId),
            Acknowledge,
        }
        let name = self
            .doc
            .as_ref()
            .map_or_else(String::new, Document::display_name);
        let ch = *self.theme.chrome();
        let bold = self.bold.clone();
        let mut answer = Answer::None;
        let first_frame = std::mem::take(&mut self.dialog_fresh);
        match self.dialog.clone() {
            Dialog::None => return,
            Dialog::Recovery => {
                self.show_recovery(ctx, first_frame);
                return;
            }
            Dialog::Shortcuts => {
                self.show_shortcuts(ctx, first_frame);
                return;
            }
            Dialog::Open { .. } => {
                self.show_open_path(ctx, first_frame);
                return;
            }
            Dialog::Picking => {
                self.show_picking(ctx, first_frame);
                return;
            }
            Dialog::Conflict => {
                self.show_conflict(ctx, first_frame);
                return;
            }
            Dialog::ConfirmReload => {
                self.show_confirm_reload(ctx, first_frame);
                return;
            }
            Dialog::Compact => {
                self.show_compact(ctx, first_frame);
                return;
            }
            Dialog::Export { .. } => {
                self.show_export(ctx, first_frame);
                return;
            }
            Dialog::ExportPath { .. } => {
                self.show_export_path(ctx, first_frame);
                return;
            }
            Dialog::ExportOverwrite { .. } => {
                self.show_export_overwrite(ctx, first_frame);
                return;
            }
            Dialog::Unsaved { error } => {
                let m = egui::Modal::new(egui::Id::unique("dlg-unsaved"))
                    .frame(dialog_frame(ctx))
                    .show(ctx, |ui| {
                        dialog_open(ui, ctx, 340.0);
                        dialog_heading(ui, "Unsaved changes", bold.as_ref(), &ch);
                        ui.label(format!("\u{201c}{name}\u{201d} has unsaved changes."));
                        if let Some(e) = &error {
                            ui.colored_label(ui.visuals().error_fg_color, e);
                        }
                        dialog_buttons(ui, |ui| {
                            let save = ui.button("Save");
                            if first_frame {
                                save.request_focus();
                            }
                            if save.clicked() {
                                answer = Answer::Save;
                            }
                            if error.is_some() && ui.button("Save As\u{2026}").clicked() {
                                answer = Answer::SaveAs;
                            }
                            if ui.button("Discard").clicked() {
                                answer = Answer::Discard;
                            }
                            if ui.button("Cancel").clicked() {
                                answer = Answer::Cancel;
                            }
                        });
                    });
                if m.should_close() {
                    answer = Answer::Cancel;
                }
            }
            Dialog::ConfirmDiscard => {
                let m = egui::Modal::new(egui::Id::unique("dlg-discard"))
                    .frame(dialog_frame(ctx))
                    .show(ctx, |ui| {
                        dialog_open(ui, ctx, 340.0);
                        dialog_heading(ui, "Discard changes?", bold.as_ref(), &ch);
                        ui.label("Your unsaved changes will be lost. This cannot be undone.");
                        dialog_buttons(ui, |ui| {
                            // The safe choice has the focus.
                            let keep = ui.button("Keep editing");
                            if first_frame {
                                keep.request_focus();
                            }
                            if keep.clicked() {
                                answer = Answer::Cancel;
                            }
                            if destructive_button(ui, "Discard changes", &ch).clicked() {
                                answer = Answer::ConfirmDiscard;
                            }
                        });
                    });
                if m.should_close() {
                    answer = Answer::Cancel;
                }
            }
            Dialog::SaveAs { path, error, .. } => {
                let mut typed = path;
                let m = egui::Modal::new(egui::Id::unique("dlg-saveas"))
                    .frame(dialog_frame(ctx))
                    .show(ctx, |ui| {
                        dialog_open(ui, ctx, 460.0);
                        dialog_heading(ui, "Save notebook as", bold.as_ref(), &ch);
                        ui.label("File path (.omatree is added when there is no extension):");
                        let edit = ui.add(
                            egui::TextEdit::singleline(&mut typed)
                                .desired_width(f32::INFINITY)
                                .id(egui::Id::unique("dlg-saveas-path")),
                        );
                        if first_frame {
                            edit.request_focus();
                        }
                        let enter =
                            edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if let Some(e) = &error {
                            ui.colored_label(ui.visuals().error_fg_color, e);
                        }
                        dialog_buttons(ui, |ui| {
                            if ui.button("Save").clicked() || enter {
                                answer = Answer::Submit(typed.clone());
                            }
                            if ui.button("Cancel").clicked() {
                                answer = Answer::Cancel;
                            }
                        });
                    });
                self.set_save_as_path(&typed);
                if m.should_close() {
                    answer = Answer::Cancel;
                }
            }
            Dialog::ConfirmDelete(id) => {
                let text = self
                    .doc
                    .as_ref()
                    .and_then(|d| d.notebook().get(id))
                    .map(|n| {
                        let branch = self.tree.row(id).is_some_and(|r| r.has_children);
                        if branch {
                            format!(
                                "Move \u{201c}{}\u{201d} and everything beneath it to Trash?",
                                n.title()
                            )
                        } else {
                            format!("Move \u{201c}{}\u{201d} to Trash?", n.title())
                        }
                    });
                let m = egui::Modal::new(egui::Id::unique("dlg-delete"))
                    .frame(dialog_frame(ctx))
                    .show(ctx, |ui| {
                        dialog_open(ui, ctx, 340.0);
                        dialog_heading(ui, "Move to Trash", bold.as_ref(), &ch);
                        ui.label(text.unwrap_or_default());
                        dialog_buttons(ui, |ui| {
                            // The safe answer has the focus.
                            let no = ui.button("Cancel");
                            if first_frame {
                                no.request_focus();
                            }
                            if no.clicked() {
                                answer = Answer::Cancel;
                            }
                            if destructive_button(ui, "Move to Trash", &ch).clicked() {
                                answer = Answer::Delete(id);
                            }
                        });
                    });
                if m.should_close() {
                    answer = Answer::Cancel;
                }
            }
            Dialog::Error(message) => {
                let m = egui::Modal::new(egui::Id::unique("dlg-error"))
                    .frame(dialog_frame(ctx))
                    .show(ctx, |ui| {
                        dialog_open(ui, ctx, 340.0);
                        dialog_heading(ui, "OmaTree", bold.as_ref(), &ch);
                        ui.label(&message);
                        dialog_buttons(ui, |ui| {
                            let ok = ui.button("OK");
                            if first_frame {
                                ok.request_focus();
                            }
                            if ok.clicked() {
                                answer = Answer::Acknowledge;
                            }
                        });
                    });
                if m.should_close() {
                    answer = Answer::Acknowledge;
                }
            }
            Dialog::Overwrite { path, .. } => {
                let m = egui::Modal::new(egui::Id::unique("dlg-overwrite"))
                    .frame(dialog_frame(ctx))
                    .show(ctx, |ui| {
                        dialog_open(ui, ctx, 340.0);
                        dialog_heading(ui, "Replace file", bold.as_ref(), &ch);
                        ui.label(format!(
                            "\u{201c}{}\u{201d} already exists. Replace it with this notebook?",
                            file_name(&path)
                        ));
                        dialog_buttons(ui, |ui| {
                            let no = ui.button("Cancel");
                            if first_frame {
                                no.request_focus();
                            }
                            if no.clicked() {
                                answer = Answer::Cancel;
                            }
                            if destructive_button(ui, "Yes, replace", &ch).clicked() {
                                answer = Answer::Overwrite;
                            }
                        });
                    });
                if m.should_close() {
                    answer = Answer::Cancel;
                }
            }
        }
        match answer {
            Answer::None => {}
            Answer::Save => self.unsaved_save(ctx),
            Answer::SaveAs => self.start_save_as(ctx, true),
            Answer::Discard => self.unsaved_discard(),
            Answer::ConfirmDiscard => self.confirm_discard(ctx),
            Answer::Cancel => self.cancel_dialog(),
            Answer::Submit(p) => {
                self.set_save_as_path(&p);
                self.submit_save_as(ctx);
            }
            Answer::Overwrite => self.confirm_overwrite(ctx),
            Answer::Delete(id) => self.confirm_delete(ctx, id),
            Answer::Acknowledge => self.cancel_dialog(),
        }
    }
}

// --- Dialog presentation (E6b) --------------------------------------------------------------

/// Shared look of every modal: one width rule, one rhythm, one heading style.
pub(crate) fn dialog_frame(ctx: &egui::Context) -> egui::Frame {
    egui::Frame::popup(&ctx.global_style()).inner_margin(egui::Margin::same(18))
}

pub(crate) fn dialog_open(ui: &mut egui::Ui, ctx: &egui::Context, width: f32) {
    ui.set_width(width.min((ctx.content_rect().width() - 72.0).max(200.0)));
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 10.0);
    ui.spacing_mut().button_padding = egui::vec2(14.0, 5.0);
}

pub(crate) fn dialog_heading(
    ui: &mut egui::Ui,
    text: &str,
    bold: Option<&egui::FontFamily>,
    ch: &theme::Chrome,
) {
    let font = bold.map_or_else(
        || egui::FontId::proportional(17.0),
        |f| egui::FontId::new(17.0, f.clone()),
    );
    ui.label(egui::RichText::new(text).font(font).color(ch.text));
}

/// The row of answers, a little apart from the text above.
pub(crate) fn dialog_buttons<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.add_space(4.0);
    ui.horizontal(add).inner
}

/// A destructive answer: the danger fill, a label that reads on it (derived from the palette,
/// see `theme::Chrome::on_danger`), and words that say what will happen, so colour is not the
/// only cue.
pub(crate) fn destructive_button(
    ui: &mut egui::Ui,
    label: &str,
    ch: &theme::Chrome,
) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(label).color(ch.on_danger))
            .fill(ch.danger_fill)
            .stroke(egui::Stroke::NONE),
    )
}
