//! The egui application shell: opens a notebook through `omatree-core`, lists its notes, and edits
//! the selected note's Markdown with `omatree_editor::EditorView`.
//!
//! Boundaries:
//! - `omatree-core::Document` is the authoritative notebook. The host never keeps a second copy of
//!   the tree's bodies; it keeps the *one* `String` of the selected note that the editor edits in
//!   place, and writes it back with `Document::set_body` the frame it changes.
//! - `EditorView` owns caret, selection and undo history per `NoteKey { notebook, note }`.
//! - The host owns the window, shortcuts that are about the document (Ctrl+S), status text and
//!   the attachment backend (none yet: [`UnavailableStore`]).

mod drag;
mod files;
mod help;
mod keymap;
mod layout;
mod lifecycle;
mod ops;
mod recent;
mod recovery_ui;
mod reload;
mod search;
mod store;
mod theme;
mod tree;
mod tree_ui;

pub use files::{
    check_args, default_picker, native_picker, scripted_picker, Leave, PickKind, PickRequest,
    PickResult, Picker,
};
pub use lifecycle::{Dialog, AUTOSAVE_DELAY};
pub use ops::Focus;
pub use recent::{default_file as default_recent_file, Recent};
pub use recovery_ui::Tab as RecoveryTab;
pub use store::UnavailableStore;
pub use tree::{Row, Tree};

use eframe::egui::{self, Key};
use omatree_core::document::{Document, DocumentError};
use omatree_core::notebook::NodeId;
use omatree_editor::theme::Palette;
use omatree_editor::{EditorInput, EditorKeys, EditorOutput, EditorView, FocusRequest, NoteKey};
use std::path::{Path, PathBuf};
use theme::ThemeState;

/// Window-independent application state, so tests can drive it frame by frame.
pub struct Host {
    doc: Option<Document>,
    path: Option<PathBuf>,
    /// Changes whenever a notebook is opened, so state of an earlier notebook can never match.
    notebook: u64,
    tree: Tree,
    selected: Option<NodeId>,
    /// The selected note's Markdown, edited in place by the editor.
    text: String,
    view: EditorView,
    store: UnavailableStore,
    /// The one theme pipeline: core palette -> egui visuals and the editor's palette.
    theme: ThemeState,
    keys: EditorKeys,
    bold: Option<egui::FontFamily>,
    /// Where the keyboard should go on the next frame (a one-shot request).
    focus: Option<Focus>,
    focus_tries: u8,
    /// Where it was when a dialog opened, and where it returns.
    focus_back: Focus,
    /// The title field's text and the title it was filled from (pending when they differ).
    title_text: String,
    loaded_title: String,
    /// Why the pending title was refused, shown beside the field until it is edited or accepted.
    title_error: Option<String>,
    /// The open note's Markdown mentions an image (`![`); only then is the image notice shown.
    note_has_image: bool,
    /// What the status bar last showed and since when, so ordinary feedback can fade away.
    status_seen: (String, bool),
    status_since: f64,
    /// Notebook-wide Search: the field, the query and the results derived from it.
    search: search::Search,
    recovery: recovery_ui::RecoveryUi,
    /// What waits for the unsaved-changes answer (close, New, Open).
    pending: Option<Leave>,
    /// The native file dialog starter (none: typed paths) and the one in flight.
    picker: Option<Picker>,
    pick: Option<files::Pick>,
    /// Recently opened notebooks (paths only, kept in the user's configuration folder).
    recent: recent::Recent,
    scroll_to: Option<NodeId>,
    /// Open the keyboard context menu on the selected row.
    menu_request: bool,
    menu_pos: Option<egui::Pos2>,
    page_rows: usize,
    /// A drag from a tree row in progress (the notebook is untouched until it is dropped).
    drag: Option<drag::Drag>,
    /// Where the tree's rows were last drawn: content origin and visible viewport (screen space).
    tree_geom: (egui::Pos2, egui::Rect),
    status: String,
    error: Option<String>,
    title: String,
    dialog: Dialog,
    dialog_fresh: bool,
    /// The window may close (set by a successful Save or a confirmed Discard).
    quit: bool,
    /// A real edit reached the document since the autosave countdown was last (re)started.
    edited: bool,
    autosave_due: Option<f64>,
    autosave_blocked: bool,
    autosave_error: Option<String>,
}

fn key_of(notebook: u64, id: NodeId) -> NoteKey {
    NoteKey::new(notebook, id.get())
}

impl Default for Host {
    fn default() -> Self {
        Self::new()
    }
}

impl Host {
    pub fn new() -> Self {
        Host {
            doc: None,
            path: None,
            notebook: 0,
            tree: Tree::default(),
            selected: None,
            text: String::new(),
            view: EditorView::new(),
            store: UnavailableStore,
            theme: ThemeState::new(None),
            keys: EditorKeys::omatree(),
            bold: None,
            focus: None,
            focus_tries: 0,
            focus_back: Focus::Tree,
            title_text: String::new(),
            loaded_title: String::new(),
            title_error: None,
            note_has_image: false,
            status_seen: (String::new(), false),
            status_since: 0.0,
            search: search::Search::default(),
            recovery: recovery_ui::RecoveryUi::default(),
            pending: None,
            picker: None,
            pick: None,
            recent: recent::Recent::default(),
            scroll_to: None,
            menu_request: false,
            menu_pos: None,
            page_rows: 10,
            drag: None,
            tree_geom: (egui::Pos2::ZERO, egui::Rect::NOTHING),
            status: String::new(),
            error: None,
            title: String::new(),
            dialog: Dialog::None,
            dialog_fresh: false,
            quit: false,
            edited: false,
            autosave_due: None,
            autosave_blocked: false,
            autosave_error: None,
        }
    }

    pub fn set_bold(&mut self, bold: Option<egui::FontFamily>) {
        self.bold = bold;
    }

    /// The palette the editor is given (derived from the core palette).
    pub fn palette(&self) -> &Palette {
        self.theme.editor()
    }

    /// The semantic palette everything is drawn from.
    pub fn theme_palette(&self) -> &omatree_core::theme::Palette {
        self.theme.palette()
    }

    /// How many times a palette has been installed into egui.
    pub fn theme_applications(&self) -> u64 {
        self.theme.applications()
    }

    /// Whether a valid Omarchy palette is in use (rather than the built-in ones).
    pub fn using_omarchy(&self) -> bool {
        self.theme.using_omarchy()
    }

    /// Follow this Omarchy colours file (or none) instead of the built-in palettes only. A host
    /// does this once at start-up with `omatree_core::theme::default_omarchy_colors_path()`.
    pub fn follow_omarchy(&mut self, path: Option<PathBuf>) {
        self.theme = ThemeState::new(path);
    }

    pub fn document(&self) -> Option<&Document> {
        self.doc.as_ref()
    }

    /// Every note in tree order (visible or not).
    pub fn rows(&self) -> &[Row] {
        self.tree.rows()
    }

    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    /// The title field's current text (what is typed, committed or not).
    pub fn title_text(&self) -> &str {
        &self.title_text
    }

    /// Where the keyboard goes on the next frame, if something asked for it.
    pub fn pending_focus(&self) -> Option<Focus> {
        self.focus
    }

    pub fn selected(&self) -> Option<NodeId> {
        self.selected
    }

    /// The active editor buffer (the selected note's Markdown, including edits the document has
    /// already received).
    pub fn buffer(&self) -> &str {
        &self.text
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    /// The reason the last open attempt failed, if it did and nothing is open.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn notebook_id(&self) -> u64 {
        self.notebook
    }

    /// Open an existing notebook, replacing the current one. A file that can't be opened leaves
    /// the current notebook as it was and reports why. The file is never created or modified.
    pub fn open(&mut self, ctx: &egui::Context, path: &Path) -> bool {
        match Document::open_existing(path) {
            Ok(doc) => {
                self.install(
                    ctx,
                    doc,
                    Some(path.to_path_buf()),
                    format!("Opened {}", file_name(path)),
                );
                true
            }
            Err(e) => {
                let msg = format!("{}: {}", file_name(path), e.open_message());
                if self.doc.is_none() {
                    self.error = Some(msg.clone());
                }
                self.status = msg;
                false
            }
        }
    }

    /// Forget editor state (caret, selection, undo, images) of the notebook being replaced.
    pub(crate) fn forget_all(&mut self, ctx: &egui::Context) {
        for r in self.tree.rows() {
            self.view.forget_note(ctx, key_of(self.notebook, r.id));
        }
        self.view.forget_images(self.notebook);
    }

    fn load(&mut self, id: NodeId) {
        self.text = self
            .doc
            .as_ref()
            .and_then(|d| d.notebook().get(id))
            .map(|n| n.body().to_owned())
            .unwrap_or_default();
        self.selected = Some(id);
        self.loaded_title = self
            .tree
            .row(id)
            .map(|r| r.title.clone())
            .unwrap_or_default();
        self.title_text = self.loaded_title.clone();
        self.title_error = None;
        self.note_has_image = self.text.contains("![");
    }

    /// Finish what the editor still holds for the selected note (a pending image width) and make
    /// sure the document has the buffer's text.
    fn leave(&mut self, ctx: &egui::Context) {
        let Some(id) = self.selected else { return };
        self.view
            .commit_pending(ctx, key_of(self.notebook, id), &mut self.text);
        self.sync();
    }

    /// Write the buffer to the document if it differs. Cheap when nothing changed.
    fn sync(&mut self) {
        let (Some(id), Some(doc)) = (self.selected, self.doc.as_mut()) else {
            return;
        };
        if doc
            .notebook()
            .get(id)
            .is_some_and(|n| n.body() == self.text)
        {
            return;
        }
        match doc.set_body(id, &self.text) {
            Ok(changed) => {
                self.edited |= changed;
                if changed {
                    self.search.body_changed();
                }
            }
            Err(e) => self.status = format!("Could not update the note: {e}"),
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.doc.as_ref().is_some_and(Document::is_dirty)
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        // F1 opens the reference and closes it again; behind any other question it does nothing.
        if self.doc.is_some()
            && matches!(self.dialog, Dialog::None | Dialog::Shortcuts)
            && ctx.input_mut(|i| i.consume_shortcut(&keymap::HELP))
        {
            if self.dialog == Dialog::Shortcuts {
                self.cancel_dialog();
            } else {
                self.open_dialog(ctx, Dialog::Shortcuts);
            }
        }
        if self.dialog_open() {
            return;
        }
        // Shift variants first: egui ignores an extra Shift.
        if ctx.input_mut(|i| i.consume_shortcut(&keymap::SAVE_AS)) {
            self.start_save_as(ctx, false);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&keymap::SAVE)) {
            self.save(ctx);
        }
        if self.drag.is_none() && ctx.input_mut(|i| i.consume_shortcut(&keymap::OPEN)) {
            self.open_notebook(ctx);
        }
        // Ctrl+Alt+N: a whole new notebook (Ctrl+N stays "new note"). Alt is never ignored by
        // egui's matching, so this is tried before and apart from Ctrl+N.
        if self.drag.is_none() && ctx.input_mut(|i| i.consume_shortcut(&keymap::NEW_NOTEBOOK)) {
            self.new_notebook(ctx);
        }
        // Application-wide note commands (as the Qt app's `Shortcut`s): they work from the tree,
        // the title and the editor alike.
        if ctx.input_mut(|i| i.consume_shortcut(&keymap::NEW_CHILD)) {
            self.create_child(ctx);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&keymap::NEW_NOTE)) {
            self.create_root(ctx);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&keymap::MOVE_UP)) {
            self.move_selected(ctx, -1);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&keymap::MOVE_DOWN)) {
            self.move_selected(ctx, 1);
        }
        if self.drag.is_none() && ctx.input_mut(|i| i.consume_shortcut(&keymap::RECOVERY)) {
            self.open_recovery(ctx);
        }
        if self.drag.is_none() && ctx.input_mut(|i| i.consume_shortcut(&keymap::FIND)) {
            self.start_search();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&keymap::RENAME)) && self.selected.is_some() {
            self.set_focus(Focus::Title);
        }
    }

    pub fn ui(&mut self, root: &mut egui::Ui) {
        let ctx = root.ctx().clone();
        self.handle_shortcuts(&ctx);

        let name = self
            .doc
            .as_ref()
            .map_or_else(|| "OmaTree".to_owned(), Document::display_name);
        let title = format!(
            "{}{} - OmaTree",
            name,
            if self.is_dirty() { " *" } else { "" }
        );
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }

        if self.doc.is_none() {
            egui::CentralPanel::default().show(root, |ui| {
                ui.heading("No notebook is open");
                ui.label(
                    self.error
                        .as_deref()
                        .unwrap_or("Start OmaTree with the path of an existing .omatree notebook."),
                );
            });
            return;
        }

        let ch = *self.theme.chrome();
        let surface_fill = theme::colour(self.theme.palette().surface);
        // The tree pane has no inset: its rows run edge to edge, so the selection band does too.
        let window = ctx.content_rect().width();
        let max_tree = (window * layout::TREE_MAX_SHARE).max(layout::TREE_MIN);
        let start =
            (window * layout::TREE_START_SHARE).clamp(layout::TREE_MIN, layout::TREE_DEFAULT);
        egui::Panel::left("notes")
            .default_size(start)
            .size_range(layout::TREE_MIN..=max_tree)
            .frame(egui::Frame::NONE.fill(surface_fill))
            .show(root, |ui| self.tree_pane(ui));

        let bar = egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(layout::GUTTER as i8, 0))
            .fill(surface_fill);
        egui::Panel::bottom("status")
            .frame(bar)
            .exact_size(layout::BAR_H)
            .show_separator_line(false)
            .show(root, |ui| self.status_bar(ui, &ch));

        if self.selected.is_some() {
            let frame = egui::Frame::NONE.fill(ctx.global_style().visuals.panel_fill);
            egui::Panel::top("note-title")
                .frame(frame)
                .show_separator_line(false)
                .show(root, |ui| self.title_field(ui));
        }

        let focus = if self.wants_focus(Focus::Editor) {
            FocusRequest::Request
        } else {
            FocusRequest::None
        };
        let frame = egui::Frame::central_panel(&ctx.global_style())
            .inner_margin(egui::Margin::symmetric(0, 8));
        let out = egui::CentralPanel::default()
            .frame(frame)
            .show(root, |ui| {
                let Some(id) = self.selected else {
                    ui.weak("Select or create a note");
                    return None;
                };
                Some(self.view.show(
                    ui,
                    EditorInput {
                        key: key_of(self.notebook, id),
                        text: &mut self.text,
                        attachments: &self.store,
                        palette: self.theme.editor(),
                        keys: &self.keys,
                        bold: self.bold.as_ref(),
                        focus,
                    },
                ))
            })
            .inner;
        if let Some(out) = out {
            self.after_editor(&ctx, out);
        }
        self.show_dialog(&ctx);
        if self.edited {
            // `logic` starts the autosave countdown on the next frame; make sure there is one.
            ctx.request_repaint();
        }
    }

    /// Whether the status message reports something that went wrong.
    fn status_is_problem(&self) -> bool {
        let s = self.status.as_str();
        let is = |m: Option<&str>| m == Some(s);
        is(self.autosave_error.as_deref())
            || is(self.title_error.as_deref())
            || is(self.error.as_deref())
            || matches!(&self.dialog, Dialog::SaveAs { error: Some(e), .. } if e == s)
            || s.starts_with("Could not")
            || s.starts_with("Not opened")
            || s.contains(" failed")
    }

    /// The bottom bar: what just happened on the left (one or two lines, the whole text in a
    /// tooltip), the document's state on the right. A clean document says nothing; the image
    /// notice appears only for a note that mentions an image, and only where there is room.
    fn status_bar(&mut self, ui: &mut egui::Ui, ch: &theme::Chrome) {
        tree_ui::paint_hairline(ui, ch.hairline, layout::GUTTER);
        let dirty = self.is_dirty();
        let stopped = dirty && self.autosave_error.is_some();
        // A refused title is already shown beside the field; do not say it twice.
        let echo = self.title_error.as_deref() == Some(self.status.as_str())
            && self.title_text.trim() != self.loaded_title;
        let problem = self.status_is_problem();
        // Feedback about an action is about the moment: it fades after a few seconds (longer for
        // a failure). A change of message or of the document's saved state starts the clock again.
        let now = ui.input(|i| i.time);
        if self.status_seen.0 != self.status || self.status_seen.1 != dirty {
            self.status_seen.0.clone_from(&self.status);
            self.status_seen.1 = dirty;
            self.status_since = now;
        }
        let ttl = if problem {
            layout::PROBLEM_SECS
        } else {
            layout::FEEDBACK_SECS
        };
        let fresh = now - self.status_since < ttl;
        if fresh && !self.status.is_empty() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(
                    (ttl - (now - self.status_since)).max(0.0) + 0.05,
                ));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if dirty {
                let (text, colour) = if stopped {
                    ("\u{25cf} Autosave stopped", ch.danger_text)
                } else {
                    ("\u{25cf} Unsaved changes", ch.warning_text)
                };
                let label = ui.label(egui::RichText::new(text).size(12.5).color(colour));
                if stopped {
                    if let Some(e) = &self.autosave_error {
                        label.on_hover_text(format!(
                            "{e}\n\nfile \u{25b8} Reload from disk\u{2026} or Save As\u{2026} resolves it."
                        ));
                    }
                }
            }
            if self.note_has_image && ui.available_width() > 420.0 {
                ui.label(
                    egui::RichText::new("Images are not supported yet")
                        .size(12.5)
                        .color(ch.muted),
                );
            }
            if echo || !fresh || self.status.is_empty() {
                return;
            }
            // The message takes what the right-hand state leaves, wrapped by hand: one or two
            // lines, cut with an ellipsis, centred in the bar.
            let (rect, r) = ui.allocate_exact_size(
                egui::vec2(ui.available_width().max(10.0), layout::BAR_H),
                egui::Sense::hover(),
            );
            let colour = if problem {
                ch.danger_text
            } else {
                ch.secondary
            };
            let mut job = egui::text::LayoutJob::single_section(
                self.status.clone(),
                egui::TextFormat::simple(egui::FontId::proportional(12.5), colour),
            );
            job.wrap = egui::text::TextWrapping {
                max_width: rect.width(),
                max_rows: 2,
                break_anywhere: true,
                overflow_character: Some('\u{2026}'),
            };
            let painter = ui.painter_at(rect);
            let galley = painter.layout_job(job);
            let at = egui::pos2(rect.left(), rect.center().y - galley.size().y / 2.0);
            let cut = galley.elided;
            painter.galley(at, galley, colour);
            // The whole text for anything that did not fit.
            if cut {
                r.on_hover_text(&self.status);
            }
        });
    }

    fn after_editor(&mut self, _ctx: &egui::Context, out: EditorOutput) {
        if out.changed {
            self.sync();
            self.note_has_image = self.text.contains("![");
        }
        if out.has_focus {
            self.focus_reached(Focus::Editor);
        }
        // Escape in the editor goes back to the selected note in the tree.
        if out.escape && self.selected.is_some() {
            self.set_focus(Focus::Tree);
        }
        if let Some(s) = out.status {
            self.status = s;
        }
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// The eframe application: a [`Host`] plus window set-up.
pub struct App {
    host: Host,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, args: Vec<PathBuf>) -> Self {
        let mut host = Host::new();
        host.set_bold(omatree_editor::fonts::install_bold(&cc.egui_ctx));
        host.follow_omarchy(omatree_core::theme::default_omarchy_colors_path());
        host.theme.update(&cc.egui_ctx);
        host.set_picker(files::default_picker());
        host.set_recent_file(recent::default_file());
        host.start_with_args(&cc.egui_ctx, &args);
        App { host }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.host.logic(ctx);
    }

    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.host.ui(root);
    }
}

#[cfg(test)]
mod tests;
