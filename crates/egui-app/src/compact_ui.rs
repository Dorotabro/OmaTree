//! File ▸ Compact Notebook… (ticket E11b): inspect the unused images of the notebook, remove them
//! on an explicit confirmation, and, as a separate step with its own confirmation, give the freed
//! space back to the file system.
//!
//! Everything that matters is `omatree-core`'s: the reference scan, the report, the opaque
//! authorization tokens, the checked deletion transaction and `VACUUM`. This module only shows
//! what the core reports and passes its tokens back unchanged. It never names an image to delete,
//! never issues SQL, never saves on its own account, and nothing in it runs unless the user
//! chose it.
//!
//! The flow is one modal with explicit stages:
//!
//! ```text
//! Report ──Remove unused images…──▶ ConfirmPrune ──Remove unused images──▶ Working ──▶ PruneDone
//!   │  ╲                               │ Cancel (default)                       ╲──▶ Stale / Failed
//!   │   Save and review again          ╰──▶ back to the Report
//!   ╰──Reclaim disk space…──▶ ConfirmReclaim ──Reclaim──▶ Working ──▶ ReclaimDone / Failed
//! ```
//!
//! A stage holds the token its preview returned. Any change of the notebook between the preview
//! and the confirmation makes the core answer `Stale`; the user is told, nothing is removed, and
//! a new preview needs a new confirmation. The state is dropped when the notebook is replaced, so
//! a token can never outlive its notebook. The core calls are synchronous (one connection owns
//! the file): the confirmed operation runs on the frame after a "Working…" frame has been shown,
//! so the brief pause on a large notebook is visible and a repeated Enter or click cannot start it
//! twice.

use super::lifecycle::{dialog_buttons, dialog_frame, dialog_heading, dialog_open};
use super::*;
use omatree_core::document::PrunePreview;
use omatree_core::document::{
    PruneError, PruneResult, PruneToken, ReclaimError, ReclaimPreview, ReclaimResult, ReclaimToken,
    Tally,
};

/// A size for people: binary units, one decimal from KiB up.
pub(crate) fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    if n < 1024 {
        return format!("{n} B");
    }
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

fn images(n: usize) -> String {
    if n == 1 {
        "1 image".to_owned()
    } else {
        format!("{n} images")
    }
}

fn tally(t: &Tally) -> String {
    format!("{} ({})", images(t.count), human_bytes(t.bytes))
}

#[derive(Clone, Debug)]
pub(crate) enum Op {
    Prune(PruneToken),
    Reclaim(ReclaimToken),
}

#[derive(Clone, Debug)]
pub(crate) enum Stage {
    /// An untitled notebook has no file: nothing to prune or reclaim.
    Untitled,
    Report {
        prune: PrunePreview,
        reclaim: Option<ReclaimPreview>,
    },
    ConfirmPrune {
        prune: PrunePreview,
        reclaim: Option<ReclaimPreview>,
    },
    /// The notebook is not what was reviewed (or has unsaved changes now). Nothing was removed.
    Stale {
        why: String,
    },
    PruneDone {
        result: PruneResult,
        reclaim: Option<ReclaimPreview>,
    },
    ConfirmReclaim {
        preview: ReclaimPreview,
    },
    ReclaimDone {
        result: ReclaimResult,
    },
    Failed {
        what: String,
        why: String,
    },
    /// The confirmed operation is about to run (see the module documentation).
    Working(Op),
}

pub(crate) struct Compact {
    /// The notebook this state belongs to; a different one drops it.
    notebook: u64,
    stage: Stage,
    /// A frame showing `Working` has been painted.
    painted: bool,
    /// The stage just changed: its safe button takes the keyboard on the next frame drawn.
    fresh: bool,
}

impl Compact {
    pub(crate) fn name(&self) -> &'static str {
        match self.stage {
            Stage::Untitled => "untitled",
            Stage::Report { .. } => "report",
            Stage::ConfirmPrune { .. } => "confirm-prune",
            Stage::Stale { .. } => "stale",
            Stage::PruneDone { .. } => "prune-done",
            Stage::ConfirmReclaim { .. } => "confirm-reclaim",
            Stage::ReclaimDone { .. } => "reclaim-done",
            Stage::Failed { .. } => "failed",
            Stage::Working(_) => "working",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Act {
    None,
    Close,
    Review,
    SaveAndReview,
    AskPrune,
    ConfirmPrune,
    AskReclaim,
    ConfirmReclaim,
    BackToReport,
}

impl Host {
    // --- Opening ---------------------------------------------------------------------------

    /// File ▸ Compact Notebook…: a read-only look first. Nothing is removed or written.
    pub fn start_compact(&mut self, ctx: &egui::Context) {
        if self.doc.is_none() || self.dialog_open() || self.pick.is_some() {
            return;
        }
        // The same settling every document-level command does (a pending title and image width).
        if !self.commit_title(ctx) {
            return;
        }
        self.leave(ctx);
        let stage = self.compact_preview_stage();
        self.compact = Some(Compact {
            notebook: self.notebook,
            stage,
            painted: false,
            fresh: true,
        });
        self.open_dialog(ctx, Dialog::Compact);
    }

    /// What the compact state is doing now, for tests and diagnostics.
    pub fn compact_stage(&self) -> Option<&'static str> {
        self.compact
            .as_ref()
            .filter(|_| self.dialog == Dialog::Compact)
            .map(Compact::name)
    }

    /// A fresh review of the notebook as it is now.
    fn compact_preview_stage(&self) -> Stage {
        let Some(doc) = self.doc.as_ref() else {
            return Stage::Failed {
                what: "Nothing to compact".to_owned(),
                why: "no notebook is open".to_owned(),
            };
        };
        if !doc.has_path() {
            return Stage::Untitled;
        }
        match doc.preview_attachment_prune() {
            Ok(prune) => Stage::Report {
                prune,
                reclaim: doc.preview_reclaim().ok(),
            },
            Err(e) => Stage::Failed {
                what: "The images could not be reviewed".to_owned(),
                why: format!(
                    "{e}. Nothing was changed. (A notebook with a damaged image table is refused \
                     rather than guessed at.)"
                ),
            },
        }
    }

    fn compact_set(&mut self, stage: Stage) {
        if let Some(c) = self.compact.as_mut() {
            c.stage = stage;
            c.painted = false;
            c.fresh = true;
        }
    }

    // --- Steps -----------------------------------------------------------------------------

    /// Review again (after Stale, or after a failure): a new report, a new token.
    pub fn compact_review(&mut self, ctx: &egui::Context) {
        if self.compact.is_none() {
            return;
        }
        self.leave(ctx);
        let stage = self.compact_preview_stage();
        self.compact_set(stage);
    }

    /// Save, then review again. Saving does not authorize anything: the report that follows is
    /// new and the removal needs its own confirmation. A refused save changes nothing.
    pub fn compact_save(&mut self, ctx: &egui::Context) {
        if self.compact.is_none() {
            return;
        }
        self.save(ctx);
        if self.dialog != Dialog::Compact {
            // The save asked a question of its own (the external-modification conflict): that
            // is the established place for it, and nothing here goes on behind it.
            self.compact = None;
            return;
        }
        if self.is_dirty() {
            let why = if self.status.is_empty() {
                "the notebook could not be saved".to_owned()
            } else {
                self.status.clone()
            };
            self.compact_set(Stage::Failed {
                what: "The notebook was not saved, so nothing was reviewed or removed".to_owned(),
                why,
            });
        } else {
            let stage = self.compact_preview_stage();
            self.compact_set(stage);
        }
    }

    pub fn compact_ask_prune(&mut self) {
        let Some(Stage::Report { prune, reclaim }) = self.compact.as_ref().map(|c| c.stage.clone())
        else {
            return;
        };
        if prune.report.eligible.is_empty() || prune.report.requires_save {
            return;
        }
        self.compact_set(Stage::ConfirmPrune { prune, reclaim });
    }

    /// Cancel on a confirmation: back to the report, nothing done.
    pub fn compact_back(&mut self) {
        match self.compact.as_ref().map(|c| c.stage.clone()) {
            Some(Stage::ConfirmPrune { prune, reclaim }) => {
                self.compact_set(Stage::Report { prune, reclaim });
            }
            Some(Stage::ConfirmReclaim { .. } | Stage::PruneDone { .. }) => {
                let stage = self.compact_preview_stage();
                self.compact_set(stage);
            }
            _ => {}
        }
    }

    /// The confirmed removal. It does not run here: the stage becomes `Working`, which is
    /// painted, and the next frame runs it (once: the stage has moved on).
    pub fn compact_confirm_prune(&mut self) {
        let Some(Stage::ConfirmPrune { prune, .. }) =
            self.compact.as_ref().map(|c| c.stage.clone())
        else {
            return;
        };
        self.compact_set(Stage::Working(Op::Prune(prune.token)));
    }

    pub fn compact_ask_reclaim(&mut self) {
        let from_done = matches!(
            self.compact.as_ref().map(|c| &c.stage),
            Some(Stage::Report { .. } | Stage::PruneDone { .. })
        );
        if !from_done {
            return;
        }
        // A fresh preview of the file as it is now, with its own token.
        let preview = self.doc.as_ref().map(Document::preview_reclaim);
        match preview {
            Some(Ok(p)) if !p.nothing_to_reclaim => {
                self.compact_set(Stage::ConfirmReclaim { preview: p });
            }
            Some(Ok(_)) => {}
            Some(Err(e)) => self.compact_set(Stage::Failed {
                what: "The disk space could not be reviewed".to_owned(),
                why: e.to_string(),
            }),
            None => {}
        }
    }

    pub fn compact_confirm_reclaim(&mut self) {
        let Some(Stage::ConfirmReclaim { preview }) =
            self.compact.as_ref().map(|c| c.stage.clone())
        else {
            return;
        };
        self.compact_set(Stage::Working(Op::Reclaim(preview.token)));
    }

    /// Run the operation the user confirmed, once, on the frame after "Working…" was painted.
    pub(crate) fn compact_step(&mut self, ctx: &egui::Context) {
        let Some(c) = self.compact.as_mut() else {
            return;
        };
        let Stage::Working(op) = c.stage.clone() else {
            return;
        };
        if c.notebook != self.notebook {
            self.compact = None;
            self.cancel_dialog();
            return;
        }
        if !c.painted {
            c.painted = true;
            ctx.request_repaint();
            return;
        }
        // Taken before running: whatever happens, this operation cannot be started again.
        let Some(doc) = self.doc.as_mut() else {
            return;
        };
        let (next, status) = match op {
            Op::Prune(token) => match doc.prune_attachments(&token) {
                Ok(result) => {
                    let status = if result.wrote {
                        format!(
                            "Removed {} ({})",
                            images(result.removed.count),
                            human_bytes(result.removed.bytes)
                        )
                    } else {
                        "No unused images were removed".to_owned()
                    };
                    let reclaim = doc.preview_reclaim().ok();
                    (Some(Stage::PruneDone { result, reclaim }), status)
                }
                Err(PruneError::Stale) => (
                    Some(Stage::Stale {
                        why: "The notebook changed after the images were reviewed.".to_owned(),
                    }),
                    "Nothing was removed: the notebook changed".to_owned(),
                ),
                Err(PruneError::NeedsSave) => (
                    Some(Stage::Stale {
                        why: "The notebook has unsaved changes now.".to_owned(),
                    }),
                    "Nothing was removed: there are unsaved changes".to_owned(),
                ),
                Err(e) if e.is_conflict() => (None, "Nothing was removed.".to_owned()),
                Err(e) => (
                    Some(Stage::Failed {
                        what: "Nothing was removed".to_owned(),
                        why: e.to_string(),
                    }),
                    format!("Removing unused images failed: {e}"),
                ),
            },
            Op::Reclaim(token) => match doc.reclaim_space(&token) {
                Ok(result) => {
                    let status = if result.ran {
                        format!(
                            "Notebook file: {} to {}",
                            human_bytes(result.size_before),
                            human_bytes(result.size_after)
                        )
                    } else {
                        "There was no free space to reclaim".to_owned()
                    };
                    (Some(Stage::ReclaimDone { result }), status)
                }
                Err(ReclaimError::Stale) => (
                    Some(Stage::Stale {
                        why: "The notebook file changed after the disk space was reviewed."
                            .to_owned(),
                    }),
                    "Nothing was changed: the notebook file changed".to_owned(),
                ),
                Err(e) if e.is_conflict() => (None, "Nothing was changed.".to_owned()),
                Err(e) => (
                    Some(Stage::Failed {
                        what: "The disk space was not reclaimed. The notebook is unchanged"
                            .to_owned(),
                        why: e.to_string(),
                    }),
                    format!("Reclaiming disk space failed: {e}"),
                ),
            },
        };
        self.status = status;
        match next {
            Some(stage) => self.compact_set(stage),
            None => {
                // Someone else wrote to the file: the established conflict question, not a
                // silent no-op.
                self.compact = None;
                self.open_conflict(ctx);
            }
        }
    }

    // --- Drawing ---------------------------------------------------------------------------

    pub(crate) fn show_compact(&mut self, ctx: &egui::Context, first_frame: bool) {
        if self
            .compact
            .as_ref()
            .is_none_or(|c| c.notebook != self.notebook)
        {
            // No state, or the notebook was replaced under it: nothing to show or to authorize.
            self.compact = None;
            self.cancel_dialog();
            return;
        }
        self.compact_step(ctx);
        let Some(c) = self.compact.take() else {
            return;
        };
        let ch = *self.theme.chrome();
        let bold = self.bold.clone();
        let mut act = Act::None;
        let busy = matches!(c.stage, Stage::Working(_));
        let focus = first_frame || c.fresh;
        let m = egui::Modal::new(egui::Id::unique("dlg-compact"))
            .frame(dialog_frame(ctx))
            .show(ctx, |ui| {
                dialog_open(ui, ctx, 500.0);
                dialog_heading(ui, "Compact notebook", bold.as_ref(), &ch);
                act = draw_stage(ui, &c.stage, focus, &ch);
            });
        let mut c = c;
        c.fresh = false;
        self.compact = Some(c);
        if busy {
            // Escape and clicks do nothing while the operation is about to run.
            return;
        }
        if m.should_close() {
            act = Act::Close;
        }
        match act {
            Act::None => {}
            Act::Close => {
                self.compact = None;
                self.cancel_dialog();
            }
            Act::Review => self.compact_review(ctx),
            Act::SaveAndReview => self.compact_save(ctx),
            Act::AskPrune => self.compact_ask_prune(),
            Act::ConfirmPrune => self.compact_confirm_prune(),
            Act::AskReclaim => self.compact_ask_reclaim(),
            Act::ConfirmReclaim => self.compact_confirm_reclaim(),
            Act::BackToReport => self.compact_back(),
        }
    }
}

fn note(ui: &mut egui::Ui, text: impl Into<String>, ch: &theme::Chrome) {
    ui.label(egui::RichText::new(text.into()).size(12.5).color(ch.muted));
}

fn row(ui: &mut egui::Ui, label: &str, value: String) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(value);
        });
    });
}

fn draw_stage(ui: &mut egui::Ui, stage: &Stage, first_frame: bool, ch: &theme::Chrome) -> Act {
    let mut act = Act::None;
    match stage {
        Stage::Untitled => {
            ui.label(
                "This notebook has no file yet, so it has no stored images to review or \
                 compact.",
            );
            note(
                ui,
                "Save the notebook first (File \u{25b8} Save), then open this again.",
                ch,
            );
            dialog_buttons(ui, |ui| {
                let b = ui.button("Close");
                if first_frame {
                    b.request_focus();
                }
                if b.clicked() {
                    act = Act::Close;
                }
            });
        }
        Stage::Report { prune, reclaim } => {
            let r = &prune.report;
            row(ui, "Images stored in the notebook", tally(&r.stored));
            ui.add_space(2.0);
            note(ui, "Protected, because something still uses them:", ch);
            row(ui, "  by notes", tally(&r.in_active));
            row(ui, "  by notes in Trash", tally(&r.in_trash));
            row(ui, "  by saved checkpoints", tally(&r.in_checkpoints));
            note(
                ui,
                "One image can be in several of these groups, so they do not add up to the \
                 total. The figures below are counted once.",
                ch,
            );
            if r.pending.count > 0 {
                row(ui, "Waiting to be saved", tally(&r.pending));
            }
            ui.separator();
            row(ui, "Will stay", tally(&r.retained));
            row(ui, "Unused, can be removed", tally(&r.eligible_total));
            if let Some(size) = r.file_size_bytes {
                row(ui, "Notebook file on disk", human_bytes(size));
            }
            if r.missing_references > 0 {
                note(
                    ui,
                    format!(
                        "{} in your notes point to images that are not in this notebook.",
                        if r.missing_references == 1 {
                            "1 reference".to_owned()
                        } else {
                            format!("{} references", r.missing_references)
                        }
                    ),
                    ch,
                );
            }
            ui.separator();
            note(
                ui,
                "Images used by a note, by a note in Trash or by a saved checkpoint are never \
                 removed, so everything you can still recover keeps its pictures. Taking an \
                 image out of a note frees it only when no note, Trash entry or checkpoint \
                 still shows it.",
                ch,
            );
            note(
                ui,
                "Only references written as assets/<32 characters>.png or .jpg count. A \
                 reference that was typed or edited by hand into another form does not protect \
                 its image.",
                ch,
            );
            if r.requires_save {
                ui.label(
                    egui::RichText::new(
                        "This notebook has unsaved changes. Save it before removing images; \
                         the figures above are cautious until then.",
                    )
                    .color(ch.warning_text),
                );
            }
            let removable = !r.eligible.is_empty() && !r.requires_save;
            if r.eligible.is_empty() {
                ui.label("There are no unused images. Nothing can be removed.");
            }
            let free = reclaim.as_ref().map_or(0, |p| p.free_bytes);
            dialog_buttons(ui, |ui| {
                let close = ui.button("Close");
                if first_frame {
                    close.request_focus();
                }
                if close.clicked() {
                    act = Act::Close;
                }
                if r.requires_save && ui.button("Save and review again").clicked() {
                    act = Act::SaveAndReview;
                }
                if removable && ui.button("Remove unused images\u{2026}").clicked() {
                    act = Act::AskPrune;
                }
                if free > 0 && ui.button("Reclaim disk space\u{2026}").clicked() {
                    act = Act::AskReclaim;
                }
            });
            if free == 0 {
                note(ui, "The notebook file has no free space to give back.", ch);
            }
        }
        Stage::ConfirmPrune { prune, .. } => {
            let t = &prune.report.eligible_total;
            ui.label(format!(
                "Remove {} ({} of image data) that no note, Trash entry or checkpoint uses?",
                images(t.count),
                human_bytes(t.bytes)
            ));
            ui.label(
                "This is permanent. Images that a note, Trash or a checkpoint needs are not \
                 touched.",
            );
            note(
                ui,
                "The notebook file may not get smaller yet: SQLite keeps the freed space \
                 inside the file until you reclaim it, which is a separate step.",
                ch,
            );
            dialog_buttons(ui, |ui| {
                // The safe answer has the focus and is the only one Enter reaches first.
                let cancel = ui.button("Cancel");
                if first_frame {
                    cancel.request_focus();
                }
                if cancel.clicked() {
                    act = Act::BackToReport;
                }
                if lifecycle::destructive_button(ui, "Remove unused images", ch).clicked() {
                    act = Act::ConfirmPrune;
                }
            });
        }
        Stage::Working(op) => {
            ui.label(match op {
                Op::Prune(_) => "Removing unused images\u{2026}",
                Op::Reclaim(_) => "Reclaiming disk space\u{2026}",
            });
            note(ui, "Large notebooks can take a moment. Please wait.", ch);
        }
        Stage::Stale { why } => {
            ui.label(format!("{why} Nothing was removed or changed."));
            ui.label("Review the notebook again to see what can be removed now.");
            dialog_buttons(ui, |ui| {
                let close = ui.button("Close");
                if first_frame {
                    close.request_focus();
                }
                if close.clicked() {
                    act = Act::Close;
                }
                if ui.button("Review again").clicked() {
                    act = Act::Review;
                }
            });
        }
        Stage::PruneDone { result, reclaim } => {
            if result.wrote {
                ui.label(format!(
                    "Removed {} ({}).",
                    images(result.removed.count),
                    human_bytes(result.removed.bytes)
                ));
            } else {
                ui.label("There was nothing to remove, and nothing was written.");
            }
            row(ui, "Images that stay", tally(&result.retained));
            if let (Some(before), Some(after)) = (result.file_size_before, result.file_size_after) {
                row(
                    ui,
                    "Notebook file",
                    if before == after {
                        format!("{} (not smaller yet)", human_bytes(after))
                    } else {
                        format!("{} to {}", human_bytes(before), human_bytes(after))
                    },
                );
            }
            if result.wrote {
                note(
                    ui,
                    "SQLite keeps the freed space inside the file. To make the file smaller, \
                     reclaim disk space; you can also leave it for later.",
                    ch,
                );
            }
            let free = reclaim.as_ref().map_or(0, |p| p.free_bytes);
            dialog_buttons(ui, |ui| {
                let done = ui.button("Done");
                if first_frame {
                    done.request_focus();
                }
                if done.clicked() {
                    act = Act::Close;
                }
                if free > 0 && ui.button("Reclaim disk space\u{2026}").clicked() {
                    act = Act::AskReclaim;
                }
            });
        }
        Stage::ConfirmReclaim { preview } => {
            row(
                ui,
                "Notebook file on disk",
                human_bytes(preview.file_size_bytes),
            );
            row(
                ui,
                "Unused space inside it",
                human_bytes(preview.free_bytes),
            );
            ui.label(
                "SQLite will rebuild the notebook file to give that space back. The saving can \
                 be smaller or larger than shown.",
            );
            note(
                ui,
                "While it works it needs free disk space about the size of the notebook for \
                 the new copy, and no other program may be using the file. If anything goes \
                 wrong the notebook stays as it is.",
                ch,
            );
            dialog_buttons(ui, |ui| {
                let cancel = ui.button("Cancel");
                if first_frame {
                    cancel.request_focus();
                }
                if cancel.clicked() {
                    act = Act::BackToReport;
                }
                if ui.button("Reclaim disk space").clicked() {
                    act = Act::ConfirmReclaim;
                }
            });
        }
        Stage::ReclaimDone { result } => {
            if result.ran {
                ui.label(format!(
                    "The notebook file went from {} to {}.",
                    human_bytes(result.size_before),
                    human_bytes(result.size_after)
                ));
                note(
                    ui,
                    format!(
                        "Free space inside the file: {} before, {} after.",
                        human_bytes(result.free_bytes_before),
                        human_bytes(result.free_bytes_after)
                    ),
                    ch,
                );
            } else {
                ui.label("There was no free space to reclaim, so nothing was changed.");
            }
            dialog_buttons(ui, |ui| {
                let done = ui.button("Done");
                if first_frame {
                    done.request_focus();
                }
                if done.clicked() {
                    act = Act::Close;
                }
            });
        }
        Stage::Failed { what, why } => {
            ui.label(egui::RichText::new(what).color(ch.danger_text));
            egui::ScrollArea::vertical()
                .max_height(140.0)
                .show(ui, |ui| {
                    ui.add(egui::Label::new(why).selectable(true));
                });
            dialog_buttons(ui, |ui| {
                let close = ui.button("Close");
                if first_frame {
                    close.request_focus();
                }
                if close.clicked() {
                    act = Act::Close;
                }
                if ui.button("Review again").clicked() {
                    act = Act::Review;
                }
            });
        }
    }
    act
}
