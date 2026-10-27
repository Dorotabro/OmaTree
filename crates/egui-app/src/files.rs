//! Starting without a file, New, Open, and the native file dialogs (ticket E8a).
//!
//! Every way of leaving the current notebook (close the window, New, Open) goes through one
//! guard, [`Host::request_leave`]: the pending title is committed, the editor is flushed, and a
//! notebook with unsaved changes is only left after Save succeeded or Discard was confirmed. The
//! action waiting for that answer is a single `Option<Leave>` that is taken exactly once, so a
//! repeated key or callback cannot run it twice, and any Cancel drops it.
//!
//! A notebook is only ever replaced by [`Host::install`], which is what opening a path, New and
//! the first start all use, so what is reset (editor state, Search, Recovery, autosave and
//! conflict state) cannot differ between them.
//!
//! The native dialogs (`rfd`, through the desktop portal on Wayland) run on their own thread and
//! answer through a channel polled once per frame; the UI thread never blocks and the notebook is
//! not touched until an answer arrives. Without a picker (tests, or `OMATREE_FILE_DIALOG=typed`)
//! the same questions are asked with a typed-path dialog.

use super::lifecycle::{dialog_buttons, dialog_frame, dialog_heading, dialog_open};
use super::*;
use std::sync::mpsc;

/// What is waiting for the unsaved-changes question to be answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Leave {
    Close,
    /// Replace the notebook with a new, untitled one.
    New,
    /// Choose a notebook and open it.
    Open,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickKind {
    Open,
    /// Save As; `then` carries on with the pending leave once saved.
    Save {
        then: bool,
    },
}

#[derive(Clone, Debug)]
pub struct PickRequest {
    pub kind: PickKind,
    /// Where the dialog starts (the folder of the current notebook), if known.
    pub dir: Option<PathBuf>,
    /// The file name offered by Save As.
    pub name: String,
}

/// A chosen path, or `None` when the dialog was cancelled.
pub type PickResult = Option<PathBuf>;

/// Starts a file dialog and returns the channel its answer arrives on. It must not block.
pub type Picker = Box<dyn FnMut(&PickRequest, &egui::Context) -> mpsc::Receiver<PickResult>>;

/// The system file dialog on its own thread (xdg-desktop-portal on Wayland, as `rfd` chooses).
pub fn native_picker() -> Picker {
    Box::new(|req, ctx| {
        let (tx, rx) = mpsc::channel();
        let (req, ctx) = (req.clone(), ctx.clone());
        std::thread::spawn(move || {
            let mut dialog = rfd::FileDialog::new()
                .add_filter("OmaTree notebooks", &["omatree"])
                .add_filter("All files", &["*"]);
            if let Some(dir) = &req.dir {
                dialog = dialog.set_directory(dir);
            }
            let picked = match req.kind {
                PickKind::Open => dialog.set_title("Open notebook").pick_file(),
                PickKind::Save { .. } => dialog
                    .set_title("Save notebook as")
                    .set_file_name(&req.name)
                    .save_file(),
            };
            let _ = tx.send(picked);
            ctx.request_repaint();
        });
        rx
    })
}

/// A picker that answers from a list, in order (and cancels when it runs out): for tests.
pub fn scripted_picker(answers: Vec<PickResult>) -> Picker {
    let mut answers = answers.into_iter();
    Box::new(move |_, ctx| {
        let (tx, rx) = mpsc::channel();
        let _ = tx.send(answers.next().flatten());
        ctx.request_repaint();
        rx
    })
}

/// The picker the application uses: native, unless typed dialogs are asked for.
pub fn default_picker() -> Option<Picker> {
    match std::env::var("OMATREE_FILE_DIALOG").as_deref() {
        Ok("typed") => None,
        _ => Some(native_picker()),
    }
}

pub(crate) struct Pick {
    kind: PickKind,
    answer: mpsc::Receiver<PickResult>,
}

impl Host {
    pub fn set_picker(&mut self, picker: Option<Picker>) {
        self.picker = picker;
    }

    /// The action waiting for an answer, if any.
    pub fn pending_leave(&self) -> Option<&Leave> {
        self.pending.as_ref()
    }

    // --- Starting, New ----------------------------------------------------------------------

    /// Start with an empty, untitled notebook that lives in memory (nothing is created on disk).
    pub fn start_untitled(&mut self, ctx: &egui::Context) {
        self.install(ctx, Document::untitled(), None, String::new());
    }

    /// Start with the notebook at `path`, or, when it cannot be opened, with an untitled one and
    /// the reason in front of the user.
    pub fn start_with(&mut self, ctx: &egui::Context, path: Option<&Path>) {
        self.start_untitled(ctx);
        if let Some(p) = path {
            if !self.open_or_report(ctx, p) {
                eprintln!("omatree-egui: {}", self.status);
            }
        }
    }

    // --- The one guard ----------------------------------------------------------------------

    /// Ask to leave the current notebook for `what`. Nothing happens while a question or a file
    /// dialog is already open (repeated commands do not stack). A clean notebook is left at once;
    /// a notebook with unsaved changes asks Save / Discard / Cancel first.
    pub fn request_leave(&mut self, ctx: &egui::Context, what: Leave) {
        if self.doc.is_none() {
            self.perform(ctx, what);
            return;
        }
        if self.dialog_open() || self.pending.is_some() || self.pick.is_some() {
            return;
        }
        // A title still being typed is part of what is saved (or asked about); one the
        // notebook refuses stops everything.
        if !self.commit_title(ctx) {
            return;
        }
        self.leave(ctx);
        if !self.is_dirty() {
            // Which notes are open in the tree is view state, written with any save: one
            // best-effort chance for a notebook that has a file (a failure must not stop this).
            if let Some(doc) = self.doc.as_mut().filter(|d| d.has_path()) {
                let _ = doc.save();
            }
            self.perform(ctx, what);
            return;
        }
        self.pending = Some(what);
        self.open_dialog(ctx, Dialog::Unsaved { error: None });
    }

    /// The pending action may now happen (Save worked, or Discard was confirmed). Taken once.
    pub(crate) fn continue_pending(&mut self, ctx: &egui::Context) {
        match self.pending.take() {
            Some(what) => self.perform(ctx, what),
            None => self.close_dialog(),
        }
    }

    fn perform(&mut self, ctx: &egui::Context, what: Leave) {
        self.dialog = Dialog::None;
        match what {
            Leave::Close => self.finish_close(ctx),
            Leave::New => self.start_untitled(ctx),
            Leave::Open => self.begin_open(ctx),
        }
    }

    pub fn new_notebook(&mut self, ctx: &egui::Context) {
        self.request_leave(ctx, Leave::New);
    }

    pub fn open_notebook(&mut self, ctx: &egui::Context) {
        self.request_leave(ctx, Leave::Open);
    }

    /// Ask which notebook to open: the system dialog, or a typed path without one.
    fn begin_open(&mut self, ctx: &egui::Context) {
        if self.picker.is_some() {
            let dir = self
                .path
                .as_deref()
                .and_then(Path::parent)
                .map(Path::to_path_buf);
            self.begin_pick(
                ctx,
                PickRequest {
                    kind: PickKind::Open,
                    dir,
                    name: String::new(),
                },
            );
        } else {
            self.open_dialog(
                ctx,
                Dialog::Open {
                    path: String::new(),
                    error: None,
                },
            );
        }
    }

    /// Open the notebook at `path` for the user: the notebook is replaced only if it opens; if it
    /// does not, the reason is shown and nothing changed. Duplicate sibling names an older
    /// notebook may hold are explained once, as Qt does.
    pub fn open_or_report(&mut self, ctx: &egui::Context, path: &Path) -> bool {
        if self.open(ctx, path) {
            self.warn_about_duplicates(ctx);
            true
        } else {
            let message = self.status.clone();
            self.focus_back = Focus::Tree;
            self.open_dialog(ctx, Dialog::Error(message));
            false
        }
    }

    // --- Replacing the notebook -----------------------------------------------------------------

    /// Make `doc` the open notebook. The old one's editor state (caret, selection, undo, images),
    /// Search, Recovery and autosave/conflict state are all dropped; the tree is built from the
    /// new document (with its stored expansion), the first note is selected and the tree has the
    /// keyboard. Nothing is written: a notebook is not dirtied by being opened.
    pub(crate) fn install(
        &mut self,
        ctx: &egui::Context,
        doc: Document,
        path: Option<PathBuf>,
        status: String,
    ) {
        self.leave(ctx);
        self.forget_all(ctx);
        self.notebook += 1;
        self.doc = Some(doc);
        self.path = path;
        self.error = None;
        self.status = status;
        self.tree = self.doc.as_ref().map(Tree::build).unwrap_or_default();
        self.selected = None;
        self.text.clear();
        self.title_text.clear();
        self.loaded_title.clear();
        self.title_error = None;
        self.note_has_image = false;
        self.dialog = Dialog::None;
        self.pending = None;
        self.pick = None;
        self.drag = None;
        self.scroll_to = None;
        self.menu_request = false;
        // Results, Trash and checkpoint lists name notes of the notebook that went away.
        self.search = search::Search::default();
        self.recovery = recovery_ui::RecoveryUi::default();
        // Autosave and conflict state belong to the document that was replaced.
        self.edited = false;
        self.autosave_due = None;
        self.autosave_blocked = false;
        self.autosave_error = None;
        if let Some(first) = self.tree.rows().first().map(|r| r.id) {
            self.load(first);
        }
        self.set_focus(Focus::Tree);
    }

    // --- File dialogs -----------------------------------------------------------------------------

    pub(crate) fn begin_pick(&mut self, ctx: &egui::Context, req: PickRequest) {
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        let answer = picker(&req, ctx);
        self.pick = Some(Pick {
            kind: req.kind,
            answer,
        });
        self.open_dialog(ctx, Dialog::Picking);
    }

    /// Once per frame: has the file dialog answered?
    pub(crate) fn poll_pick(&mut self, ctx: &egui::Context) {
        let Some(pick) = &self.pick else { return };
        let (kind, result) = match pick.answer.try_recv() {
            Ok(r) => (pick.kind, Some(r)),
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => (pick.kind, None),
        };
        self.pick = None;
        self.dialog = Dialog::None;
        match result.flatten() {
            // Cancelled (or the dialog died): the notebook and the editor are as they were.
            None => {
                self.pending = None;
                self.close_dialog();
            }
            Some(path) => match kind {
                PickKind::Open => {
                    self.pending = None;
                    self.set_focus(self.focus_back);
                    self.open_or_report(ctx, &path);
                }
                PickKind::Save { then } => {
                    let typed = path.to_string_lossy().into_owned();
                    self.submit_save_target(ctx, &typed, then);
                }
            },
        }
    }

    /// Enter in the typed Open dialog.
    pub fn submit_open_path(&mut self, ctx: &egui::Context) {
        let Dialog::Open { path, .. } = self.dialog.clone() else {
            return;
        };
        let typed = path.trim();
        if typed.is_empty() {
            self.dialog = Dialog::Open {
                path,
                error: Some("Enter the path of a notebook.".to_owned()),
            };
            self.dialog_fresh = true;
            return;
        }
        let target = {
            let p = PathBuf::from(typed);
            if p.is_absolute() {
                p
            } else {
                self.path
                    .as_deref()
                    .and_then(Path::parent)
                    .map(Path::to_path_buf)
                    .or_else(|| std::env::current_dir().ok())
                    .unwrap_or_default()
                    .join(p)
            }
        };
        match Document::open_existing(&target) {
            Ok(_) => {
                self.pending = None;
                self.open_or_report(ctx, &target);
            }
            Err(e) => {
                self.dialog = Dialog::Open {
                    path,
                    error: Some(format!("{}: {}", file_name(&target), e.open_message())),
                };
                // Enter took the keyboard out of the field: it goes back for the correction.
                self.dialog_fresh = true;
            }
        }
    }

    pub(crate) fn show_picking(&mut self, ctx: &egui::Context) {
        let ch = *self.theme.chrome();
        let bold = self.bold.clone();
        egui::Modal::new(egui::Id::unique("dlg-picking"))
            .frame(dialog_frame(ctx))
            .show(ctx, |ui| {
                dialog_open(ui, ctx, 340.0);
                dialog_heading(ui, "Choose a file", bold.as_ref(), &ch);
                ui.label("Waiting for the system file dialog\u{2026}");
            });
    }

    pub(crate) fn show_open_path(&mut self, ctx: &egui::Context, first_frame: bool) {
        let ch = *self.theme.chrome();
        let bold = self.bold.clone();
        let Dialog::Open { path, error } = self.dialog.clone() else {
            return;
        };
        let mut typed = path;
        let (mut submit, mut cancel) = (false, false);
        let m = egui::Modal::new(egui::Id::unique("dlg-open"))
            .frame(dialog_frame(ctx))
            .show(ctx, |ui| {
                dialog_open(ui, ctx, 460.0);
                dialog_heading(ui, "Open notebook", bold.as_ref(), &ch);
                ui.label("Path of an .omatree notebook:");
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut typed)
                        .desired_width(f32::INFINITY)
                        .id(egui::Id::unique("dlg-open-path")),
                );
                if first_frame {
                    edit.request_focus();
                }
                let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if let Some(e) = &error {
                    ui.label(egui::RichText::new(e).color(ch.danger_text));
                }
                dialog_buttons(ui, |ui| {
                    if ui.button("Open").clicked() || enter {
                        submit = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if let Dialog::Open { path, .. } = &mut self.dialog {
            path.clone_from(&typed);
        }
        if cancel || m.should_close() {
            self.cancel_dialog();
        } else if submit {
            self.submit_open_path(ctx);
        }
    }
}
