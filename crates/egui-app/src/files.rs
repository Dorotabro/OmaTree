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
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

/// What is waiting for the unsaved-changes question to be answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Leave {
    Close,
    /// Replace the notebook with a new, untitled one.
    New,
    /// Choose a notebook (native dialog, or typed path when there is none) and open it.
    Open,
    /// Ask for a typed path and open that, whether or not a native dialog exists.
    OpenTyped,
    /// Open this notebook (a recent file, or a request from the system).
    OpenPath(PathBuf),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickKind {
    Open,
    /// Save As; `then` carries on with the pending leave once saved.
    Save {
        then: bool,
    },
    /// Export note (E11a): a file to create, or (Markdown folder) a folder to create it in.
    Export(note_export::ExportFormat),
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

/// What a dialog can be parented to: any window `rfd` accepts (eframe's `Frame` is one).
pub trait Parent: raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle {}
impl<T: raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle + ?Sized> Parent
    for T
{
}

/// A dialog asked for by the native picker and not yet started.
struct Queued {
    req: PickRequest,
    tx: mpsc::Sender<PickResult>,
    ctx: egui::Context,
}

/// Starts the dialogs that the native picker has queued, as children of the application window.
///
/// The picker runs inside `Host`, which has no window handle; the handle lives in eframe's `Frame`,
/// which exists only inside `App::ui`. So the picker only queues the request (and hands out the
/// channel its answer will come on, exactly as before) and `App::ui` calls [`Launcher::launch`] with
/// the `Frame` right after the frame's work. `launch` builds each dialog *on the UI thread* with
/// `set_parent(frame)` (a safe borrow, no raw handle is kept or converted back) and only then hands
/// the finished dialog to a thread to wait for the answer, which `rfd` supports (`FileDialog` is
/// `Send`). Asking, cancelling, the typed-path fallback and every guard are untouched: this only
/// decides which window the dialog belongs to.
#[derive(Clone, Default)]
pub struct Launcher {
    queue: Rc<RefCell<Vec<Queued>>>,
}

impl Launcher {
    /// Starts every queued dialog, parented to `parent` when there is one. Does not block.
    pub fn launch(&self, parent: Option<&dyn Parent>) {
        let queued: Vec<Queued> = self.queue.borrow_mut().drain(..).collect();
        for Queued { req, tx, ctx } in queued {
            let dialog = build_dialog(&req, parent);
            std::thread::spawn(move || {
                let _ = tx.send(dialog());
                ctx.request_repaint();
            });
        }
    }

    /// How many dialogs are waiting to be started (for tests).
    pub fn queued(&self) -> usize {
        self.queue.borrow().len()
    }
}

/// The dialog for `req`, built here (so that the parent is borrowed on the calling thread) and
/// run, blocking, by whoever calls the returned function.
fn build_dialog(
    req: &PickRequest,
    parent: Option<&dyn Parent>,
) -> Box<dyn FnOnce() -> PickResult + Send> {
    let mut dialog = rfd::FileDialog::new();
    if let Some(parent) = parent {
        dialog = dialog.set_parent(parent);
    }
    if let Some(dir) = &req.dir {
        dialog = dialog.set_directory(dir);
    }
    let notebooks = |d: rfd::FileDialog| {
        d.add_filter("OmaTree notebooks", &["omatree"])
            .add_filter("All files", &["*"])
    };
    let (kind, name) = (req.kind, req.name.clone());
    Box::new(move || match kind {
        PickKind::Open => notebooks(dialog).set_title("Open notebook").pick_file(),
        PickKind::Save { .. } => notebooks(dialog)
            .set_title("Save notebook as")
            .set_file_name(&name)
            .save_file(),
        PickKind::Export(note_export::ExportFormat::Folder) => dialog
            .set_title("Choose where to create the export folder")
            .pick_folder(),
        PickKind::Export(f) => {
            let mut d = dialog.set_title("Export note").set_file_name(&name);
            if let Some(ext) = f.extension() {
                d = d.add_filter(f.label(), &[ext]);
            }
            d.save_file()
        }
    })
}

/// The system file dialog (xdg-desktop-portal on Linux, as `rfd` chooses), and the [`Launcher`] that
/// starts it as a child of the window. The picker returns at once; nothing is shown until
/// `Launcher::launch` runs.
pub fn native_picker() -> (Picker, Launcher) {
    let launcher = Launcher::default();
    let queue = launcher.queue.clone();
    let picker: Picker = Box::new(move |req, ctx| {
        let (tx, rx) = mpsc::channel();
        queue.borrow_mut().push(Queued {
            req: req.clone(),
            tx,
            ctx: ctx.clone(),
        });
        rx
    });
    (picker, launcher)
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
pub fn default_picker() -> Option<(Picker, Launcher)> {
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

    /// Keep the recent-notebooks list in `file` (none: only in memory).
    pub fn set_recent_file(&mut self, file: Option<PathBuf>) {
        self.recent = recent::Recent::load(file);
    }

    pub fn recent_notebooks(&self) -> &[PathBuf] {
        self.recent.items()
    }

    pub fn clear_recent(&mut self) {
        self.recent.clear();
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

    /// Start from the command line (the arguments after the program name, already as paths).
    /// One path opens that notebook. OmaTree shows one notebook at a time, so several paths, or
    /// something that looks like an option, open nothing and say so, as the Qt app does; an
    /// untitled notebook is what is left. The window never replaces a notebook it has not been
    /// asked to.
    pub fn start_with_args(&mut self, ctx: &egui::Context, args: &[PathBuf]) {
        match check_args(args) {
            Ok(path) => self.start_with(ctx, path),
            Err(message) => {
                self.start_untitled(ctx);
                eprintln!("omatree-egui: {message}");
                self.status.clone_from(&message);
                self.focus_back = Focus::Tree;
                self.open_dialog(ctx, Dialog::Error(message));
            }
        }
    }

    /// An outside request to open `path` (a recent entry, or an operating-system open request):
    /// the same guard and the same opening as File ▸ Open, so unsaved changes are asked about
    /// first and an unreadable file changes nothing. Ignored while a question is open.
    pub fn request_open_path(&mut self, ctx: &egui::Context, path: PathBuf) {
        self.request_leave(ctx, Leave::OpenPath(path));
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
            Leave::OpenTyped => self.open_typed_open(ctx),
            Leave::OpenPath(path) => {
                self.open_or_report(ctx, &path);
            }
        }
    }

    pub fn new_notebook(&mut self, ctx: &egui::Context) {
        self.request_leave(ctx, Leave::New);
    }

    pub fn open_notebook(&mut self, ctx: &egui::Context) {
        self.request_leave(ctx, Leave::Open);
    }

    /// Ask which notebook to open: the system dialog, or a typed path without one.
    fn open_typed_open(&mut self, ctx: &egui::Context) {
        self.open_dialog(
            ctx,
            Dialog::Open {
                path: String::new(),
                error: None,
            },
        );
    }

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
            self.open_typed_open(ctx);
        }
    }

    /// Open the notebook at `path` for the user: the notebook is replaced only if it opens; if it
    /// does not, the reason is shown and nothing changed. Duplicate sibling names an older
    /// notebook may hold are explained once, as Qt does.
    pub fn open_or_report(&mut self, ctx: &egui::Context, path: &Path) -> bool {
        if self.open(ctx, path) {
            self.recent.push(path);
            self.warn_about_duplicates(ctx);
            true
        } else {
            // A recent file that is gone is forgotten; anything else stays listed.
            if !path.exists() {
                self.recent.remove(path);
            }
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
        // What the adapter recorded about the old notebook's images means nothing now.
        self.images = ImageReport::default();
        // A review and its tokens belong to the notebook that was replaced.
        self.compact = None;
        self.image_retry = f64::NEG_INFINITY;
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
        // One file dialog at a time: a second request while one is open is ignored.
        if self.pick.is_some() {
            return;
        }
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
                PickKind::Export(format) => {
                    self.dialog = Dialog::None;
                    self.submit_picked_export(ctx, format, &path);
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

    /// Stop waiting for the native dialog. Whatever it answers later goes nowhere (its channel is
    /// dropped), so a late result can never open or save anything.
    fn abandon_pick(&mut self) -> Option<PickKind> {
        self.pick.take().map(|p| p.kind)
    }

    pub(crate) fn show_picking(&mut self, ctx: &egui::Context, first_frame: bool) {
        let ch = *self.theme.chrome();
        let bold = self.bold.clone();
        let (mut cancel, mut typed) = (false, false);
        let m = egui::Modal::new(egui::Id::unique("dlg-picking"))
            .frame(dialog_frame(ctx))
            .show(ctx, |ui| {
                dialog_open(ui, ctx, 380.0);
                dialog_heading(ui, "Choose a file", bold.as_ref(), &ch);
                ui.label("Waiting for the system file dialog\u{2026}");
                ui.label(
                    egui::RichText::new(
                        "If it does not appear, or you would rather type the path, use the second button.",
                    )
                    .size(12.5)
                    .color(ch.muted),
                );
                dialog_buttons(ui, |ui| {
                    let c = ui.button("Cancel");
                    if first_frame {
                        c.request_focus();
                    }
                    cancel = c.clicked();
                    typed = ui.button("Type a path instead\u{2026}").clicked();
                });
            });
        if typed {
            match self.abandon_pick() {
                Some(PickKind::Open) => self.open_typed_open(ctx),
                Some(PickKind::Save { then }) => self.open_typed_save_as(ctx, then),
                Some(PickKind::Export(format)) => self.open_typed_export(ctx, format),
                None => {}
            }
        } else if cancel || m.should_close() {
            self.abandon_pick();
            self.cancel_dialog();
        }
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

/// The path to open from the command line, if any; an error message when the arguments cannot be
/// one notebook.
pub fn check_args(args: &[PathBuf]) -> Result<Option<&Path>, String> {
    if let Some(flag) = args.iter().find(|a| a.to_string_lossy().starts_with('-')) {
        return Err(format!(
            "OmaTree does not know the option \u{201c}{}\u{201d}. Start it with at most one notebook path.",
            flag.display()
        ));
    }
    match args {
        [] => Ok(None),
        [one] => Ok(Some(one.as_path())),
        _ => Err("OmaTree takes at most one notebook path.".to_owned()),
    }
}
