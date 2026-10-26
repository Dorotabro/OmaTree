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

mod lifecycle;
mod ops;
mod store;
mod tree;
mod tree_ui;

pub use lifecycle::{Dialog, AUTOSAVE_DELAY};
pub use ops::Focus;
pub use store::UnavailableStore;
pub use tree::{Row, Tree};

use eframe::egui::{self, Key, Modifiers};
use omatree_core::document::{Document, DocumentError};
use omatree_core::notebook::NodeId;
use omatree_editor::theme::Palette;
use omatree_editor::{EditorInput, EditorKeys, EditorOutput, EditorView, FocusRequest, NoteKey};
use std::path::{Path, PathBuf};

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
    palette: Palette,
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
    scroll_to: Option<NodeId>,
    /// Open the keyboard context menu on the selected row.
    menu_request: bool,
    menu_pos: Option<egui::Pos2>,
    page_rows: usize,
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
            palette: Palette::fallback(),
            keys: EditorKeys::omatree(),
            bold: None,
            focus: None,
            focus_tries: 0,
            focus_back: Focus::Tree,
            title_text: String::new(),
            loaded_title: String::new(),
            scroll_to: None,
            menu_request: false,
            menu_pos: None,
            page_rows: 10,
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

    pub fn palette(&self) -> &Palette {
        &self.palette
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
                self.leave(ctx);
                self.forget_all(ctx);
                self.notebook += 1;
                self.doc = Some(doc);
                self.path = Some(path.to_path_buf());
                self.error = None;
                self.status = format!("Opened {}", file_name(path));
                self.tree = self.doc.as_ref().map(Tree::build).unwrap_or_default();
                self.selected = None;
                self.text.clear();
                self.title_text.clear();
                self.loaded_title.clear();
                self.dialog = Dialog::None;
                if let Some(first) = self.tree.rows().first().map(|r| r.id) {
                    self.load(first);
                }
                self.set_focus(Focus::Tree);
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
    fn forget_all(&mut self, ctx: &egui::Context) {
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
            Ok(changed) => self.edited |= changed,
            Err(e) => self.status = format!("Could not update the note: {e}"),
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.doc.as_ref().is_some_and(Document::is_dirty)
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if self.dialog_open() {
            return;
        }
        // Ctrl+Shift+S first: egui matches Ctrl+S even with Shift held.
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::S)) {
            self.start_save_as(ctx, false);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::S)) {
            self.save(ctx);
        }
        // Application-wide note commands (as the Qt app's `Shortcut`s): they work from the tree,
        // the title and the editor alike. Shift variants first: egui ignores an extra Shift.
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::N)) {
            self.create_child(ctx);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::N)) {
            self.create_root(ctx);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::ALT, Key::ArrowUp)) {
            self.move_selected(ctx, -1);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::ALT, Key::ArrowDown)) {
            self.move_selected(ctx, 1);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F2)) && self.selected.is_some() {
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

        egui::Panel::left("notes")
            .default_size(260.0)
            .show(root, |ui| self.tree_pane(ui));

        let dirty = self.is_dirty();
        egui::Panel::bottom("status").show(root, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.weak("Images are not supported yet");
                ui.weak(if dirty && self.autosave_error.is_some() {
                    "Unsaved changes (autosave stopped)"
                } else if dirty {
                    "Unsaved changes"
                } else {
                    "No unsaved changes"
                });
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(egui::Label::new(&self.status).truncate());
                });
            });
        });

        if self.selected.is_some() {
            egui::Panel::top("note-title").show(root, |ui| self.title_field(ui));
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
                        palette: &self.palette,
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

    fn after_editor(&mut self, _ctx: &egui::Context, out: EditorOutput) {
        if out.changed {
            self.sync();
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
    pub fn new(cc: &eframe::CreationContext<'_>, path: Option<PathBuf>) -> Self {
        let mut host = Host::new();
        host.set_bold(omatree_editor::fonts::install_bold(&cc.egui_ctx));
        host.palette().apply(&cc.egui_ctx);
        match path {
            Some(p) => {
                if !host.open(&cc.egui_ctx, &p) {
                    eprintln!("omatree-egui: {}", host.status());
                }
            }
            None => eprintln!("usage: omatree-egui NOTEBOOK.omatree"),
        }
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
