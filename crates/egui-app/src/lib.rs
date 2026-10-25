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

mod store;

pub use store::UnavailableStore;

use eframe::egui::{self, Key, Modifiers};
use omatree_core::document::{Document, DocumentError};
use omatree_core::notebook::NodeId;
use omatree_editor::theme::Palette;
use omatree_editor::{EditorInput, EditorKeys, EditorOutput, EditorView, FocusRequest, NoteKey};
use std::path::{Path, PathBuf};

/// What the left pane shows for one note.
pub struct Row {
    pub id: NodeId,
    pub depth: usize,
    pub title: String,
}

/// Window-independent application state, so tests can drive it frame by frame.
pub struct Host {
    doc: Option<Document>,
    path: Option<PathBuf>,
    /// Changes whenever a notebook is opened, so state of an earlier notebook can never match.
    notebook: u64,
    rows: Vec<Row>,
    selected: Option<NodeId>,
    /// The selected note's Markdown, edited in place by the editor.
    text: String,
    view: EditorView,
    store: UnavailableStore,
    palette: Palette,
    keys: EditorKeys,
    bold: Option<egui::FontFamily>,
    focus_next: bool,
    status: String,
    error: Option<String>,
    title: String,
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
            rows: Vec::new(),
            selected: None,
            text: String::new(),
            view: EditorView::new(),
            store: UnavailableStore,
            palette: Palette::fallback(),
            keys: EditorKeys::omatree(),
            bold: None,
            focus_next: false,
            status: String::new(),
            error: None,
            title: String::new(),
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

    pub fn rows(&self) -> &[Row] {
        &self.rows
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
                self.rebuild_rows();
                self.selected = None;
                self.text.clear();
                if let Some(first) = self.rows.first().map(|r| r.id) {
                    self.load(first);
                }
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

    fn rebuild_rows(&mut self) {
        self.rows.clear();
        let Some(doc) = &self.doc else { return };
        let mut depth_of = std::collections::HashMap::new();
        for n in doc.notebook().depth_first() {
            let depth = n.parent_id().map_or(0, |p| depth_of[&p] + 1);
            depth_of.insert(n.id(), depth);
            self.rows.push(Row {
                id: n.id(),
                depth,
                title: n.title().to_owned(),
            });
        }
    }

    /// Forget editor state (caret, selection, undo, images) of the notebook being replaced.
    fn forget_all(&mut self, ctx: &egui::Context) {
        for r in &self.rows {
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
        self.focus_next = true;
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
        if let Err(e) = doc.set_body(id, &self.text) {
            self.status = format!("Could not update the note: {e}");
        }
    }

    pub fn select(&mut self, ctx: &egui::Context, id: NodeId) {
        if self.selected == Some(id) || !self.rows.iter().any(|r| r.id == id) {
            return;
        }
        self.leave(ctx);
        self.load(id);
    }

    /// Ctrl+S: write the notebook. A failure is reported and nothing is lost: the document stays
    /// dirty and the buffer unchanged.
    pub fn save(&mut self, ctx: &egui::Context) {
        self.leave(ctx);
        let Some(doc) = self.doc.as_mut() else { return };
        self.status = match doc.save() {
            Ok(()) => "Saved".to_owned(),
            Err(e) => save_failure(&e),
        };
    }

    pub fn is_dirty(&self) -> bool {
        self.doc.as_ref().is_some_and(Document::is_dirty)
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        // Save As is reserved (Ctrl+Shift+S); it must never fall through to Save or formatting.
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::S)) {
            self.status = "Save As is not available yet.".to_owned();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::S)) {
            self.save(ctx);
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

        let mut clicked = None;
        egui::Panel::left("notes")
            .default_size(240.0)
            .show(root, |ui| {
                let row_h = ui.spacing().interact_size.y;
                egui::ScrollArea::vertical().auto_shrink(false).show_rows(
                    ui,
                    row_h,
                    self.rows.len(),
                    |ui, range| {
                        for r in &self.rows[range] {
                            ui.horizontal(|ui| {
                                ui.add_space(r.depth as f32 * 14.0);
                                if ui
                                    .selectable_label(self.selected == Some(r.id), &r.title)
                                    .clicked()
                                {
                                    clicked = Some(r.id);
                                }
                            });
                        }
                    },
                );
            });
        if let Some(id) = clicked {
            self.select(&ctx, id);
        }

        let dirty = self.is_dirty();
        egui::Panel::bottom("status").show(root, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.weak("Images are not supported yet");
                ui.weak(if dirty {
                    "Unsaved changes"
                } else {
                    "No unsaved changes"
                });
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(egui::Label::new(&self.status).truncate());
                });
            });
        });

        if let Some(r) = self
            .selected
            .and_then(|id| self.rows.iter().find(|r| r.id == id))
        {
            egui::Panel::top("note-title").show(root, |ui| {
                ui.add_space(4.0);
                ui.label(egui::RichText::new(&r.title).strong().size(18.0));
                ui.add_space(4.0);
            });
        }

        let focus = if std::mem::take(&mut self.focus_next) {
            FocusRequest::Request
        } else {
            FocusRequest::None
        };
        let frame = egui::Frame::central_panel(&ctx.global_style())
            .inner_margin(egui::Margin::symmetric(0, 8));
        let out = egui::CentralPanel::default()
            .frame(frame)
            .show(root, |ui| {
                let id = self.selected?;
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
            self.after_editor(out);
        }
    }

    fn after_editor(&mut self, out: EditorOutput) {
        if out.changed {
            self.sync();
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

fn save_failure(e: &DocumentError) -> String {
    e.save_message()
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
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.host.ui(root);
    }
}

#[cfg(test)]
mod tests;
