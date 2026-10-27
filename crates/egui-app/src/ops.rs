//! Selection, expansion and the note operations of the tree (create, rename, move, delete). All of
//! them go through `Document` (`create_default_*`, `rename`, `move_node`, `delete`,
//! `set_expanded` ...); the host only sequences them with the editor: the pending title is
//! committed and the editor buffer flushed before the selection leaves a note, and editor state
//! (caret, selection, undo) of notes that no longer exist is forgotten.

use super::*;
use omatree_core::notebook::NotebookError;

/// Where the keyboard goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Tree,
    /// The title field, text selected (F2, or right after a note was created).
    Title,
    Editor,
}

/// What the Qt app says when a rename is refused (`rename_error_message` in `tree_model.rs`).
fn rename_message(e: NotebookError, title: &str) -> String {
    match e {
        NotebookError::TitleConflict => {
            format!("A note named \"{}\" already exists here.", title.trim())
        }
        NotebookError::EmptyTitle => "A note needs a name.".to_owned(),
        _ => "That note could not be renamed.".to_owned(),
    }
}

impl Host {
    pub(crate) fn tree_id() -> egui::Id {
        egui::Id::unique("omatree-tree")
    }

    pub(crate) fn title_id(&self) -> egui::Id {
        egui::Id::unique((
            "omatree-title",
            self.notebook,
            self.selected.map(NodeId::get),
        ))
    }

    fn rebuild_tree(&mut self) {
        if let Some(doc) = &self.doc {
            self.tree = Tree::build(doc);
        }
    }

    fn refresh_visible(&mut self) {
        if let Some(doc) = &self.doc {
            self.tree.refresh_visible(doc);
        }
    }

    /// Ask for the keyboard to go to `target`. The request is repeated for a few frames until it
    /// has taken effect: egui can refuse a focus change in the frame after a modal closed.
    pub fn focus(&mut self, target: Focus) {
        self.set_focus(target);
    }

    pub(crate) fn set_focus(&mut self, target: Focus) {
        self.focus = Some(target);
        self.focus_tries = 6;
    }

    /// Whether `target` is being asked for this frame (and counts the attempt).
    pub(crate) fn wants_focus(&mut self, target: Focus) -> bool {
        if self.focus != Some(target) {
            return false;
        }
        if self.focus_tries == 0 {
            self.focus = None;
            return false;
        }
        self.focus_tries -= 1;
        true
    }

    /// `target` has the keyboard: the request is fulfilled.
    pub(crate) fn focus_reached(&mut self, target: Focus) {
        if self.focus == Some(target) {
            self.focus = None;
        }
    }

    // --- Title -------------------------------------------------------------------------------

    pub fn title_pending(&self) -> bool {
        self.selected.is_some() && self.title_text.trim() != self.loaded_title
    }

    /// Write the pending title of the selected note. True when nothing was pending, or it was
    /// written. False when the notebook refused it (empty, or a sibling has that name): the
    /// message is shown and the typed text stays to be corrected.
    pub fn commit_title(&mut self, ctx: &egui::Context) -> bool {
        let Some(id) = self.selected else {
            return true;
        };
        let wanted = self.title_text.trim().to_owned();
        if wanted == self.loaded_title {
            return true;
        }
        let Some(doc) = self.doc.as_mut() else {
            return true;
        };
        match doc.rename(id, &wanted) {
            Ok(changed) => {
                self.edited |= changed;
                self.loaded_title = wanted.clone();
                self.title_error = None;
                self.title_text = wanted;
                self.rebuild_tree();
                true
            }
            Err(e) => {
                let msg = rename_message(e, &wanted);
                self.status = msg.clone();
                self.title_error = Some(msg.clone());
                self.open_dialog(ctx, Dialog::Error(msg));
                self.focus_back = Focus::Title;
                false
            }
        }
    }

    // --- Selection -----------------------------------------------------------------------------

    /// Make `id` the current note, revealing it. Refused (false) while a pending title is
    /// invalid: the edit stays and so does the selection.
    pub fn select(&mut self, ctx: &egui::Context, id: NodeId) -> bool {
        if self.selected == Some(id) {
            return true;
        }
        if !self.tree.contains(id) || !self.commit_title(ctx) {
            return false;
        }
        self.leave(ctx);
        self.load(id);
        self.reveal(id);
        true
    }

    /// Open the ancestors of `id` (remembered by the document) and scroll to it.
    pub fn reveal(&mut self, id: NodeId) {
        if self.tree.visible_position(id).is_none() {
            if let Some(doc) = self.doc.as_mut() {
                doc.expand_ancestors(id);
            }
            self.refresh_visible();
        }
        self.scroll_to = Some(id);
    }

    /// Up / Down: `delta` visible rows from the current one, not wrapping.
    pub fn select_offset(&mut self, ctx: &egui::Context, delta: isize) {
        let n = self.tree.len_visible();
        if n == 0 {
            return;
        }
        let at = self
            .selected
            .and_then(|id| self.tree.visible_position(id))
            .map_or(if delta >= 0 { 0 } else { n - 1 }, |p| {
                (p as isize + delta).clamp(0, n as isize - 1) as usize
            });
        let id = self.tree.visible_row(at).id;
        self.select(ctx, id);
    }

    pub fn select_page(&mut self, ctx: &egui::Context, down: bool) {
        let page = self.page_rows.saturating_sub(1).max(1) as isize;
        self.select_offset(ctx, if down { page } else { -page });
    }

    // --- Expansion (view state: never dirties the document) ---------------------------------

    /// Left / Right on the current note, or a click on its marker.
    pub fn set_expanded(&mut self, ctx: &egui::Context, id: NodeId, open: bool) {
        let Some(row) = self.tree.row(id) else { return };
        if !row.has_children {
            return;
        }
        if !open && !self.may_hide_selection(ctx, Some(id)) {
            return;
        }
        if let Some(doc) = self.doc.as_mut() {
            doc.set_expanded(id, open);
        }
        self.refresh_visible();
        self.keep_selection_visible(ctx);
    }

    pub fn toggle_expanded(&mut self, ctx: &egui::Context, id: NodeId) {
        let open = self.doc.as_ref().is_some_and(|d| d.is_expanded(id));
        self.set_expanded(ctx, id, !open);
    }

    /// Ctrl+Right / Ctrl+Left: the note and everything below it.
    pub fn set_subtree_expanded(&mut self, ctx: &egui::Context, id: NodeId, open: bool) {
        if !self.tree.contains(id) {
            return;
        }
        if !open && !self.may_hide_selection(ctx, Some(id)) {
            return;
        }
        if let Some(doc) = self.doc.as_mut() {
            doc.set_subtree_expanded(id, open);
        }
        self.refresh_visible();
        self.keep_selection_visible(ctx);
    }

    pub fn set_all_expanded(&mut self, ctx: &egui::Context, open: bool) {
        if !open && !self.may_hide_selection(ctx, None) {
            return;
        }
        if let Some(doc) = self.doc.as_mut() {
            doc.set_all_expanded(open);
        }
        self.refresh_visible();
        self.keep_selection_visible(ctx);
    }

    /// Collapsing can hide the selected note, which then moves to the nearest visible ancestor.
    /// That is a selection change, so a pending title must be committable first. `under` is the
    /// note being collapsed (its descendants hide), or `None` for "collapse all".
    fn may_hide_selection(&mut self, ctx: &egui::Context, under: Option<NodeId>) -> bool {
        let Some(sel) = self.selected else {
            return true;
        };
        let inside = match under {
            Some(id) => sel != id && self.tree.subtree_ids(id).contains(&sel),
            None => self.tree.row(sel).is_some_and(|r| r.depth > 0),
        };
        !inside || self.commit_title(ctx)
    }

    /// After rows were closed, a hidden current note is replaced by its nearest visible ancestor.
    fn keep_selection_visible(&mut self, ctx: &egui::Context) {
        let Some(sel) = self.selected else { return };
        if self.tree.visible_position(sel).is_some() {
            return;
        }
        if let Some(anc) = self.tree.nearest_visible(sel) {
            self.select(ctx, anc);
        }
    }

    // --- Create ------------------------------------------------------------------------------

    pub fn create_root(&mut self, ctx: &egui::Context) {
        if self.doc.is_none() || !self.commit_title(ctx) {
            return;
        }
        self.leave(ctx);
        let Some(doc) = self.doc.as_mut() else { return };
        match doc.create_default_root() {
            Ok(id) => self.after_create(id),
            Err(e) => self.status = format!("Could not create a note: {e}"),
        }
    }

    pub fn create_child(&mut self, ctx: &egui::Context) {
        let Some(parent) = self.selected else { return };
        if !self.commit_title(ctx) {
            return;
        }
        self.leave(ctx);
        let Some(doc) = self.doc.as_mut() else { return };
        match doc.create_default_child(parent) {
            Ok(id) => self.after_create(id),
            Err(e) => self.status = format!("Could not create a note: {e}"),
        }
    }

    fn after_create(&mut self, id: NodeId) {
        self.edited = true;
        self.rebuild_tree();
        self.load(id);
        self.reveal(id);
        self.set_focus(Focus::Title);
    }

    // --- Move ----------------------------------------------------------------------------------

    /// Alt+Up / Alt+Down: one place up or down among its siblings.
    pub fn move_selected(&mut self, ctx: &egui::Context, step: isize) {
        let Some(id) = self.selected else { return };
        let siblings = self.tree.siblings(id);
        let Some(at) = siblings.iter().position(|s| *s == id) else {
            return;
        };
        let target = at as isize + step;
        if target < 0 || target as usize >= siblings.len() {
            return;
        }
        let parent = self.tree.row(id).and_then(|r| r.parent);
        // The note stays open in the editor, but whatever it holds is written first.
        self.leave(ctx);
        let Some(doc) = self.doc.as_mut() else { return };
        match doc.move_node(id, parent, target as usize) {
            Ok(changed) => {
                self.edited |= changed;
                self.rebuild_tree();
                self.reveal(id);
            }
            Err(e) => self.status = format!("Could not move the note: {e}"),
        }
    }

    // --- Drag and drop ---------------------------------------------------------------------------

    /// The drag is over. Notes that hovering opened are closed again, so a drag that moved
    /// nothing leaves the recorded expansion as it was (hover-opening is transient).
    pub(crate) fn end_drag(&mut self, opened: Vec<NodeId>) {
        self.drag = None;
        if let Some(doc) = self.doc.as_mut() {
            for id in opened {
                doc.set_expanded(id, false);
            }
        }
        self.refresh_visible();
    }

    /// A completed drop on a target the notebook accepted: the one place a drag changes the notebook.
    /// The moved note is selected and revealed (opening the way to it, as Qt does); everything
    /// stays keyed by note id, so its text, caret and undo history are untouched.
    pub(crate) fn drop_note(
        &mut self,
        ctx: &egui::Context,
        id: NodeId,
        parent: Option<NodeId>,
        position: usize,
    ) {
        if !self.tree.contains(id) || !self.commit_title(ctx) {
            return;
        }
        self.leave(ctx);
        let Some(doc) = self.doc.as_mut() else { return };
        match doc.move_node(id, parent, position) {
            Ok(changed) => {
                self.edited |= changed;
                self.rebuild_tree();
                if changed {
                    self.select(ctx, id);
                    self.reveal(id);
                    self.set_focus(Focus::Tree);
                }
            }
            Err(e) => self.status = format!("Could not move the note: {e}"),
        }
    }

    // --- Delete ------------------------------------------------------------------------------

    /// Delete / the menu: ask first.
    pub fn ask_delete(&mut self, ctx: &egui::Context) {
        if let Some(id) = self.selected {
            self.open_dialog(ctx, Dialog::ConfirmDelete(id));
        }
    }

    /// The answer was yes: the note and its subtree go to Trash (a checkpoint is taken first by
    /// `Document::delete`). The editor buffer is flushed first so Trash holds the latest text; a
    /// title typed for the note has no home and is dropped. Editor state of every deleted note
    /// is forgotten, so no undo step can come back from them.
    pub(crate) fn confirm_delete(&mut self, ctx: &egui::Context, id: NodeId) {
        self.dialog = Dialog::None;
        if !self.tree.contains(id) {
            return;
        }
        if self.selected == Some(id) {
            self.leave(ctx);
        }
        let doomed = self.tree.subtree_ids(id);
        let row = self.tree.row(id).cloned();
        let fallback = self.survivor_after(id);
        let Some(doc) = self.doc.as_mut() else { return };
        match doc.delete(id) {
            Ok(()) => {
                for d in &doomed {
                    self.view.forget_note(ctx, key_of(self.notebook, *d));
                }
                self.edited = true;
                self.rebuild_tree();
                if self.selected.is_some_and(|s| doomed.contains(&s)) {
                    self.selected = None;
                    self.text.clear();
                    self.title_text.clear();
                    self.loaded_title.clear();
                    if let Some(f) = fallback {
                        self.load(f);
                        self.reveal(f);
                    }
                }
                self.status = format!(
                    "Moved \u{201c}{}\u{201d} to Trash",
                    row.map(|r| r.title).unwrap_or_default()
                );
            }
            Err(e) => self.status = format!("Could not move the note to Trash: {e}"),
        }
        self.set_focus(Focus::Tree);
    }

    /// What to show after `id` (and below) is gone: its parent, as the Qt app does; for a
    /// top-level note (where Qt shows nothing) the next, else the previous top-level note.
    fn survivor_after(&self, id: NodeId) -> Option<NodeId> {
        let parent = self.tree.row(id)?.parent;
        if parent.is_some() {
            return parent;
        }
        let sibs = self.tree.siblings(id);
        let at = sibs.iter().position(|s| *s == id)?;
        sibs.get(at + 1)
            .or_else(|| at.checked_sub(1).and_then(|p| sibs.get(p)))
            .copied()
    }
}
