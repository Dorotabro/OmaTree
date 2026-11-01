//! A deliberately small multi-note host for [`EditorView`], used by the `multinote` example and by
//! the regression tests. It is not part of the library API: it only shows (and tests) how a host
//! owns several notes, lends one note's `String` to the editor at a time, and switches or
//! invalidates notes. No database, no files except the attachment store the caller provides.
//!
//! Protocol, in one place:
//! - The host owns every note's Markdown. The editor edits the selected note's `String` in place,
//!   so the host's copy is current after every frame (write-through by construction).
//! - Leaving a note: `commit_pending` first (a slider adjustment in progress becomes one undo step
//!   in *that* note), then change the selection and ask for focus.
//! - Deleting a note, replacing its body from outside, restoring a snapshot or replacing the
//!   notebook: `forget_note` for every affected key (and `forget_images` for a replaced notebook).
//! - Identity is `NoteKey { notebook, note }`, never the title.

use omatree_editor::attachments::AttachmentStore;
use omatree_editor::egui::{self, Key, Modifiers};
use omatree_editor::theme::Palette;
use omatree_editor::{EditorInput, EditorKeys, EditorOutput, EditorView, FocusRequest, NoteKey};
use std::collections::HashMap;

pub struct Note {
    pub key: NoteKey,
    /// Display only; never used to identify a note.
    pub title: String,
    pub text: String,
}

pub struct MultiHost {
    pub notebook: u64,
    pub notes: Vec<Note>,
    pub selected: Option<NoteKey>,
    next_note: u64,
    pub view: EditorView,
    pub store: Box<dyn AttachmentStore>,
    pub palette: Palette,
    pub keys: EditorKeys,
    pub focus_next: bool,
    pub status: String,
    pub last: EditorOutput,
    /// Saved copies of bodies, for the restore-from-snapshot scenario.
    pub snapshots: HashMap<NoteKey, String>,
    /// Which note each frame reported `changed` for, with the text it then held.
    pub edits: Vec<(NoteKey, String)>,
}

impl MultiHost {
    pub fn new(notebook: u64, notes: &[(&str, &str)], store: Box<dyn AttachmentStore>) -> Self {
        let mut h = MultiHost {
            notebook,
            notes: Vec::new(),
            selected: None,
            next_note: 1,
            view: EditorView::new(),
            store,
            palette: Palette::fallback(),
            keys: EditorKeys::omatree(),
            focus_next: true,
            status: String::new(),
            last: EditorOutput::default(),
            snapshots: HashMap::new(),
            edits: Vec::new(),
        };
        h.fill(notes);
        h
    }

    fn fill(&mut self, notes: &[(&str, &str)]) {
        self.notes.clear();
        for (title, text) in notes {
            let key = NoteKey::new(self.notebook, self.next_note);
            self.next_note += 1;
            self.notes.push(Note {
                key,
                title: (*title).to_owned(),
                text: (*text).to_owned(),
            });
        }
        self.selected = self.notes.first().map(|n| n.key);
        self.focus_next = true;
    }

    pub fn keys(&self) -> Vec<NoteKey> {
        self.notes.iter().map(|n| n.key).collect()
    }

    pub fn text_of(&self, key: NoteKey) -> Option<&str> {
        self.notes
            .iter()
            .find(|n| n.key == key)
            .map(|n| n.text.as_str())
    }

    fn note_mut(&mut self, key: NoteKey) -> Option<&mut Note> {
        self.notes.iter_mut().find(|n| n.key == key)
    }

    /// Show `key`. The note being left first finishes any pending image-width adjustment.
    pub fn select(&mut self, ctx: &egui::Context, key: NoteKey) {
        if self.selected == Some(key) || !self.notes.iter().any(|n| n.key == key) {
            return;
        }
        self.leave(ctx);
        self.selected = Some(key);
        self.focus_next = true;
    }

    /// Select the next (`1`) or previous (`-1`) note, wrapping.
    pub fn select_offset(&mut self, ctx: &egui::Context, delta: isize) {
        let keys = self.keys();
        if keys.is_empty() {
            return;
        }
        let at = self
            .selected
            .and_then(|k| keys.iter().position(|x| *x == k))
            .unwrap_or(0) as isize;
        let n = keys.len() as isize;
        let next = keys[(at + delta).rem_euclid(n) as usize];
        self.select(ctx, next);
    }

    /// Commit what the editor still holds for the selected note.
    fn leave(&mut self, ctx: &egui::Context) {
        if let Some(cur) = self.selected {
            let mut view = std::mem::take(&mut self.view);
            if let Some(n) = self.note_mut(cur) {
                view.commit_pending(ctx, cur, &mut n.text);
            }
            self.view = view;
        }
    }

    pub fn delete(&mut self, ctx: &egui::Context, key: NoteKey) {
        if self.selected == Some(key) {
            self.leave(ctx);
        }
        self.view.forget_note(ctx, key);
        self.notes.retain(|n| n.key != key);
        if self.selected == Some(key) {
            self.selected = self.notes.first().map(|n| n.key);
            self.focus_next = true;
        }
    }

    /// The body changed outside the editor (a sync, a restore, a script).
    pub fn replace_body(&mut self, ctx: &egui::Context, key: NoteKey, text: &str) {
        if self.selected == Some(key) {
            // A pending width belongs to the old body: finish it there, then replace.
            self.leave(ctx);
        }
        self.view.forget_note(ctx, key);
        if let Some(n) = self.note_mut(key) {
            n.text = text.to_owned();
        }
        if self.selected == Some(key) {
            self.focus_next = true;
        }
    }

    pub fn snapshot(&mut self, key: NoteKey) {
        if let Some(t) = self.text_of(key).map(str::to_owned) {
            self.snapshots.insert(key, t);
        }
    }

    pub fn restore(&mut self, ctx: &egui::Context, key: NoteKey) {
        if let Some(t) = self.snapshots.get(&key).cloned() {
            self.replace_body(ctx, key, &t);
        }
    }

    /// Replace the whole notebook: every old note is forgotten, the new notebook gets a new id
    /// (so equal note numbers cannot meet old state) and its decoded images start empty.
    pub fn replace_notebook(
        &mut self,
        ctx: &egui::Context,
        notebook: u64,
        notes: &[(&str, &str)],
        store: Option<Box<dyn AttachmentStore>>,
    ) {
        self.leave(ctx);
        for k in self.keys() {
            self.view.forget_note(ctx, k);
        }
        self.view.forget_images(self.notebook);
        self.snapshots.clear();
        self.notebook = notebook;
        self.next_note = 1;
        if let Some(s) = store {
            self.store = s;
        }
        self.fill(notes);
    }

    pub fn ui(&mut self, root: &mut egui::Ui) {
        let ctx = root.ctx().clone();
        // Keyboard switching: Alt+Down / Alt+Up. Consumed before the editor sees them.
        if ctx.input_mut(|i| i.consume_key(Modifiers::ALT, Key::ArrowDown)) {
            self.select_offset(&ctx, 1);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::ALT, Key::ArrowUp)) {
            self.select_offset(&ctx, -1);
        }
        let mut clicked = None;
        let mut action: Option<&'static str> = None;
        egui::Panel::left("notes").show(root, |ui| {
            for n in &self.notes {
                if ui
                    .selectable_label(self.selected == Some(n.key), &n.title)
                    .clicked()
                {
                    clicked = Some(n.key);
                }
            }
            ui.separator();
            for a in [
                "Snapshot",
                "Restore",
                "Replace body",
                "Delete",
                "New notebook",
            ] {
                if ui.button(a).clicked() {
                    action = Some(a);
                }
            }
        });
        if let Some(k) = clicked {
            self.select(&ctx, k);
        }
        if let (Some(a), Some(cur)) = (action, self.selected) {
            match a {
                "Snapshot" => self.snapshot(cur),
                "Restore" => self.restore(&ctx, cur),
                "Replace body" => self.replace_body(&ctx, cur, "Replaced from outside"),
                "Delete" => self.delete(&ctx, cur),
                _ => {}
            }
        }
        if action == Some("New notebook") {
            let id = self.notebook + 1;
            self.replace_notebook(
                &ctx,
                id,
                &[("Fresh one", "first of a new notebook"), ("Fresh two", "")],
                None,
            );
        }
        let focus = if std::mem::take(&mut self.focus_next) {
            FocusRequest::Request
        } else {
            FocusRequest::None
        };
        let panel = egui::Frame::central_panel(&ctx.global_style())
            .inner_margin(egui::Margin::symmetric(0, 8));
        let sel = self.selected;
        let out = egui::CentralPanel::default()
            .frame(panel)
            .show(root, |ui| {
                let note = sel.and_then(|k| self.notes.iter_mut().find(|n| n.key == k));
                match note {
                    Some(n) => Some((
                        n.key,
                        self.view.show(
                            ui,
                            EditorInput {
                                key: n.key,
                                text: &mut n.text,
                                attachments: &*self.store,
                                palette: &self.palette,
                                keys: &self.keys,
                                bold: None,
                                focus,
                            },
                        ),
                    )),
                    None => {
                        ui.label("No notes");
                        None
                    }
                }
            })
            .inner;
        if let Some((key, out)) = out {
            if out.changed {
                if let Some(t) = self.text_of(key).map(str::to_owned) {
                    self.edits.push((key, t));
                }
            }
            if let Some(s) = &out.status {
                self.status = s.clone();
            }
            self.last = out;
        }
    }
}
