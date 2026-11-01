//! External modification and Reload from Disk (ticket E8b).
//!
//! A notebook file that something else changed since it was opened is never overwritten:
//! `Document::save` refuses and autosave stops. This module is how the user gets out of that
//! state without losing either version. The choices are Save As (their version goes to another
//! file; the notebook then belongs to that file and the conflict is gone), Reload from Disk (the
//! version on disk replaces the one in memory, after an explicit confirmation when there are
//! unsaved edits), or Keep editing. There is no "overwrite" answer. The conflict state is cleared
//! only by `Host::install`, that is, only when a notebook was actually loaded or saved elsewhere.

use super::lifecycle::{
    destructive_button, dialog_buttons, dialog_frame, dialog_heading, dialog_open,
};
use super::*;

impl Host {
    /// Show the conflict question (a save was refused because the file changed).
    pub(crate) fn open_conflict(&mut self, ctx: &egui::Context) {
        self.open_dialog(ctx, Dialog::Conflict);
    }

    /// File ▸ Reload from disk: replace the notebook with the file's current content. Unsaved
    /// edits are only thrown away after a confirmation; a notebook with no file has nothing to
    /// reload.
    pub fn request_reload(&mut self, ctx: &egui::Context) {
        if self.doc.is_none() || self.dialog_open() || self.pick.is_some() {
            return;
        }
        if self.path.is_none() || !self.doc.as_ref().is_some_and(|d| d.has_path()) {
            self.status = "This notebook has no file on disk to reload.".to_owned();
            return;
        }
        if !self.commit_title(ctx) {
            return;
        }
        self.leave(ctx);
        if self.is_dirty() {
            self.open_dialog(ctx, Dialog::ConfirmReload);
        } else {
            self.reload_from_disk(ctx);
        }
    }

    /// Load the file again through `Document::open_existing` and, only if that worked, make it the
    /// notebook (`install`: editor state, Search, Recovery, expansion, autosave and conflict state
    /// are all reset). The note that was selected stays selected when it still exists. A file
    /// that cannot be read leaves the notebook, its edits and the conflict exactly as they were.
    /// The file itself is never written.
    pub fn reload_from_disk(&mut self, ctx: &egui::Context) -> bool {
        let Some(path) = self.path.clone() else {
            return false;
        };
        match Document::open_existing(&path) {
            Ok(doc) => {
                let keep = self.selected;
                self.install(
                    ctx,
                    doc,
                    Some(path.clone()),
                    format!("Reloaded {} from disk", file_name(&path)),
                );
                if let Some(id) = keep.filter(|id| self.tree.contains(*id)) {
                    self.select(ctx, id);
                }
                self.warn_about_duplicates(ctx);
                true
            }
            Err(e) => {
                let message = format!(
                    "{} could not be reloaded: {} Your notebook in memory is unchanged.",
                    file_name(&path),
                    e.open_message()
                );
                self.status.clone_from(&message);
                self.focus_back = Focus::Tree;
                self.open_dialog(ctx, Dialog::Error(message));
                false
            }
        }
    }

    pub(crate) fn show_conflict(&mut self, ctx: &egui::Context, first_frame: bool) {
        enum Act {
            None,
            Keep,
            SaveAs,
            Reload,
            Discard,
        }
        let ch = *self.theme.chrome();
        let bold = self.bold.clone();
        let name = self
            .doc
            .as_ref()
            .map_or_else(String::new, Document::display_name);
        let leaving = self.pending.is_some();
        let mut act = Act::None;
        let m = egui::Modal::new(egui::Id::unique("dlg-conflict"))
            .frame(dialog_frame(ctx))
            .show(ctx, |ui| {
                dialog_open(ui, ctx, 420.0);
                dialog_heading(ui, "The file was changed elsewhere", bold.as_ref(), &ch);
                ui.label(format!(
                    "\u{201c}{name}\u{201d} was changed by something else after you opened it. \
                     If OmaTree saved now, those changes would be lost, so it has not saved and \
                     will not save by itself."
                ));
                ui.label(
                    egui::RichText::new(
                        "Save As keeps your version in a new file. Reload from Disk gives up your \
                         unsaved edits and shows what is on disk. Keep editing changes nothing.",
                    )
                    .size(12.5)
                    .color(ch.secondary),
                );
                dialog_buttons(ui, |ui| {
                    let keep = ui.button("Keep editing");
                    if first_frame {
                        keep.request_focus();
                    }
                    if keep.clicked() {
                        act = Act::Keep;
                    }
                    if ui.button("Save As\u{2026}").clicked() {
                        act = Act::SaveAs;
                    }
                    if ui.button("Reload from Disk\u{2026}").clicked() {
                        act = Act::Reload;
                    }
                    if leaving && ui.button("Discard").clicked() {
                        act = Act::Discard;
                    }
                });
            });
        if m.should_close() {
            act = Act::Keep;
        }
        match act {
            Act::None => {}
            Act::Keep => self.cancel_dialog(),
            Act::SaveAs => {
                let then = leaving;
                self.start_save_as(ctx, then);
            }
            Act::Reload => {
                // Whatever was about to happen (close, New, Open) is dropped: after a reload
                // the user starts again from the notebook they now have.
                self.pending = None;
                if self.is_dirty() {
                    self.set_dialog_fresh(Dialog::ConfirmReload);
                } else {
                    self.reload_from_disk(ctx);
                }
            }
            Act::Discard => self.unsaved_discard(),
        }
    }

    pub(crate) fn show_confirm_reload(&mut self, ctx: &egui::Context, first_frame: bool) {
        let ch = *self.theme.chrome();
        let bold = self.bold.clone();
        let name = self
            .doc
            .as_ref()
            .map_or_else(String::new, Document::display_name);
        let (mut keep, mut reload) = (false, false);
        let m = egui::Modal::new(egui::Id::unique("dlg-reload"))
            .frame(dialog_frame(ctx))
            .show(ctx, |ui| {
                dialog_open(ui, ctx, 400.0);
                dialog_heading(ui, "Reload from disk?", bold.as_ref(), &ch);
                ui.label(format!(
                    "Your unsaved changes to \u{201c}{name}\u{201d} will be lost and the notebook \
                     will show what is on disk now. This cannot be undone. To keep your version, \
                     choose Save As first."
                ));
                dialog_buttons(ui, |ui| {
                    let k = ui.button("Keep editing");
                    if first_frame {
                        k.request_focus();
                    }
                    keep = k.clicked();
                    reload = destructive_button(ui, "Reload from disk", &ch).clicked();
                });
            });
        if keep || m.should_close() {
            self.cancel_dialog();
        } else if reload {
            // Confirmed. (If the file cannot be read, the error is shown and nothing changed.)
            self.dialog = Dialog::None;
            self.reload_from_disk(ctx);
        }
    }

    fn set_dialog_fresh(&mut self, d: Dialog) {
        self.dialog = d;
        self.dialog_fresh = true;
    }
}
