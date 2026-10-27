//! The Recovery dialog: Trash and checkpoints of the open notebook (ticket E7b).
//!
//! Everything that changes the notebook is `Document::restore_trash` / `restore_checkpoint`; this
//! module lists what the document retains (newest first, as the Qt app does), asks the questions
//! and puts the editor, tree and Search in step with the result. It never rebuilds a deleted note
//! itself and never touches storage.
//!
//! Editor state after a restore (see `Host::restore_checkpoint`): a Trash restore adds notes, so
//! nothing that exists is touched; the restored notes start with no caret or undo history. A
//! checkpoint restore replaces the notebook, so the state of every note whose body is not
//! byte-identical afterwards (and of every note that is gone) is forgotten: Ctrl+Z can never bring
//! back text that only existed before the restore.

use super::layout::GUTTER;
use super::lifecycle::{
    destructive_button, dialog_buttons, dialog_frame, dialog_heading, dialog_open,
};
use super::*;
use egui::{Color32, FontId, Key, Pos2, Rect, Sense, Stroke, Vec2};
use omatree_core::notebook::NodeId as Id;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

const ROW_TRASH: f32 = 64.0;
const ROW_CHECKPOINT: f32 = 46.0;
const LIST_MAX: f32 = 300.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Trash,
    Checkpoints,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tone {
    Secondary,
    Muted,
    Danger,
}

struct Line {
    text: String,
    tone: Tone,
}

/// What a row shows. Derived from the document when the dialog opens and after each restore,
/// never per frame.
struct Row {
    title: String,
    lines: Vec<Line>,
}

#[derive(Default)]
pub(crate) struct RecoveryUi {
    tab: Tab,
    selected: [usize; 2],
    rows: [Vec<Row>; 2],
    /// A checkpoint (as listed, newest first) waiting for the user to confirm the replacement.
    confirm: Option<usize>,
    /// Why the last restore was refused, shown inside the dialog.
    error: Option<String>,
    scroll_to_selected: bool,
}

impl RecoveryUi {
    fn index(tab: Tab) -> usize {
        match tab {
            Tab::Trash => 0,
            Tab::Checkpoints => 1,
        }
    }

    fn current(&self) -> usize {
        Self::index(self.tab)
    }

    fn step(&mut self, delta: isize) {
        let i = self.current();
        let n = self.rows[i].len();
        if n == 0 {
            return;
        }
        self.selected[i] = (self.selected[i] as isize + delta).clamp(0, n as isize - 1) as usize;
        self.scroll_to_selected = true;
    }

    fn switch(&mut self, tab: Tab) {
        if self.tab != tab {
            self.tab = tab;
            self.selected[Self::index(tab)] = 0;
            self.error = None;
        }
    }
}

/// `1970-01-01 00:00 UTC` for 0. The notebook stores UTC seconds and the standard library has no
/// time zones, so the zone is named rather than guessed.
pub(crate) fn format_utc(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (hour, minute) = (rem / 3600, rem % 3600 / 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02} UTC")
}

fn hash_of(text: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

fn notes(n: usize) -> String {
    if n == 1 {
        "1 note".to_owned()
    } else {
        format!("{n} notes")
    }
}

impl Host {
    // --- Opening ---------------------------------------------------------------------------

    /// Show Recovery. The title being typed and the editor's buffer are settled first, so the
    /// notebook the lists describe (and the checkpoint a restore takes) holds everything the user
    /// has written; a title the notebook refuses blocks it, with the usual explanation.
    pub fn open_recovery(&mut self, ctx: &egui::Context) {
        if self.doc.is_none() || self.dialog_open() || !self.commit_title(ctx) {
            return;
        }
        self.leave(ctx);
        self.recovery = RecoveryUi::default();
        self.rebuild_recovery_rows();
        self.open_dialog(ctx, Dialog::Recovery);
    }

    pub fn recovery_tab(&self) -> Tab {
        self.recovery.tab
    }

    /// How many entries each list shows: (Trash, checkpoints).
    pub fn recovery_counts(&self) -> (usize, usize) {
        (self.recovery.rows[0].len(), self.recovery.rows[1].len())
    }

    /// The titles listed on the current tab, newest first.
    pub fn recovery_titles(&self) -> Vec<String> {
        self.recovery.rows[self.recovery.current()]
            .iter()
            .map(|r| r.title.clone())
            .collect()
    }

    pub fn recovery_selected(&self) -> Option<usize> {
        let i = self.recovery.current();
        (!self.recovery.rows[i].is_empty()).then_some(self.recovery.selected[i])
    }

    pub fn recovery_error(&self) -> Option<&str> {
        self.recovery.error.as_deref()
    }

    /// Whether a checkpoint is waiting for its replacement to be confirmed.
    pub fn recovery_confirming(&self) -> bool {
        self.recovery.confirm.is_some()
    }

    /// The path of `id` as "A › B › id".
    fn path_of(&self, id: Id) -> String {
        let mut titles = Vec::new();
        let mut at = Some(id);
        while let Some(r) = at.and_then(|i| self.tree.row(i)) {
            titles.push(r.title.as_str());
            at = r.parent;
            if titles.len() > self.tree.rows().len() {
                break;
            }
        }
        titles.reverse();
        titles.join(" \u{203a} ")
    }

    fn rebuild_recovery_rows(&mut self) {
        let Some(doc) = self.doc.as_ref() else { return };
        let rec = doc.recovery();
        let n = rec.trash().len();
        let trash = (0..n)
            .rev()
            .map(|stored| {
                let e = &rec.trash()[stored];
                let comes_back = match doc.plan_trash_restore(stored) {
                    Ok((Some(parent), _)) => Line {
                        text: format!("Returns to {}", self.path_of(parent)),
                        tone: Tone::Muted,
                    },
                    Ok((None, _)) if e.root().parent_id().is_some() => Line {
                        text: "Returns to the top level (its old place is gone)".to_owned(),
                        tone: Tone::Muted,
                    },
                    Ok((None, _)) => Line {
                        text: "Returns to the top level".to_owned(),
                        tone: Tone::Muted,
                    },
                    Err(err) => Line {
                        text: err.message().to_owned(),
                        tone: Tone::Danger,
                    },
                };
                Row {
                    title: e.title().to_owned(),
                    lines: vec![
                        Line {
                            text: format!(
                                "Deleted {} \u{b7} {}",
                                format_utc(e.deleted_at()),
                                notes(e.node_count())
                            ),
                            tone: Tone::Secondary,
                        },
                        comes_back,
                    ],
                }
            })
            .collect();
        let checkpoints = rec
            .checkpoints()
            .iter()
            .rev()
            .map(|c| Row {
                title: c.reason().to_owned(),
                lines: vec![Line {
                    text: format_utc(c.created_at()),
                    tone: Tone::Secondary,
                }],
            })
            .collect();
        let r = &mut self.recovery;
        r.rows = [trash, checkpoints];
        for i in 0..2 {
            let last = r.rows[i].len().saturating_sub(1);
            r.selected[i] = r.selected[i].min(last);
        }
    }

    // --- Restoring ---------------------------------------------------------------------------

    /// After a restore: the user is told once about duplicate sibling names the notebook may
    /// contain (older checkpoints can hold them), as the Qt app does.
    fn warn_about_duplicates(&mut self, ctx: &egui::Context) {
        let dup = self
            .doc
            .as_ref()
            .is_some_and(|d| d.notebook().sibling_title_conflicts() > 0);
        if dup {
            self.focus_back = Focus::Tree;
            self.open_dialog(
                ctx,
                Dialog::Error(
                    "This notebook contains duplicate note names under the same parent.\n\
                     Nothing was changed. Rename the duplicates to resolve them.\n\
                     OmaTree will prevent new duplicates."
                        .to_owned(),
                ),
            );
        }
    }

    fn refuse(&mut self, message: String) -> bool {
        self.status.clone_from(&message);
        self.recovery.error = Some(message);
        false
    }

    /// Restore Trash entry `shown` (0 = the newest) with everything beneath it. The note is
    /// selected and revealed. On a refusal nothing changes and the entry stays in Trash.
    pub fn restore_trash(&mut self, ctx: &egui::Context, shown: usize) -> bool {
        if !self.commit_title(ctx) {
            return false;
        }
        self.leave(ctx);
        let Some(doc) = self.doc.as_mut() else {
            return false;
        };
        let len = doc.recovery().trash().len();
        let Some(stored) = len.checked_sub(shown + 1) else {
            return self.refuse(RecoveryError::NoSuchEntry.message().to_owned());
        };
        let title = doc.recovery().trash()[stored].title().to_owned();
        match doc.restore_trash(stored) {
            Ok(root) => {
                self.edited = true;
                self.rebuild_tree();
                // The notes come back as new notes to the editor: no caret, no undo history.
                for id in self.tree.subtree_ids(root) {
                    self.view.forget_note(ctx, key_of(self.notebook, id));
                }
                self.recovery.error = None;
                self.rebuild_recovery_rows();
                self.select(ctx, root);
                self.status = format!("Restored \u{201c}{title}\u{201d} from Trash");
                self.finish_restore(ctx);
                true
            }
            Err(e) => self.refuse(e.message().to_owned()),
        }
    }

    /// Replace the notebook with checkpoint `shown` (0 = the newest). The core checkpoints what
    /// it replaces first. Everything the editor remembers about notes that changed or vanished is
    /// forgotten; the selection is kept when that note still exists.
    pub fn restore_checkpoint(&mut self, ctx: &egui::Context, shown: usize) -> bool {
        if !self.commit_title(ctx) {
            return false;
        }
        self.leave(ctx);
        let Some(doc) = self.doc.as_mut() else {
            return false;
        };
        let len = doc.recovery().checkpoints().len();
        let Some(stored) = len.checked_sub(shown + 1) else {
            return self.refuse(RecoveryError::NoSuchEntry.message().to_owned());
        };
        let reason = doc.recovery().checkpoints()[stored].reason().to_owned();
        let before: HashMap<Id, u64> = doc
            .notebook()
            .depth_first()
            .iter()
            .map(|n| (n.id(), hash_of(n.body())))
            .collect();
        if let Err(e) = doc.restore_checkpoint(stored) {
            return self.refuse(e.message().to_owned());
        }
        self.edited = true;
        let after: HashMap<Id, u64> = self
            .doc
            .as_ref()
            .map(|d| {
                d.notebook()
                    .depth_first()
                    .iter()
                    .map(|n| (n.id(), hash_of(n.body())))
                    .collect()
            })
            .unwrap_or_default();
        for id in before.keys().chain(after.keys()) {
            if !before.contains_key(id) || before.get(id) != after.get(id) {
                self.view.forget_note(ctx, key_of(self.notebook, *id));
            }
        }
        self.rebuild_tree();
        self.recovery.error = None;
        self.rebuild_recovery_rows();
        // The selection survives only as the same note; its text and title are read again.
        let keep = self.selected.filter(|id| self.tree.contains(*id));
        self.selected = None;
        self.text.clear();
        self.title_text.clear();
        self.loaded_title.clear();
        if let Some(id) = keep.or_else(|| self.tree.rows().first().map(|r| r.id)) {
            self.load(id);
            self.reveal(id);
        }
        self.status = format!("Restored checkpoint \u{201c}{reason}\u{201d}");
        self.finish_restore(ctx);
        true
    }

    fn finish_restore(&mut self, ctx: &egui::Context) {
        self.recovery.confirm = None;
        self.close_dialog();
        self.set_focus(Focus::Tree);
        self.warn_about_duplicates(ctx);
    }

    // --- The dialog ----------------------------------------------------------------------------

    pub(crate) fn recovery_list_id() -> egui::Id {
        egui::Id::unique("recovery-list")
    }

    pub(crate) fn recovery_tabs_id() -> egui::Id {
        egui::Id::unique("recovery-tabs")
    }

    pub(crate) fn show_recovery(&mut self, ctx: &egui::Context, first_frame: bool) {
        enum Act {
            None,
            Close,
            Restore(usize),
            Confirm(usize),
            Back,
        }
        let ch = *self.theme.chrome();
        let bold = self.bold.clone();
        let mut act = Act::None;
        let list_id = Host::recovery_list_id();
        let tabs_id = Host::recovery_tabs_id();
        let confirm = self.recovery.confirm;
        let tab = self.recovery.tab;
        let max_list = (ctx.content_rect().height() - 330.0).clamp(ROW_CHECKPOINT * 2.0, LIST_MAX);
        let m = egui::Modal::new(egui::Id::unique("dlg-recovery"))
            .frame(dialog_frame(ctx))
            .show(ctx, |ui| {
                dialog_open(ui, ctx, 480.0);
                if let Some(shown) = confirm {
                    let (reason, when) = self
                        .doc
                        .as_ref()
                        .map(|d| {
                            let list = d.recovery().checkpoints();
                            list.len()
                                .checked_sub(shown + 1)
                                .map(|i| (list[i].reason().to_owned(), list[i].created_at()))
                                .unwrap_or_default()
                        })
                        .unwrap_or_default();
                    dialog_heading(ui, "Replace the notebook?", bold.as_ref(), &ch);
                    ui.label(format!(
                        "The whole notebook will be replaced with the checkpoint \u{201c}{reason}\u{201d} \
                         from {}.",
                        format_utc(when)
                    ));
                    ui.label(
                        egui::RichText::new(
                            "Notes created or changed since then leave the notebook. What is replaced is \
                             checkpointed first (unsaved edits included), so this can be undone from here.",
                        )
                        .color(ch.secondary),
                    );
                    dialog_buttons(ui, |ui| {
                        let cancel = ui.button("Cancel");
                        if first_frame || ui.memory(|m| m.focused().is_none()) {
                            cancel.request_focus();
                        }
                        if cancel.clicked() {
                            act = Act::Back;
                        }
                        if destructive_button(ui, "Replace notebook", &ch).clicked() {
                            act = Act::Confirm(shown);
                        }
                    });
                    return;
                }
                dialog_heading(ui, "Recovery", bold.as_ref(), &ch);
                // Tabs: one stop for Tab; Left / Right choose.
                let counts = self.recovery_counts();
                let labels = [
                    format!("Trash ({})", counts.0),
                    format!("Checkpoints ({})", counts.1),
                ];
                let (head, _) = ui
                    .allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::hover());
                let head_resp = ui.interact(head, tabs_id, Sense::click());
                head_resp.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::TabList, true, "Recovery views")
                });
                let mut x = head.left();
                let mut spans = [(0.0, 0.0); 2];
                for (i, label) in labels.iter().enumerate() {
                    let on = (i == 0) == (tab == Tab::Trash);
                    let colour = if on { ch.text } else { ch.muted };
                    let g = ui.painter().layout_no_wrap(
                        label.to_uppercase(),
                        FontId::monospace(12.5),
                        colour,
                    );
                    let w = g.size().x + 20.0;
                    spans[i] = (x, x + w);
                    ui.painter().galley(
                        Pos2::new(x + 10.0, head.center().y - g.size().y / 2.0),
                        g,
                        colour,
                    );
                    if on {
                        ui.painter().rect_filled(
                            Rect::from_min_size(
                                Pos2::new(x, head.bottom() - 2.0),
                                Vec2::new(w, 2.0),
                            ),
                            0.0,
                            ch.accent,
                        );
                    }
                    x += w;
                }
                ui.painter().hline(
                    head.x_range(),
                    head.bottom() - 0.5,
                    Stroke::new(1.0, ch.hairline),
                );
                if head_resp.has_focus() {
                    ui.painter().rect_stroke(
                        head.shrink(1.0),
                        3.0,
                        Stroke::new(1.0, ch.accent),
                        egui::StrokeKind::Inside,
                    );
                    ui.memory_mut(|m| {
                        m.set_focus_lock_filter(
                            tabs_id,
                            egui::EventFilter {
                                horizontal_arrows: true,
                                ..Default::default()
                            },
                        );
                    });
                }
                if head_resp.clicked() {
                    if let Some(p) = head_resp.interact_pointer_pos() {
                        self.recovery.switch(if p.x < spans[0].1 {
                            Tab::Trash
                        } else {
                            Tab::Checkpoints
                        });
                    }
                    head_resp.request_focus();
                }
                let tab = self.recovery.tab;
                let now = self.recovery.current();
                ui.label(
                    egui::RichText::new(match tab {
                        Tab::Trash => {
                            "Restoring puts the note and everything beneath it back in the \
                             notebook, where it was if that place still exists."
                        }
                        Tab::Checkpoints => {
                            "Restoring replaces the whole notebook with how it was at that time. \
                             You will be asked to confirm."
                        }
                    })
                    .size(12.5)
                    .color(ch.muted),
                );
                let rows = &self.recovery.rows[now];
                let row_h = if tab == Tab::Trash {
                    ROW_TRASH
                } else {
                    ROW_CHECKPOINT
                };
                let list_focused = ui.memory(|m| m.has_focus(list_id));
                if rows.is_empty() {
                    ui.add_space(14.0);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new(if tab == Tab::Trash {
                                "Trash is empty"
                            } else {
                                "No checkpoints yet"
                            })
                            .color(ch.muted),
                        );
                    });
                    ui.add_space(14.0);
                } else {
                    let want = std::mem::take(&mut self.recovery.scroll_to_selected);
                    let sel = self.recovery.selected[now];
                    let mut surface_resp = None;
                    egui::ScrollArea::vertical()
                        .max_height(max_list)
                        .auto_shrink([false, true])
                        .id_salt(("recovery-scroll", now))
                        .show(ui, |ui| {
                            let width = ui.available_width();
                            let total = rows.len() as f32 * row_h;
                            let (area, _) =
                                ui.allocate_exact_size(Vec2::new(width, total), Sense::hover());
                            // The focusable surface under the rows; the rows and their Restore
                            // commands take the pointer on top of it.
                            surface_resp = Some(ui.interact(area, list_id, Sense::click()));
                            for (i, row) in rows.iter().enumerate() {
                                let rect = Rect::from_min_size(
                                    area.min + Vec2::new(0.0, i as f32 * row_h),
                                    Vec2::new(width, row_h),
                                );
                                if !ui.is_rect_visible(rect) {
                                    continue;
                                }
                                let r = ui.interact(
                                    rect,
                                    ui.make_persistent_id(("recovery-row", now, i)),
                                    Sense::CLICK,
                                );
                                r.widget_info(|| {
                                    egui::WidgetInfo::labeled(
                                        egui::Role::ListItem,
                                        true,
                                        row.title.as_str(),
                                    )
                                });
                                let current = i == sel;
                                if current {
                                    let (fill, bar) = if list_focused {
                                        (ch.selected_focus, ch.accent)
                                    } else {
                                        (ch.selected_blur, ch.muted)
                                    };
                                    ui.painter().rect_filled(rect, 0.0, fill);
                                    ui.painter().rect_filled(
                                        Rect::from_min_size(rect.min, Vec2::new(2.0, row_h)),
                                        0.0,
                                        bar,
                                    );
                                } else if r.hovered() {
                                    ui.painter().rect_filled(rect, 0.0, ch.hover_fill);
                                }
                                // The Restore command, on the right of the title line.
                                let label = "Restore";
                                let g = ui.painter().layout_no_wrap(
                                    label.to_owned(),
                                    FontId::proportional(12.5),
                                    Color32::PLACEHOLDER,
                                );
                                let cmd = Rect::from_min_size(
                                    Pos2::new(rect.right() - g.size().x - 22.0, rect.top() + 6.0),
                                    Vec2::new(g.size().x + 16.0, 24.0),
                                );
                                let cr = ui.interact(
                                    cmd,
                                    ui.make_persistent_id(("recovery-restore", now, i)),
                                    Sense::CLICK,
                                );
                                cr.widget_info(|| {
                                    egui::WidgetInfo::labeled(
                                        egui::Role::Button,
                                        true,
                                        format!("Restore {}", row.title),
                                    )
                                });
                                let cc = if cr.hovered() { ch.accent } else { ch.secondary };
                                if cr.hovered() {
                                    ui.painter().rect_filled(cmd, 4.0, ch.hover_fill);
                                }
                                ui.painter().galley(
                                    cmd.center() - g.size() / 2.0,
                                    g,
                                    cc,
                                );
                                let text_left = rect.left() + GUTTER;
                                let text_w = (cmd.left() - text_left - 6.0).max(20.0);
                                let wide = (rect.right() - GUTTER - text_left).max(20.0);
                                let one = |s: &str, size: f32, c: Color32, w: f32| {
                                    let mut job = egui::text::LayoutJob::single_section(
                                        s.to_owned(),
                                        egui::TextFormat::simple(FontId::proportional(size), c),
                                    );
                                    job.wrap = egui::text::TextWrapping {
                                        max_width: w,
                                        max_rows: 1,
                                        break_anywhere: true,
                                        overflow_character: Some('\u{2026}'),
                                    };
                                    ui.painter().layout_job(job)
                                };
                                let tc = if current { ch.selected_text } else { ch.text };
                                let g = one(&row.title, 14.5, tc, text_w);
                                let mut y = rect.top() + 7.0;
                                if g.elided && r.hovered() {
                                    r.show_tooltip_text(&row.title);
                                }
                                ui.painter()
                                    .galley(Pos2::new(text_left, y), g, tc);
                                y += 20.0;
                                for line in &row.lines {
                                    let c = match line.tone {
                                        Tone::Secondary => ch.secondary,
                                        Tone::Muted => ch.muted,
                                        Tone::Danger => ch.danger_text,
                                    };
                                    let g = one(&line.text, 12.5, c, wide);
                                    if g.elided && r.hovered() {
                                        r.show_tooltip_text(&line.text);
                                    }
                                    ui.painter().galley(Pos2::new(text_left, y), g, c);
                                    y += 16.0;
                                }
                                if cr.clicked() {
                                    act = Act::Restore(i);
                                } else if r.clicked() {
                                    self.recovery.selected[now] = i;
                                    ui.memory_mut(|m| m.request_focus(list_id));
                                }
                            }
                            if want {
                                let r = Rect::from_min_size(
                                    area.min + Vec2::new(0.0, sel as f32 * row_h),
                                    Vec2::new(width, row_h),
                                );
                                ui.scroll_to_rect(r, None);
                            }
                        });
                    if surface_resp.is_some_and(|r| r.clicked()) {
                        ui.memory_mut(|m| m.request_focus(list_id));
                    }
                    if list_focused {
                        ui.memory_mut(|m| {
                            m.set_focus_lock_filter(
                                list_id,
                                egui::EventFilter {
                                    vertical_arrows: true,
                                    horizontal_arrows: true,
                                    tab: false,
                                    escape: false,
                                },
                            );
                        });
                    }
                }
                if let Some(e) = &self.recovery.error {
                    ui.label(egui::RichText::new(e).color(ch.danger_text));
                }
                dialog_buttons(ui, |ui| {
                    let close = ui.button("Close");
                    if first_frame && rows.is_empty() {
                        close.request_focus();
                    }
                    if close.clicked() {
                        act = Act::Close;
                    }
                });
                if first_frame && !self.recovery.rows[now].is_empty() {
                    ui.memory_mut(|m| m.request_focus(list_id));
                }
            });
        // Keys: only the part of the dialog that has the keyboard acts on them.
        if confirm.is_none() {
            let none = egui::Modifiers::NONE;
            let (tabs_focus, list_focus) = ctx.memory(|m| {
                (
                    m.has_focus(Host::recovery_tabs_id()),
                    m.has_focus(Host::recovery_list_id()),
                )
            });
            if tabs_focus {
                let (l, r) = ctx.input_mut(|i| {
                    (
                        i.consume_key(none, Key::ArrowLeft),
                        i.consume_key(none, Key::ArrowRight),
                    )
                });
                if l {
                    self.recovery.switch(Tab::Trash);
                }
                if r {
                    self.recovery.switch(Tab::Checkpoints);
                }
            }
            if list_focus {
                let page = (max_list / ROW_CHECKPOINT) as isize;
                let (down, up, pd, pu, home, end, go) = ctx.input_mut(|i| {
                    (
                        i.consume_key(none, Key::ArrowDown),
                        i.consume_key(none, Key::ArrowUp),
                        i.consume_key(none, Key::PageDown),
                        i.consume_key(none, Key::PageUp),
                        i.consume_key(none, Key::Home),
                        i.consume_key(none, Key::End),
                        i.consume_key(none, Key::Enter) | i.consume_key(none, Key::Space),
                    )
                });
                let r = &mut self.recovery;
                if down {
                    r.step(1);
                }
                if up {
                    r.step(-1);
                }
                if pd {
                    r.step(page);
                }
                if pu {
                    r.step(-page);
                }
                if home {
                    r.step(-(isize::MAX / 2));
                }
                if end {
                    r.step(isize::MAX / 2);
                }
                if go {
                    if let Some(i) = self.recovery_selected() {
                        act = Act::Restore(i);
                    }
                }
            }
        }
        if m.should_close() {
            act = if confirm.is_some() {
                Act::Back
            } else {
                Act::Close
            };
        }
        match act {
            Act::None => {}
            Act::Close => self.cancel_dialog(),
            Act::Back => {
                self.recovery.confirm = None;
                self.set_focus_in_dialog();
            }
            Act::Restore(i) => {
                if self.recovery.tab == Tab::Trash {
                    self.restore_trash(ctx, i);
                } else {
                    self.recovery.error = None;
                    self.recovery.confirm = Some(i);
                    // The question starts on its safe answer.
                    self.dialog_fresh = true;
                }
            }
            Act::Confirm(i) => {
                self.restore_checkpoint(ctx, i);
            }
        }
    }

    /// Back from the confirmation to the lists: the list has the keyboard again.
    fn set_focus_in_dialog(&mut self) {
        self.dialog_fresh = true;
    }
}

use omatree_core::recovery::RecoveryError;
