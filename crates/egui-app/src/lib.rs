//! The egui application shell: opens a notebook through `omatree-core`, lists its notes, and edits
//! the selected note's Markdown with `omatree_editor::EditorView`.
//!
//! Boundaries:
//! - `omatree-core::Document` is the authoritative notebook. The host never keeps a second copy of
//!   the tree's bodies; it keeps the *one* `String` of the selected note that the editor edits in
//!   place, and writes it back with `Document::set_body` the frame it changes.
//! - `EditorView` owns caret, selection and undo history per `NoteKey { notebook, note }`.
//! - The host owns the window, shortcuts that are about the document (Ctrl+S), status text and
//!   the attachment backend ([`DocStore`], a view of the open `Document`).

mod compact_ui;
mod drag;
mod files;
mod help;
mod keymap;
mod layout;
mod lifecycle;
mod note_export;
mod ops;
mod recent;
mod recovery_ui;
mod reload;
mod search;
mod startup;
mod store;
mod theme;
mod tree;
mod tree_ui;

/// The Linux application identity (ticket E12b): the Wayland `app_id`, the X11 `WM_CLASS`, the
/// desktop file's name and `StartupWMClass`, the icon name and the AppStream id all use this one
/// string. It differs from the Qt application's `io.github.Dorotabro.OmaTree` only while both are
/// installable side by side; E13 renames it (docs/EGUI_MIGRATION_PLAN.md, "E12b implementation record").
pub const APP_ID: &str = "io.github.Dorotabro.OmaTreeEgui";

pub use startup::{explain as explain_startup_failure, report as startup_failure_report};

/// The 256 px application icon, embedded so the window icon needs no file at run time. (Wayland
/// ignores window icons and takes the icon from the desktop file that matches [`APP_ID`]; X11
/// window managers use this one.)
const WINDOW_ICON_PNG: &[u8] = include_bytes!("../../../assets/logo/omatree-icon-256.png");

/// The window icon, or `None` if the embedded PNG did not decode (it is a test that it does).
pub fn window_icon() -> Option<eframe::egui::IconData> {
    eframe::icon_data::from_png_bytes(WINDOW_ICON_PNG).ok()
}

pub use files::{
    check_args, default_picker, native_picker, scripted_picker, Leave, PickKind, PickRequest,
    PickResult, Picker,
};
pub use lifecycle::{Dialog, AUTOSAVE_DELAY};
pub use ops::Focus;
pub use recent::{default_file as default_recent_file, Recent};
pub use recovery_ui::Tab as RecoveryTab;
pub use store::{DocStore, ImageReport};
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
    /// What the image adapter reported while the editor ran (see `store`).
    images: ImageReport,
    /// The Compact Notebook dialog's state (`compact_ui.rs`), while it is open.
    compact: Option<compact_ui::Compact>,
    /// An input-method composition is in progress (see `Host::filter_input`).
    composing: bool,
    /// When unreadable images were last given another chance (see `after_editor`).
    image_retry: f64,
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
            images: ImageReport::default(),
            compact: None,
            composing: false,
            image_retry: f64::NEG_INFINITY,
            theme: ThemeState::new(None),
            keys: {
                // The omatree profile leaves export to the host; Ctrl+Shift+E is reported by the
                // editor (which must win over Ctrl+E, inline code) and opens the host's dialog.
                let mut keys = EditorKeys::omatree();
                keys.export = Some(keymap::EXPORT);
                keys
            },
            bold: None,
            focus: None,
            focus_tries: 0,
            focus_back: Focus::Tree,
            title_text: String::new(),
            loaded_title: String::new(),
            title_error: None,
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

    /// Gives the controls that have no visible label a name an assistive technology can say: the
    /// tree is "Notes" (a tree), the title field "Note title" and the Markdown editor "Note text".
    /// The nodes exist only while the control is on screen; this changes nothing visible.
    fn name_controls_for_assistive_technology(&self, ctx: &egui::Context) {
        // The window itself: egui gives the root node no name, so a screen reader announced an
        // unnamed window. It is the title bar text, which is rewritten from the live notebook name.
        ctx.accesskit_node_builder(egui::accesskit_root_id(), |node| {
            node.set_label(self.title.as_str());
        });
        ctx.accesskit_node_builder(Host::tree_id(), |node| {
            node.set_role(egui::accesskit::Role::Tree);
            node.set_label("Notes");
        });
        if let Some(id) = self.selected {
            ctx.accesskit_node_builder(self.title_id(), |node| node.set_label("Note title"));
            ctx.accesskit_node_builder(key_of(self.notebook, id).widget_id(), |node| {
                node.set_label("Note text (Markdown)");
            });
        }
    }

    /// Drops the input-method events that carry nothing, before egui sees them.
    ///
    /// On Wayland with a text-input-v3 input method (fcitx5 and others), every frame in which a
    /// text field has the focus makes egui-winit send the cursor rectangle again whenever the
    /// frame has *any* input event, and the input method answers every such message with `done`,
    /// which winit turns into an empty pre-edit event: an event, so the next frame sends the
    /// rectangle again. The window then renders at the display's refresh rate for as long as a
    /// text field is focused (measured: 60 frames and 60 text-input commits per second, about 8 %
    /// of a core, with nothing moving). An empty pre-edit while no composition is in progress
    /// clears nothing, so it is dropped; real composition (a non-empty pre-edit, its commit, or
    /// the empty pre-edit that cancels it) is passed through untouched.
    ///
    /// Applied on Linux only (E12c): the loop was measured and traced on Linux text-input-v3, and
    /// Windows and macOS input methods have not been observed, so there egui and winit's events pass
    /// through exactly as they arrive. Where it does apply it can only ever suppress the empty
    /// "nothing is being composed" event: a stuck `composing` flag (focus lost mid-composition) merely
    /// lets idle empty events through again.
    pub fn filter_input(&mut self, raw: &mut egui::RawInput) {
        if cfg!(target_os = "linux") {
            self.drop_idle_preedit(raw);
        }
    }

    /// The filter itself, on every platform so that it is testable everywhere.
    pub(crate) fn drop_idle_preedit(&mut self, raw: &mut egui::RawInput) {
        use egui::{Event, ImeEvent};
        let composing = &mut self.composing;
        raw.events.retain(|e| match e {
            Event::Ime(ImeEvent::Preedit { text, .. }) if text.is_empty() => {
                std::mem::take(composing)
            }
            Event::Ime(ImeEvent::Preedit { .. }) => {
                *composing = true;
                true
            }
            Event::Ime(ImeEvent::Commit(_)) => {
                *composing = false;
                true
            }
            _ => true,
        });
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
                .exact_size(layout::HEADER_H)
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
                let (Some(id), Some(doc)) = (self.selected, self.doc.as_ref()) else {
                    let new_note = ui.ctx().format_shortcut(&keymap::NEW_NOTE);
                    if self.tree.rows().is_empty() {
                        empty_state(
                            ui,
                            &ch,
                            "This notebook is empty",
                            &format!("Press {new_note} to write the first note."),
                        );
                    } else {
                        empty_state(
                            ui,
                            &ch,
                            "No note selected",
                            &format!(
                                "Choose a note on the left, or press {new_note} for a new one."
                            ),
                        );
                    }
                    return None;
                };
                // A pasted image goes where the caret is. A note whose editor has never had the
                // keyboard has no caret yet, and the editor would put the image at the very start
                // of the note, out of sight when the note is long: give it one at the end.
                if ui.ctx().input(|i| {
                    i.events
                        .iter()
                        .any(|e| matches!(e, egui::Event::PasteImage(_)))
                }) {
                    place_caret_at_end_if_none(ui.ctx(), key_of(self.notebook, id), &self.text);
                }
                // The editor's images come from this notebook's own `Document`, for this frame.
                let store = DocStore::new(doc, &self.images);
                Some(self.view.show(
                    ui,
                    EditorInput {
                        key: key_of(self.notebook, id),
                        text: &mut self.text,
                        attachments: &store,
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
        self.name_controls_for_assistive_technology(&ctx);
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
    /// tooltip), the document's state on the right. A clean document says nothing.
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
                let label = ui.label(egui::RichText::new(text).size(layout::SMALL_PX).color(colour));
                if stopped {
                    if let Some(e) = &self.autosave_error {
                        label.on_hover_text(format!(
                            "{e}\n\nfile \u{25b8} Reload from disk\u{2026} or Save As\u{2026} resolves it."
                        ));
                    }
                }
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
                egui::TextFormat::simple(egui::FontId::proportional(layout::SMALL_PX), colour),
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

    fn after_editor(&mut self, ctx: &egui::Context, out: EditorOutput) {
        // A pasted image is a change like any other (the editor reports it since E13a).
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
        self.report_images(ctx);
        // Ctrl+Shift+E: the editor consumes the key (so it can never reach Ctrl+E, inline code)
        // and only reports it; what it opens is the host's business.
        if out.export_requested && !self.dialog_open() {
            self.start_export(ctx);
        }
    }

    /// Turns what the image adapter recorded into the status bar's words. A refused paste says
    /// why (the editor's own sentence is generic). A stored image that is damaged is named as
    /// such. A read the database could not do right now is not "a missing image": the editor
    /// remembers a failed read as unavailable, so the notebook's decoded images are dropped (at
    /// most every few seconds) to let it try again. Nothing here deletes or changes anything.
    fn report_images(&mut self, ctx: &egui::Context) {
        use omatree_core::attachments::AttachmentError as Core;
        if let Some(e) = self.images.take_put() {
            self.status = store::friendly_put_error(&e);
        }
        let Some(e) = self.images.take_read() else {
            return;
        };
        match e {
            Core::Corrupt(why) => {
                self.status =
                    format!("An image in this note is damaged and cannot be shown: {why}");
            }
            other => {
                self.status = format!("An image could not be read right now: {other}");
                let now = ctx.input(|i| i.time);
                if now - self.image_retry > 3.0 {
                    self.image_retry = now;
                    self.view.forget_images(self.notebook);
                    ctx.request_repaint_after(std::time::Duration::from_secs(3));
                }
            }
        }
    }
}

/// Gives `key`'s text edit a caret at the end of `text` if it has none (it never had the focus).
/// A caret the user left behind is kept.
/// What the editor pane shows when there is no note to edit: a short statement and the one thing
/// to do about it, centred a little above the middle, in the chrome's quiet colours.
fn empty_state(ui: &mut egui::Ui, ch: &theme::Chrome, title: &str, hint: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() * 0.36).max(24.0));
        ui.label(
            egui::RichText::new(title)
                .size(layout::TITLE_PX - 3.0)
                .color(ch.secondary),
        );
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(hint)
                .size(layout::UI_PX)
                .color(ch.muted),
        );
    });
}

fn place_caret_at_end_if_none(ctx: &egui::Context, key: NoteKey, text: &str) {
    use egui::text::{CCursor, CCursorRange};
    let id = key.widget_id();
    let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
    if state.cursor.char_range().is_none() {
        let end = CCursor::new(text.chars().count());
        state.cursor.set_char_range(Some(CCursorRange::one(end)));
        state.store(ctx, id);
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
    /// Starts the native file dialogs the host asks for, as children of this window.
    dialogs: files::Launcher,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, args: Vec<PathBuf>) -> Self {
        let mut host = Host::new();
        host.set_bold(omatree_editor::fonts::install_bold(&cc.egui_ctx));
        host.follow_omarchy(omatree_core::theme::default_omarchy_colors_path());
        host.theme.update(&cc.egui_ctx);
        let dialogs = match files::default_picker() {
            Some((picker, launcher)) => {
                host.set_picker(Some(picker));
                launcher
            }
            None => files::Launcher::default(),
        };
        host.set_recent_file(recent::default_file());
        host.start_with_args(&cc.egui_ctx, &args);
        App { host, dialogs }
    }
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        self.host.filter_input(raw_input);
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.host.logic(ctx);
    }

    fn ui(&mut self, root: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.host.ui(root);
        // Dialogs the frame asked for open now, with this window as their parent.
        self.dialogs.launch(Some(&*frame));
    }
}

#[cfg(test)]
mod tests;
