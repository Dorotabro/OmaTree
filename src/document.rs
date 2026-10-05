//! The open notebook: its tree, the file it came from, and whether it has
//! unsaved changes. Pure Rust; the Qt model wraps this.

use std::collections::BTreeSet;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::notebook::{MovePlan, NodeId, Notebook, NotebookError, DEFAULT_TITLE};
use crate::recovery::{unix_now, Checkpoint, Recovery, RecoveryError, TrashEntry};
use crate::storage::{Storage, StorageError};

#[derive(Debug)]
pub enum DocumentError {
    Storage(StorageError),
    Io(io::Error),
    /// Saving an untitled notebook.
    NoPath,
    /// Save As onto an existing file without the caller confirming overwrite.
    TargetExists,
    /// Save As onto an existing file that is not a supported OmaTree notebook.
    NotANotebook(StorageError),
}

impl From<StorageError> for DocumentError {
    fn from(e: StorageError) -> Self {
        DocumentError::Storage(e)
    }
}

impl fmt::Display for DocumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DocumentError::Storage(e) => write!(f, "{e}"),
            DocumentError::Io(e) => write!(f, "{e}"),
            DocumentError::NoPath => write!(f, "notebook has no file"),
            DocumentError::TargetExists => write!(f, "target file already exists"),
            DocumentError::NotANotebook(e) => write!(f, "target is not a notebook: {e}"),
        }
    }
}

/// What the user is told when the file was changed behind this window's back.
const CONFLICT_MESSAGE: &str = "This notebook was changed on disk after OmaTree opened it, \
     so nothing was saved and the file was left as it is. Your changes are still \
     here. Use Save As to keep your version, or reopen the notebook to use the one \
     on disk.";

impl DocumentError {
    /// Whether this is the "changed on disk by someone else" refusal.
    #[cfg(test)]
    pub fn is_conflict(&self) -> bool {
        matches!(
            self,
            DocumentError::Storage(StorageError::ExternallyModified)
        )
    }

    /// Short human-readable text for a failed open. The raw error is for logs.
    pub fn open_message(&self) -> String {
        match self {
            DocumentError::Storage(StorageError::ExternallyModified) => CONFLICT_MESSAGE,
            DocumentError::Storage(StorageError::UnsupportedSchemaVersion(_)) => {
                "This file is not a notebook this version of OmaTree can open. \
                 It was left untouched."
            }
            DocumentError::Storage(StorageError::InvalidData(_)) => {
                "This notebook file contains invalid data and can't be opened. \
                 It was left untouched."
            }
            DocumentError::Storage(StorageError::Sqlite(_)) => {
                "This file is not a readable OmaTree notebook (it may be damaged). \
                 It was left untouched."
            }
            DocumentError::Io(_) => "The notebook file could not be accessed.",
            DocumentError::NoPath | DocumentError::TargetExists => "No notebook file was given.",
            DocumentError::NotANotebook(_) => "This file is not an OmaTree notebook.",
        }
        .to_string()
    }

    /// Short human-readable text for a failed Save As.
    pub fn save_as_message(&self) -> String {
        match self {
            DocumentError::Storage(StorageError::ExternallyModified) => CONFLICT_MESSAGE,
            DocumentError::NotANotebook(_) => {
                "That file already exists and is not an OmaTree notebook, so it was \
                 not overwritten."
            }
            DocumentError::TargetExists => "That file already exists and was not replaced.",
            _ => {
                "The notebook could not be saved there. Check that the location is \
                 writable. Your current notebook is unchanged."
            }
        }
        .to_string()
    }

    /// Short human-readable text for a failed save.
    pub fn save_message(&self) -> String {
        match self {
            DocumentError::Storage(StorageError::ExternallyModified) => CONFLICT_MESSAGE,
            DocumentError::NoPath => {
                "This notebook has no file yet, so it can't be saved. \
                 Start OmaTree with a notebook path to save your notes."
            }
            _ => "The notebook could not be saved. Check that the file is writable.",
        }
        .to_string()
    }
}

/// Appends `.omatree` when the user gave a name with no extension. An
/// explicit extension, whatever it is, is left alone.
pub fn with_default_extension(path: &Path) -> PathBuf {
    if path.extension().is_some() || path.file_name().is_none() {
        return path.to_path_buf();
    }
    let mut name = path.as_os_str().to_owned();
    name.push(".omatree");
    PathBuf::from(name)
}

pub struct Document {
    notebook: Notebook,
    recovery: Recovery,
    storage: Option<Storage>,
    path: Option<PathBuf>,
    /// The active notebook differs from what was last written.
    active_dirty: bool,
    /// Trash or checkpoints differ from what was last written. Implies a
    /// full save; ordinary edits never set this.
    recovery_dirty: bool,
    /// Which notes are open in the tree. View state, not content: changing
    /// it never makes the document dirty. May briefly name nodes that are
    /// gone (deleted, or replaced by a checkpoint); those are left out of
    /// every save and dropped once a save succeeds.
    expanded: BTreeSet<NodeId>,
    /// `expanded` differs from what was last written. Only an explicit save
    /// acts on this by itself; it never counts as unsaved content.
    view_dirty: bool,
}

impl Document {
    /// Empty in-memory notebook with no file.
    pub fn untitled() -> Self {
        Document {
            notebook: Notebook::new(),
            recovery: Recovery::default(),
            storage: None,
            path: None,
            active_dirty: false,
            recovery_dirty: false,
            expanded: BTreeSet::new(),
            view_dirty: false,
        }
    }

    /// Opens the notebook at `path`, or creates an empty one there if the
    /// path does not exist. An existing file that can't be loaded is an
    /// error and is never written to.
    pub fn open(path: &Path) -> Result<Self, DocumentError> {
        match std::fs::metadata(path) {
            Ok(_) => Self::open_existing(path),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Document {
                notebook: Notebook::new(),
                recovery: Recovery::default(),
                storage: Some(Storage::create(path)?),
                path: Some(path.to_path_buf()),
                active_dirty: false,
                recovery_dirty: false,
                expanded: BTreeSet::new(),
                view_dirty: false,
            }),
            Err(e) => Err(DocumentError::Io(e)),
        }
    }

    /// Opens an existing notebook. A missing file is an error, never created.
    pub fn open_existing(path: &Path) -> Result<Self, DocumentError> {
        std::fs::metadata(path).map_err(DocumentError::Io)?;
        let storage = Storage::open(path)?;
        let (notebook, recovery) = storage.load_document()?;
        let expanded = storage.load_expanded(&notebook)?;
        Ok(Document {
            notebook,
            recovery,
            storage: Some(storage),
            path: Some(path.to_path_buf()),
            active_dirty: false,
            recovery_dirty: false,
            expanded,
            view_dirty: false,
        })
    }

    /// Saves to the document's file, writing as little as is safe:
    ///
    /// - nothing dirty and no pending view state: no database write at all;
    /// - only the active notebook and/or the view state changed: just the
    ///   active nodes, id counter and expanded nodes, leaving Trash and
    ///   checkpoints untouched;
    /// - recovery state dirty, or the file older than the current schema:
    ///   the full atomic save (which also migrates an older file).
    ///
    /// Pending view state is written by any save that happens, and an
    /// explicit save writes it even when the content is clean. Dirty flags
    /// are cleared only after the write succeeded.
    pub fn save(&mut self) -> Result<(), DocumentError> {
        if self.storage.is_none() {
            return Err(DocumentError::NoPath);
        }
        if !self.active_dirty && !self.recovery_dirty && !self.view_dirty {
            return Ok(());
        }
        let expanded = self.valid_expanded();
        let storage = self.storage.as_mut().ok_or(DocumentError::NoPath)?;
        if self.recovery_dirty || !storage.is_current_schema() {
            storage.save_document_with_view(&self.notebook, &self.recovery, &expanded)?;
            self.active_dirty = false;
            self.recovery_dirty = false;
        } else {
            storage.save_active_with_view(&self.notebook, &expanded)?;
            self.active_dirty = false;
        }
        self.view_dirty = false;
        // What was written is the whole truth now; stale ids are gone.
        self.expanded = expanded;
        Ok(())
    }

    /// Saves the notebook to `path` and, only if that fully succeeds, makes
    /// `path` the document's file and clears dirty. On any failure the
    /// document keeps its previous path, storage and dirty state.
    ///
    /// - The current file: a normal save.
    /// - A missing path: a new notebook is created there (and removed again
    ///   if the save fails).
    /// - An existing path: refused unless `overwrite`, and refused if it is
    ///   not a supported OmaTree notebook; the file is never modified in
    ///   those cases. Otherwise its contents are replaced transactionally.
    pub fn save_as(&mut self, path: &Path, overwrite: bool) -> Result<(), DocumentError> {
        if self.is_current_path(path) {
            return self.save();
        }
        let existed = match std::fs::metadata(path) {
            Ok(_) => true,
            Err(e) if e.kind() == io::ErrorKind::NotFound => false,
            Err(e) => return Err(DocumentError::Io(e)),
        };

        let mut target = if existed {
            if !overwrite {
                return Err(DocumentError::TargetExists);
            }
            Storage::open(path).map_err(DocumentError::NotANotebook)?
        } else {
            Storage::create(path)?
        };
        let expanded = self.valid_expanded();
        if let Err(e) = target.save_document_with_view(&self.notebook, &self.recovery, &expanded) {
            drop(target);
            if !existed {
                let _ = std::fs::remove_file(path);
            }
            return Err(e.into());
        }

        self.storage = Some(target);
        self.path = Some(path.to_path_buf());
        self.active_dirty = false;
        self.recovery_dirty = false;
        self.view_dirty = false;
        self.expanded = expanded;
        Ok(())
    }

    /// Whether `path` is the file this document is already saved to.
    pub fn is_current_path(&self, path: &Path) -> bool {
        let Some(current) = &self.path else {
            return false;
        };
        match (std::fs::canonicalize(current), std::fs::canonicalize(path)) {
            (Ok(a), Ok(b)) => a == b,
            _ => current == path,
        }
    }

    /// True if Save As to `path` would replace an existing file (so the user
    /// must confirm first). Saving to the current file never does.
    pub fn save_as_needs_confirmation(&self, path: &Path) -> bool {
        !self.is_current_path(path) && std::fs::metadata(path).is_ok()
    }

    pub fn notebook(&self) -> &Notebook {
        &self.notebook
    }

    /// Unsaved changes of any kind. The two kinds are an internal detail
    /// that only decides how much `save` has to write.
    pub fn is_dirty(&self) -> bool {
        self.active_dirty || self.recovery_dirty
    }

    /// (active dirty, recovery dirty). Tests only.
    #[cfg(test)]
    fn dirty_domains(&self) -> (bool, bool) {
        (self.active_dirty, self.recovery_dirty)
    }

    // View state: which notes are open in the tree. None of this is content.
    // It does not dirty the document, take checkpoints or touch Trash; it is
    // simply written along with the next save.

    /// Whether the note is recorded as open in the tree.
    pub fn is_expanded(&self, id: NodeId) -> bool {
        self.expanded.contains(&id)
    }

    /// Records one note as open or closed. Notes that do not exist are
    /// ignored. Returns whether anything changed.
    pub fn set_expanded(&mut self, id: NodeId, expanded: bool) -> bool {
        let changed = if expanded {
            self.notebook.get(id).is_some() && self.expanded.insert(id)
        } else {
            self.expanded.remove(&id)
        };
        self.view_dirty |= changed;
        changed
    }

    /// Records a note and every note below it as open (those with children)
    /// or closed.
    pub fn set_subtree_expanded(&mut self, id: NodeId, expanded: bool) {
        let Ok(subtree) = self.notebook.subtree(id) else {
            return;
        };
        for node in &subtree {
            let open = expanded
                && self
                    .notebook
                    .children(node.id())
                    .is_ok_and(|c| !c.is_empty());
            self.set_expanded(node.id(), open);
        }
    }

    /// Records every note as open (those with children) or closed.
    pub fn set_all_expanded(&mut self, expanded: bool) {
        let roots: Vec<NodeId> = self.notebook.roots().iter().map(|n| n.id()).collect();
        for root in roots {
            self.set_subtree_expanded(root, expanded);
        }
        if !expanded {
            // Anything else recorded (stale ids) goes too.
            self.view_dirty |= !self.expanded.is_empty();
            self.expanded.clear();
        }
    }

    /// Records every ancestor of the note as open, so it is visible.
    pub fn expand_ancestors(&mut self, id: NodeId) {
        let mut parent = self.notebook.get(id).and_then(|n| n.parent_id());
        while let Some(p) = parent {
            self.set_expanded(p, true);
            parent = self.notebook.get(p).and_then(|n| n.parent_id());
        }
    }

    /// The recorded notes that exist now; what a save writes.
    fn valid_expanded(&self) -> BTreeSet<NodeId> {
        self.expanded
            .iter()
            .copied()
            .filter(|id| self.notebook.get(*id).is_some())
            .collect()
    }

    /// Treats the file as it is now as seen. Tests only.
    #[cfg(test)]
    fn resync_for_test(&mut self) {
        if let Some(storage) = self.storage.as_mut() {
            storage.resync();
        }
    }

    /// The expanded notes still pending to be saved. Tests only.
    #[cfg(test)]
    fn view_dirty(&self) -> bool {
        self.view_dirty
    }

    pub fn has_path(&self) -> bool {
        self.path.is_some()
    }

    /// File name for display, or "Untitled".
    pub fn display_name(&self) -> String {
        self.path.as_ref().and_then(|p| p.file_name()).map_or_else(
            || "Untitled".to_string(),
            |n| n.to_string_lossy().into_owned(),
        )
    }

    // Mutations. Each marks the document dirty only if it succeeded and
    // actually changed something. Ordinary edits dirty only the active
    // state; structural operations (below) dirty the recovery state too.

    pub fn create_root(&mut self, title: &str) -> Result<NodeId, NotebookError> {
        let id = self.notebook.create_root(title)?;
        self.active_dirty = true;
        Ok(id)
    }

    /// A new top-level note named "New note", "New note 2", ... whichever is
    /// the first free among the top-level notes.
    pub fn create_default_root(&mut self) -> Result<NodeId, NotebookError> {
        let title = self.notebook.unique_default_title(None, DEFAULT_TITLE);
        self.create_root(&title)
    }

    /// Like `create_default_root`, under `parent`.
    pub fn create_default_child(&mut self, parent: NodeId) -> Result<NodeId, NotebookError> {
        let title = self
            .notebook
            .unique_default_title(Some(parent), DEFAULT_TITLE);
        self.create_child(parent, &title)
    }

    pub fn create_child(&mut self, parent: NodeId, title: &str) -> Result<NodeId, NotebookError> {
        let id = self.notebook.create_child(parent, title)?;
        self.active_dirty = true;
        Ok(id)
    }

    /// Returns whether the title actually changed. An empty title, or one
    /// equivalent to another sibling's, is refused and nothing changes.
    pub fn rename(&mut self, id: NodeId, title: &str) -> Result<bool, NotebookError> {
        let unchanged = self
            .notebook
            .get(id)
            .is_some_and(|n| n.title() == title.trim());
        self.notebook.rename(id, title)?;
        self.active_dirty |= !unchanged;
        Ok(!unchanged)
    }

    /// Returns whether the body actually changed.
    pub fn set_body(&mut self, id: NodeId, body: &str) -> Result<bool, NotebookError> {
        let unchanged = self.notebook.get(id).is_some_and(|n| n.body() == body);
        self.notebook.set_body(id, body)?;
        self.active_dirty |= !unchanged;
        Ok(!unchanged)
    }

    /// Moves a node and its whole subtree to Trash, after taking a checkpoint
    /// of the notebook as it is now. Nothing changes if the node is missing.
    pub fn delete(&mut self, id: NodeId) -> Result<(), NotebookError> {
        let subtree = self.notebook.subtree(id)?;
        let reason = format!("Before deleting \"{}\"", subtree[0].title());
        let entry = TrashEntry::new(unix_now(), subtree)?;
        self.create_checkpoint(&reason);
        self.recovery.push_trash(entry);
        self.notebook.delete(id)?;
        self.mark_structural_change();
        Ok(())
    }

    /// A structural change replaced or restructured the tree and touched
    /// Trash or checkpoints, so both kinds of state need saving.
    fn mark_structural_change(&mut self) {
        self.active_dirty = true;
        self.recovery_dirty = true;
    }

    /// Validates a move without changing anything; `None` means it would
    /// change nothing. See `Notebook::plan_move` for the position rules.
    pub(crate) fn plan_move(
        &self,
        id: NodeId,
        new_parent: Option<NodeId>,
        new_position: usize,
    ) -> Result<Option<MovePlan>, NotebookError> {
        self.notebook.plan_move(id, new_parent, new_position)
    }

    /// Moves a node and its subtree to `new_parent` at final sibling index
    /// `new_position`. A real move takes exactly one checkpoint of the tree
    /// as it was ("Before moving …") and then marks both kinds of state
    /// dirty, so the next save is a full, recovery-aware one. A failed or
    /// no-op move changes nothing at all. Returns whether the tree changed.
    pub fn move_node(
        &mut self,
        id: NodeId,
        new_parent: Option<NodeId>,
        new_position: usize,
    ) -> Result<bool, NotebookError> {
        // Prove the move works on a copy before touching anything.
        let mut moved = self.notebook.clone();
        if !moved.move_node(id, new_parent, new_position)? {
            return Ok(false);
        }
        let title = self.notebook.get(id).map_or("", |n| n.title()).to_string();
        self.create_checkpoint(&format!("Before moving \"{title}\""));
        self.notebook = moved;
        self.mark_structural_change();
        Ok(true)
    }

    /// Pretends the document was just saved. Tests only.
    #[cfg(test)]
    fn save_as_dirty_reset_for_test(&mut self) {
        self.active_dirty = false;
        self.recovery_dirty = false;
    }

    // Recovery.

    pub fn recovery(&self) -> &Recovery {
        &self.recovery
    }

    /// Records the current notebook as a recovery checkpoint (keeping only
    /// the newest few). Call this before any operation that restructures
    /// or replaces the tree. The recovery history changed, so the next save
    /// must be a full one.
    pub fn create_checkpoint(&mut self, reason: &str) {
        let checkpoint = Checkpoint::of(&self.notebook, reason, unix_now());
        self.recovery.add_checkpoint(checkpoint);
        self.recovery_dirty = true;
    }

    /// Where Trash entry `index` (oldest first) would be restored: its
    /// parent (None = top level) and sibling position. Fails if restoring
    /// would collide with an active node.
    pub fn plan_trash_restore(
        &self,
        index: usize,
    ) -> Result<(Option<NodeId>, usize), RecoveryError> {
        let entry = self
            .recovery
            .trash()
            .get(index)
            .ok_or(RecoveryError::NoSuchEntry)?;
        Ok(self.notebook.restore_placement(entry.nodes())?)
    }

    /// Restores a Trash entry, after checkpointing the current notebook, and
    /// removes it from Trash. The original parent and position are used when
    /// possible, otherwise the subtree becomes the last root. On failure
    /// nothing changes. Returns the restored root's id.
    pub fn restore_trash(&mut self, index: usize) -> Result<NodeId, RecoveryError> {
        let entry = self
            .recovery
            .trash()
            .get(index)
            .ok_or(RecoveryError::NoSuchEntry)?;
        let reason = format!("Before restoring \"{}\" from Trash", entry.title());
        let root = entry.root().id();
        let nodes = entry.nodes().to_vec();

        // Try it on a copy first so a failure leaves the document as it was.
        let mut restored = self.notebook.clone();
        restored.restore_subtree(nodes)?;

        self.create_checkpoint(&reason);
        self.notebook = restored;
        self.recovery.take_trash(index);
        self.mark_structural_change();
        Ok(root)
    }

    /// Replaces the active notebook with checkpoint `index` (oldest first),
    /// after checkpointing the notebook being replaced. Trash and the other
    /// checkpoints are kept, and ids are never reused. Not saved to disk.
    pub fn restore_checkpoint(&mut self, index: usize) -> Result<(), RecoveryError> {
        let checkpoint = self
            .recovery
            .checkpoints()
            .get(index)
            .ok_or(RecoveryError::NoSuchEntry)?;
        let reason = format!("Before restoring checkpoint \"{}\"", checkpoint.reason());
        let restored = self
            .notebook
            .with_nodes_keeping_counter(checkpoint.nodes().to_vec())?;

        self.create_checkpoint(&reason);
        self.notebook = restored;
        self.mark_structural_change();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A unique path in the temp dir, removed on drop.
    struct TempFile(PathBuf);

    impl TempFile {
        fn new() -> Self {
            static COUNTER: AtomicU32 = AtomicU32::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let name = format!("omatree-doc-test-{}-{n}.omatree", std::process::id());
            let path = std::env::temp_dir().join(name);
            let _ = std::fs::remove_file(&path);
            TempFile(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            if let Ok(meta) = std::fs::metadata(&self.0) {
                let mut perms = meta.permissions();
                #[allow(clippy::permissions_set_readonly_false)]
                perms.set_readonly(false);
                let _ = std::fs::set_permissions(&self.0, perms);
            }
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn titles(nb: &Notebook, parent: Option<NodeId>) -> Vec<String> {
        let nodes = match parent {
            None => nb.roots(),
            Some(p) => nb.children(p).unwrap(),
        };
        nodes.iter().map(|n| n.title().to_string()).collect()
    }

    #[test]
    fn untitled_document_is_clean_and_has_no_path() {
        let doc = Document::untitled();
        assert_eq!(doc.display_name(), "Untitled");
        assert!(!doc.has_path());
        assert!(!doc.is_dirty());
        assert!(doc.notebook().roots().is_empty());
    }

    fn cwd_entries() -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(".")
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn saving_untitled_fails_without_creating_a_file() {
        let before = cwd_entries();
        let mut doc = Document::untitled();
        doc.create_root("a").unwrap();
        assert!(matches!(doc.save(), Err(DocumentError::NoPath)));
        assert!(doc.is_dirty());
        assert_eq!(before, cwd_entries());
    }

    #[test]
    fn missing_path_creates_a_valid_empty_notebook_file() {
        let file = TempFile::new();
        let doc = Document::open(file.path()).unwrap();
        assert!(file.path().exists());
        assert!(doc.has_path());
        assert!(!doc.is_dirty());
        assert_eq!(
            doc.display_name(),
            file.path().file_name().unwrap().to_string_lossy()
        );
        let reopened = Storage::open(file.path()).unwrap().load().unwrap();
        assert!(reopened.roots().is_empty());
    }

    #[test]
    fn save_and_reopen_preserves_the_notebook() {
        let file = TempFile::new();
        let mut doc = Document::open(file.path()).unwrap();
        let projects = doc.create_root("Projects").unwrap();
        let inbox = doc.create_root("Inbox").unwrap();
        let omatree = doc.create_child(projects, "OmaTree").unwrap();
        let ideas = doc.create_child(omatree, "Ideas").unwrap();
        doc.create_child(projects, "Second").unwrap();
        doc.set_body(ideas, "line 1\nline 2").unwrap();
        doc.set_body(inbox, "inbox body").unwrap();
        doc.save().unwrap();
        drop(doc);

        let mut doc = Document::open(file.path()).unwrap();
        assert!(!doc.is_dirty());
        let nb = doc.notebook();
        assert_eq!(titles(nb, None), ["Projects", "Inbox"]);
        assert_eq!(titles(nb, Some(projects)), ["OmaTree", "Second"]);
        assert_eq!(titles(nb, Some(omatree)), ["Ideas"]);
        assert_eq!(nb.get(ideas).unwrap().body(), "line 1\nline 2");
        assert_eq!(nb.get(inbox).unwrap().body(), "inbox body");
        // New ids continue after the loaded ones.
        let fresh = doc.create_root("new").unwrap();
        assert!(fresh > inbox && fresh > ideas);
    }

    #[test]
    fn dirty_follows_successful_mutations_and_saves() {
        let file = TempFile::new();
        let mut doc = Document::open(file.path()).unwrap();
        assert!(!doc.is_dirty());

        let a = doc.create_root("a").unwrap();
        assert!(doc.is_dirty());
        doc.save().unwrap();
        assert!(!doc.is_dirty());

        doc.set_body(a, "text").unwrap();
        assert!(doc.is_dirty(), "edit after save makes it dirty again");
        doc.save().unwrap();

        doc.rename(a, "b").unwrap();
        assert!(doc.is_dirty());
        doc.save().unwrap();

        let child = doc.create_child(a, "c").unwrap();
        assert!(doc.is_dirty());
        doc.save().unwrap();

        doc.delete(child).unwrap();
        assert!(doc.is_dirty());
    }

    #[test]
    fn rename_and_set_body_report_whether_anything_changed() {
        let mut doc = Document::untitled();
        let a = doc.create_root("a").unwrap();
        assert!(doc.rename(a, "b").unwrap());
        assert!(!doc.rename(a, "b").unwrap(), "same title");
        assert!(doc.set_body(a, "text").unwrap());
        assert!(!doc.set_body(a, "text").unwrap(), "same body");
        let gone = doc.create_root("gone").unwrap();
        doc.delete(gone).unwrap();
        assert!(doc.rename(gone, "x").is_err());
        assert!(doc.set_body(gone, "x").is_err());
    }

    #[test]
    fn failed_or_noop_mutations_do_not_mark_dirty() {
        let file = TempFile::new();
        let mut doc = Document::open(file.path()).unwrap();
        let a = doc.create_root("a").unwrap();
        doc.set_body(a, "same").unwrap();
        doc.save().unwrap();

        let missing = {
            let gone = doc.create_root("gone").unwrap();
            doc.delete(gone).unwrap();
            doc.save().unwrap();
            gone
        };
        assert!(doc.create_child(missing, "x").is_err());
        assert!(doc.rename(missing, "x").is_err());
        assert!(doc.set_body(missing, "x").is_err());
        assert!(doc.delete(missing).is_err());
        assert!(!doc.is_dirty());

        doc.rename(a, "a").unwrap();
        doc.set_body(a, "same").unwrap();
        assert!(!doc.is_dirty(), "writing identical text is not a change");
    }

    #[cfg(unix)]
    #[test]
    fn failed_save_leaves_dirty_set_and_file_intact() {
        use std::os::unix::fs::PermissionsExt;

        let file = TempFile::new();
        {
            let mut doc = Document::open(file.path()).unwrap();
            doc.create_root("kept").unwrap();
            doc.save().unwrap();
        }
        std::fs::set_permissions(file.path(), std::fs::Permissions::from_mode(0o444)).unwrap();
        // Root bypasses file permissions, so the failure can't be provoked.
        if std::fs::OpenOptions::new()
            .write(true)
            .open(file.path())
            .is_ok()
        {
            return;
        }

        let mut doc = Document::open(file.path()).unwrap();
        doc.create_root("unsaved").unwrap();
        assert!(doc.save().is_err());
        assert!(doc.is_dirty());
        drop(doc);

        let nb = Storage::open(file.path()).unwrap().load().unwrap();
        assert_eq!(titles(&nb, None), ["kept"]);
    }

    #[test]
    fn corrupt_file_fails_to_open_and_is_not_modified() {
        let file = TempFile::new();
        let junk = b"this is definitely not a sqlite database, just some text".to_vec();
        std::fs::write(file.path(), &junk).unwrap();
        let err = Document::open(file.path()).err().expect("must fail");
        assert!(matches!(
            err,
            DocumentError::Storage(StorageError::Sqlite(_))
        ));
        assert!(!err.open_message().is_empty());
        assert_eq!(std::fs::read(file.path()).unwrap(), junk);
    }

    #[test]
    fn unsupported_schema_version_fails_to_open_and_is_not_modified() {
        let file = TempFile::new();
        Storage::create(file.path()).unwrap();
        rusqlite::Connection::open(file.path())
            .unwrap()
            .pragma_update(None, "user_version", 99)
            .unwrap();
        let before = std::fs::read(file.path()).unwrap();
        let err = Document::open(file.path()).err().expect("must fail");
        assert!(matches!(
            err,
            DocumentError::Storage(StorageError::UnsupportedSchemaVersion(99))
        ));
        assert_eq!(std::fs::read(file.path()).unwrap(), before);
    }

    #[test]
    fn empty_existing_file_is_rejected_not_adopted() {
        let file = TempFile::new();
        std::fs::write(file.path(), b"").unwrap();
        assert!(Document::open(file.path()).is_err());
        assert_eq!(std::fs::metadata(file.path()).unwrap().len(), 0);
    }

    #[test]
    fn directory_path_fails_cleanly() {
        assert!(Document::open(&std::env::temp_dir()).is_err());
    }

    #[test]
    fn save_message_for_untitled_mentions_no_file() {
        assert!(DocumentError::NoPath.save_message().contains("no file"));
    }

    // ---- Save As / Open existing ----

    /// A unique directory in the temp dir, removed (recursively) on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static COUNTER: AtomicU32 = AtomicU32::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir =
                std::env::temp_dir().join(format!("omatree-doc-dir-{}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }

        fn join(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            if let Ok(entries) = std::fs::read_dir(&self.0) {
                for entry in entries.flatten() {
                    if let Ok(meta) = entry.metadata() {
                        let mut perms = meta.permissions();
                        #[allow(clippy::permissions_set_readonly_false)]
                        perms.set_readonly(false);
                        let _ = std::fs::set_permissions(entry.path(), perms);
                    }
                }
            }
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn stored_titles(path: &Path) -> Vec<String> {
        titles(&Storage::open(path).unwrap().load().unwrap(), None)
    }

    #[test]
    fn save_as_from_untitled_creates_file_and_switches_path() {
        let dir = TempDir::new();
        let target = dir.join("first.omatree");
        let mut doc = Document::untitled();
        let a = doc.create_root("Projects").unwrap();
        let child = doc.create_child(a, "Child").unwrap();
        doc.set_body(child, "child body").unwrap();
        assert!(doc.is_dirty());

        doc.save_as(&target, false).unwrap();
        assert!(doc.has_path());
        assert!(doc.is_current_path(&target));
        assert_eq!(doc.display_name(), "first.omatree");
        assert!(!doc.is_dirty());
        drop(doc);

        let reopened = Document::open_existing(&target).unwrap();
        assert_eq!(titles(reopened.notebook(), None), ["Projects"]);
        assert_eq!(reopened.notebook().get(child).unwrap().body(), "child body");
    }

    #[test]
    fn save_as_failure_keeps_path_dirty_and_creates_nothing() {
        let dir = TempDir::new();
        let original = dir.join("orig.omatree");
        let mut doc = Document::open(&original).unwrap();
        doc.create_root("unsaved").unwrap();

        // The target's directory does not exist, so creation fails.
        let bad = dir.join("no-such-dir").join("x.omatree");
        assert!(doc.save_as(&bad, false).is_err());
        assert!(doc.is_dirty());
        assert!(doc.is_current_path(&original));
        assert_eq!(doc.display_name(), "orig.omatree");
        assert!(!bad.exists());

        // The document still saves to its real file afterwards.
        doc.save().unwrap();
        assert_eq!(stored_titles(&original), ["unsaved"]);

        // Untitled stays untitled after a failed Save As.
        let mut untitled = Document::untitled();
        untitled.create_root("x").unwrap();
        assert!(untitled.save_as(&bad, false).is_err());
        assert!(!untitled.has_path());
        assert!(untitled.is_dirty());
    }

    #[test]
    fn save_as_to_a_new_path_leaves_the_original_file_unchanged() {
        let dir = TempDir::new();
        let first = dir.join("first.omatree");
        let second = dir.join("second.omatree");
        let mut doc = Document::open(&first).unwrap();
        doc.create_root("one").unwrap();
        doc.save().unwrap();
        let before = std::fs::read(&first).unwrap();

        doc.create_root("two").unwrap();
        doc.save_as(&second, false).unwrap();
        assert!(doc.is_current_path(&second));
        assert_eq!(std::fs::read(&first).unwrap(), before);
        assert_eq!(stored_titles(&first), ["one"]);
        assert_eq!(stored_titles(&second), ["one", "two"]);

        // Later saves go to the new file only.
        doc.create_root("three").unwrap();
        doc.save().unwrap();
        assert_eq!(stored_titles(&first), ["one"]);
        assert_eq!(stored_titles(&second), ["one", "two", "three"]);
    }

    #[test]
    fn save_as_to_the_current_path_is_a_normal_save() {
        let dir = TempDir::new();
        let path = dir.join("same.omatree");
        let mut doc = Document::open(&path).unwrap();
        doc.create_root("kept").unwrap();
        assert!(!doc.save_as_needs_confirmation(&path));
        // No overwrite flag needed, and a different spelling of the same file works too.
        let respelled = dir.join(".").join("same.omatree");
        assert!(!doc.save_as_needs_confirmation(&respelled));
        doc.save_as(&respelled, false).unwrap();
        assert!(!doc.is_dirty());
        assert_eq!(stored_titles(&path), ["kept"]);
    }

    #[test]
    fn save_as_onto_existing_file_requires_explicit_overwrite() {
        let dir = TempDir::new();
        let target = dir.join("target.omatree");
        {
            let mut other = Document::open(&target).unwrap();
            other.create_root("old content").unwrap();
            other.save().unwrap();
        }
        let before = std::fs::read(&target).unwrap();

        let mut doc = Document::untitled();
        doc.create_root("new content").unwrap();
        assert!(doc.save_as_needs_confirmation(&target));
        assert!(matches!(
            doc.save_as(&target, false),
            Err(DocumentError::TargetExists)
        ));
        assert!(doc.is_dirty() && !doc.has_path());
        assert_eq!(std::fs::read(&target).unwrap(), before);

        doc.save_as(&target, true).unwrap();
        assert!(!doc.is_dirty());
        assert!(doc.is_current_path(&target));
        assert_eq!(stored_titles(&target), ["new content"]);
    }

    #[cfg(unix)]
    #[test]
    fn failed_overwrite_leaves_the_existing_target_valid_and_unchanged() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new();
        let target = dir.join("locked.omatree");
        {
            let mut other = Document::open(&target).unwrap();
            other.create_root("precious").unwrap();
            other.save().unwrap();
        }
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o444)).unwrap();
        if std::fs::OpenOptions::new()
            .write(true)
            .open(&target)
            .is_ok()
        {
            return; // running as root: permissions can't force a failure
        }
        let before = std::fs::read(&target).unwrap();

        let mut doc = Document::untitled();
        doc.create_root("replacement").unwrap();
        assert!(doc.save_as(&target, true).is_err());
        assert!(doc.is_dirty() && !doc.has_path());
        assert_eq!(std::fs::read(&target).unwrap(), before);
        assert_eq!(stored_titles(&target), ["precious"]);
    }

    #[test]
    fn save_as_refuses_to_overwrite_non_notebook_files() {
        let dir = TempDir::new();
        let text = dir.join("notes.txt");
        let junk = b"ordinary text file, not a database".to_vec();
        std::fs::write(&text, &junk).unwrap();

        let empty = dir.join("empty.omatree");
        std::fs::write(&empty, b"").unwrap();

        let future = dir.join("future.omatree");
        Storage::create(&future).unwrap();
        rusqlite::Connection::open(&future)
            .unwrap()
            .pragma_update(None, "user_version", 99)
            .unwrap();
        let future_bytes = std::fs::read(&future).unwrap();

        let mut doc = Document::untitled();
        doc.create_root("mine").unwrap();
        for (target, expected) in [
            (&text, junk.clone()),
            (&empty, Vec::new()),
            (&future, future_bytes),
        ] {
            let err = doc.save_as(target, true).unwrap_err();
            assert!(matches!(err, DocumentError::NotANotebook(_)), "{err}");
            assert!(!err.save_as_message().is_empty());
            assert_eq!(&std::fs::read(target).unwrap(), &expected);
            assert!(doc.is_dirty() && !doc.has_path());
        }
    }

    #[test]
    fn open_existing_never_creates_and_failure_touches_nothing() {
        let dir = TempDir::new();
        let missing = dir.join("missing.omatree");
        assert!(Document::open_existing(&missing).is_err());
        assert!(!missing.exists());

        let junk = dir.join("junk.omatree");
        std::fs::write(&junk, b"not sqlite").unwrap();
        assert!(Document::open_existing(&junk).is_err());
        assert_eq!(std::fs::read(&junk).unwrap(), b"not sqlite");
    }

    #[test]
    fn save_as_needs_confirmation_only_for_other_existing_files() {
        let dir = TempDir::new();
        let existing = dir.join("existing.omatree");
        Storage::create(&existing).unwrap();
        let fresh = dir.join("fresh.omatree");
        let doc = Document::untitled();
        assert!(doc.save_as_needs_confirmation(&existing));
        assert!(!doc.save_as_needs_confirmation(&fresh));
    }

    #[test]
    fn default_extension_is_appended_only_when_missing() {
        let add = |p: &str| with_default_extension(Path::new(p));
        assert_eq!(add("/tmp/notes"), PathBuf::from("/tmp/notes.omatree"));
        assert_eq!(
            add("/tmp/notes.omatree"),
            PathBuf::from("/tmp/notes.omatree")
        );
        assert_eq!(add("/tmp/notes.db"), PathBuf::from("/tmp/notes.db"));
        assert_eq!(add("/tmp/my.notes"), PathBuf::from("/tmp/my.notes"));
        assert_eq!(add("/tmp/.hidden"), PathBuf::from("/tmp/.hidden.omatree"));
    }

    // ---- Trash and checkpoints ----

    fn sorted(mut nodes: Vec<crate::notebook::Node>) -> Vec<crate::notebook::Node> {
        nodes.sort_by_key(|n| n.id());
        nodes
    }

    /// Projects
    /// ├── OmaTree        (body "omatree body")
    /// │   └── Ideas      (body "ideas body")
    /// └── Threatwright
    /// Inbox
    fn sample() -> (Document, [NodeId; 5]) {
        let mut doc = Document::untitled();
        let projects = doc.create_root("Projects").unwrap();
        let inbox = doc.create_root("Inbox").unwrap();
        let omatree = doc.create_child(projects, "OmaTree").unwrap();
        let threat = doc.create_child(projects, "Threatwright").unwrap();
        let ideas = doc.create_child(omatree, "Ideas").unwrap();
        doc.set_body(omatree, "omatree body").unwrap();
        doc.set_body(ideas, "ideas body").unwrap();
        (doc, [projects, inbox, omatree, threat, ideas])
    }

    #[test]
    fn deleting_a_leaf_moves_it_to_trash() {
        let (mut doc, [_, _, _, threat, _]) = sample();
        doc.delete(threat).unwrap();
        assert!(doc.notebook().get(threat).is_none());
        let trash = doc.recovery().trash();
        assert_eq!(trash.len(), 1);
        assert_eq!(trash[0].title(), "Threatwright");
        assert_eq!(trash[0].node_count(), 1);
        assert!(trash[0].deleted_at() > 0);
        assert!(doc.is_dirty());
    }

    #[test]
    fn deleting_a_parent_stores_the_whole_subtree() {
        let (mut doc, [projects, inbox, omatree, threat, ideas]) = sample();
        doc.delete(projects).unwrap();
        assert_eq!(titles(doc.notebook(), None), ["Inbox"]);
        let entry = &doc.recovery().trash()[0];
        assert_eq!(entry.node_count(), 4);
        let ids: Vec<NodeId> = sorted(entry.nodes().to_vec())
            .iter()
            .map(|n| n.id())
            .collect();
        assert_eq!(ids, [projects, omatree, threat, ideas]);
        assert!(doc.notebook().get(inbox).is_some());
    }

    #[test]
    fn trash_preserves_ids_titles_bodies_hierarchy_and_original_placement() {
        let (mut doc, [projects, _, omatree, _, ideas]) = sample();
        let before = doc.notebook().subtree(projects).unwrap();
        doc.delete(projects).unwrap();
        let entry = &doc.recovery().trash()[0];
        assert_eq!(entry.nodes()[0], before[0], "root first");
        assert_eq!(sorted(entry.nodes().to_vec()), sorted(before));
        assert_eq!(entry.root().id(), projects);
        assert_eq!(entry.root().parent_id(), None);
        assert_eq!(entry.root().position(), 0);
        let find = |id| entry.nodes().iter().find(|n| n.id() == id).unwrap();
        assert_eq!(find(omatree).parent_id(), Some(projects));
        assert_eq!(find(ideas).parent_id(), Some(omatree));
        assert_eq!(find(ideas).body(), "ideas body");
        assert_eq!(find(omatree).title(), "OmaTree");
    }

    #[test]
    fn trash_survives_save_and_reopen() {
        let dir = TempDir::new();
        let path = dir.join("t.omatree");
        let (mut doc, [projects, ..]) = sample();
        doc.delete(projects).unwrap();
        let saved = doc.recovery().trash()[0].clone();
        doc.save_as(&path, false).unwrap();
        doc.save().unwrap();
        drop(doc);

        let reopened = Document::open_existing(&path).unwrap();
        assert_eq!(reopened.recovery().trash(), &[saved]);
        assert!(!reopened.is_dirty());
    }

    #[test]
    fn restoring_trash_reproduces_the_subtree() {
        let (mut doc, [projects, ..]) = sample();
        let before = doc.notebook().snapshot_nodes();
        let subtree_before = doc.notebook().subtree(projects).unwrap();
        doc.delete(projects).unwrap();
        let restored = doc.restore_trash(0).unwrap();
        assert_eq!(restored, projects);
        assert_eq!(
            sorted(doc.notebook().subtree(projects).unwrap()),
            sorted(subtree_before)
        );
        assert_eq!(sorted(doc.notebook().snapshot_nodes()), sorted(before));
    }

    #[test]
    fn restore_uses_the_original_parent_and_position() {
        let mut doc = Document::untitled();
        let parent = doc.create_root("P").unwrap();
        let a = doc.create_child(parent, "a").unwrap();
        let b = doc.create_child(parent, "b").unwrap();
        let c = doc.create_child(parent, "c").unwrap();
        doc.delete(b).unwrap();
        assert_eq!(doc.plan_trash_restore(0).unwrap(), (Some(parent), 1));
        doc.restore_trash(0).unwrap();
        assert_eq!(titles(doc.notebook(), Some(parent)), ["a", "b", "c"]);
        let positions: Vec<usize> = doc
            .notebook()
            .children(parent)
            .unwrap()
            .iter()
            .map(|n| n.position())
            .collect();
        assert_eq!(positions, [0, 1, 2]);
        assert_eq!(doc.notebook().get(b).unwrap().parent_id(), Some(parent));
        let _ = (a, c);
    }

    #[test]
    fn restore_falls_back_to_the_end_of_the_roots_if_the_parent_is_gone() {
        let mut doc = Document::untitled();
        let parent = doc.create_root("P").unwrap();
        doc.create_root("other").unwrap();
        let child = doc.create_child(parent, "child").unwrap();
        doc.delete(child).unwrap();
        doc.delete(parent).unwrap(); // its subtree no longer contains `child`
        doc.create_root("later").unwrap();
        // Newest Trash entry is P; restore the older one (the child).
        let index = doc
            .recovery()
            .trash()
            .iter()
            .position(|e| e.title() == "child")
            .unwrap();
        assert_eq!(doc.plan_trash_restore(index).unwrap(), (None, 2));
        doc.restore_trash(index).unwrap();
        assert_eq!(titles(doc.notebook(), None), ["other", "later", "child"]);
        assert_eq!(doc.notebook().get(child).unwrap().parent_id(), None);
    }

    #[test]
    fn restore_clamps_an_unavailable_sibling_position() {
        let mut doc = Document::untitled();
        let parent = doc.create_root("P").unwrap();
        let a = doc.create_child(parent, "a").unwrap();
        let b = doc.create_child(parent, "b").unwrap();
        let c = doc.create_child(parent, "c").unwrap();
        doc.delete(c).unwrap(); // position 2
        doc.delete(a).unwrap();
        doc.delete(b).unwrap();
        assert!(doc.notebook().children(parent).unwrap().is_empty());
        let index = doc
            .recovery()
            .trash()
            .iter()
            .position(|e| e.title() == "c")
            .unwrap();
        assert_eq!(doc.plan_trash_restore(index).unwrap(), (Some(parent), 0));
        doc.restore_trash(index).unwrap();
        assert_eq!(titles(doc.notebook(), Some(parent)), ["c"]);
        assert_eq!(doc.notebook().get(c).unwrap().position(), 0);
    }

    #[test]
    fn a_deleted_root_returns_to_its_original_place_among_the_roots() {
        let mut doc = Document::untitled();
        doc.create_root("first").unwrap();
        let middle = doc.create_root("middle").unwrap();
        doc.create_root("last").unwrap();
        doc.delete(middle).unwrap();
        assert_eq!(doc.plan_trash_restore(0).unwrap(), (None, 1));
        doc.restore_trash(0).unwrap();
        assert_eq!(titles(doc.notebook(), None), ["first", "middle", "last"]);
    }

    #[test]
    fn restoring_removes_the_entry_from_trash() {
        let (mut doc, [_, _, _, threat, _]) = sample();
        doc.delete(threat).unwrap();
        doc.restore_trash(0).unwrap();
        assert!(doc.recovery().trash().is_empty());
        assert!(matches!(
            doc.restore_trash(0),
            Err(RecoveryError::NoSuchEntry)
        ));
    }

    #[test]
    fn restore_id_collisions_fail_cleanly() {
        let (mut doc, [projects, ..]) = sample();
        doc.delete(projects).unwrap();
        // Restoring the pre-delete checkpoint brings the same ids back.
        doc.restore_checkpoint(0).unwrap();
        assert!(doc.notebook().get(projects).is_some());

        let notebook_before = doc.notebook().snapshot_nodes();
        let trash_before = doc.recovery().trash().to_vec();
        let checkpoints_before = doc.recovery().checkpoints().len();
        doc.save_as_dirty_reset_for_test();

        assert_eq!(doc.plan_trash_restore(0), Err(RecoveryError::IdCollision));
        assert_eq!(doc.restore_trash(0), Err(RecoveryError::IdCollision));
        assert_eq!(doc.notebook().snapshot_nodes(), notebook_before);
        assert_eq!(doc.recovery().trash(), &trash_before[..]);
        assert_eq!(doc.recovery().checkpoints().len(), checkpoints_before);
        assert!(
            !doc.is_dirty(),
            "a failed restore must not mark the document dirty"
        );
    }

    #[test]
    fn node_ids_are_not_reused_after_delete_save_and_reopen() {
        let dir = TempDir::new();
        let path = dir.join("ids.omatree");
        let mut doc = Document::open(&path).unwrap();
        let a = doc.create_root("a").unwrap();
        let b = doc.create_root("b").unwrap();
        doc.delete(b).unwrap(); // the highest id now lives only in Trash
        doc.save().unwrap();
        drop(doc);

        let mut doc = Document::open_existing(&path).unwrap();
        let c = doc.create_root("c").unwrap();
        assert!(c > b && c > a, "{c:?} must be above {b:?}");
        doc.save().unwrap();
        drop(doc);

        let doc = Document::open_existing(&path).unwrap();
        assert_eq!(doc.recovery().trash()[0].root().id(), b);
        assert!(doc.notebook().get(b).is_none());
    }

    #[test]
    fn a_checkpoint_is_created_before_delete_and_holds_the_exact_prior_tree() {
        let (mut doc, [projects, ..]) = sample();
        assert!(doc.recovery().checkpoints().is_empty());
        let before = doc.notebook().snapshot_nodes();
        doc.delete(projects).unwrap();
        let checkpoints = doc.recovery().checkpoints();
        assert_eq!(checkpoints.len(), 1);
        assert_eq!(checkpoints[0].reason(), "Before deleting \"Projects\"");
        assert!(checkpoints[0].created_at() > 0);
        assert_eq!(sorted(checkpoints[0].nodes().to_vec()), sorted(before));
    }

    #[test]
    fn restoring_a_checkpoint_restores_the_previous_tree_and_keeps_trash() {
        let (mut doc, [projects, ..]) = sample();
        let before = doc.notebook().snapshot_nodes();
        doc.delete(projects).unwrap();
        doc.delete(doc.notebook().roots()[0].id()).unwrap(); // also delete Inbox
        assert!(doc.notebook().roots().is_empty());

        // Newest-first would be index 1; stored oldest-first, "Before
        // deleting Projects" is index 0.
        doc.restore_checkpoint(0).unwrap();
        assert_eq!(sorted(doc.notebook().snapshot_nodes()), sorted(before));
        assert_eq!(doc.recovery().trash().len(), 2, "Trash stays intact");
        assert!(doc.is_dirty());
    }

    #[test]
    fn restoring_a_checkpoint_checkpoints_the_state_being_replaced() {
        let (mut doc, [projects, ..]) = sample();
        doc.delete(projects).unwrap();
        let replaced = doc.notebook().snapshot_nodes();
        let count = doc.recovery().checkpoints().len();
        doc.restore_checkpoint(0).unwrap();
        let checkpoints = doc.recovery().checkpoints();
        assert_eq!(checkpoints.len(), count + 1);
        let newest = checkpoints.last().unwrap();
        assert_eq!(
            newest.reason(),
            "Before restoring checkpoint \"Before deleting \"Projects\"\""
        );
        assert_eq!(sorted(newest.nodes().to_vec()), sorted(replaced.clone()));

        // So the restore itself can be undone.
        doc.restore_checkpoint(checkpoints.len() - 1).unwrap();
        assert_eq!(sorted(doc.notebook().snapshot_nodes()), sorted(replaced));
    }

    #[test]
    fn restoring_trash_checkpoints_the_state_being_replaced() {
        let (mut doc, [_, _, _, threat, _]) = sample();
        doc.delete(threat).unwrap();
        let before_restore = doc.notebook().snapshot_nodes();
        doc.restore_trash(0).unwrap();
        let newest = doc.recovery().checkpoints().last().unwrap();
        assert_eq!(
            newest.reason(),
            "Before restoring \"Threatwright\" from Trash"
        );
        assert_eq!(sorted(newest.nodes().to_vec()), sorted(before_restore));
    }

    #[test]
    fn ordinary_edits_and_renames_create_no_checkpoints() {
        let (mut doc, [projects, _, omatree, ..]) = sample();
        doc.set_body(projects, "typing, typing, typing").unwrap();
        doc.set_body(projects, "more typing").unwrap();
        doc.rename(omatree, "OmaTree 2").unwrap();
        doc.create_root("another").unwrap();
        assert!(doc.recovery().checkpoints().is_empty());
        assert!(doc.recovery().trash().is_empty());
    }

    #[test]
    fn checkpoint_history_is_capped_at_the_newest_100() {
        let mut doc = Document::untitled();
        for i in 0..105 {
            doc.create_checkpoint(&format!("c{i}"));
        }
        let checkpoints = doc.recovery().checkpoints();
        assert_eq!(checkpoints.len(), 100);
        assert_eq!(checkpoints[0].reason(), "c5");
        assert_eq!(checkpoints[99].reason(), "c104");
    }

    #[test]
    fn save_as_preserves_trash_and_checkpoints() {
        let dir = TempDir::new();
        let first = dir.join("first.omatree");
        let second = dir.join("second.omatree");
        let (mut doc, [projects, ..]) = sample();
        doc.save_as(&first, false).unwrap();
        doc.delete(projects).unwrap();
        doc.save().unwrap();
        doc.save_as(&second, false).unwrap();
        let trash = doc.recovery().trash().to_vec();
        let checkpoints = doc.recovery().checkpoints().to_vec();
        drop(doc);

        for path in [&second, &first] {
            let reopened = Document::open_existing(path).unwrap();
            assert_eq!(reopened.recovery().trash(), &trash[..], "{path:?}");
            assert_eq!(
                reopened.recovery().checkpoints(),
                &checkpoints[..],
                "{path:?}"
            );
        }
    }

    #[test]
    fn recovery_operations_set_dirty_and_inspecting_does_not() {
        let dir = TempDir::new();
        let path = dir.join("dirty.omatree");
        let (mut doc, [projects, ..]) = sample();
        doc.save_as(&path, false).unwrap();
        assert!(!doc.is_dirty());

        let _ = (doc.recovery().trash(), doc.recovery().checkpoints());
        let _ = doc.plan_trash_restore(0);
        assert!(!doc.is_dirty(), "looking at recovery state is not a change");

        doc.delete(projects).unwrap();
        assert!(doc.is_dirty(), "delete");
        doc.save().unwrap();

        doc.restore_trash(0).unwrap();
        assert!(doc.is_dirty(), "restore from Trash");
        doc.save().unwrap();

        doc.restore_checkpoint(0).unwrap();
        assert!(doc.is_dirty(), "restore checkpoint");
        doc.save().unwrap();
        assert!(!doc.is_dirty());
    }

    #[test]
    fn failed_recovery_operations_change_nothing() {
        let (mut doc, [projects, ..]) = sample();
        doc.delete(projects).unwrap();
        doc.save_as_dirty_reset_for_test();
        assert_eq!(doc.restore_trash(7), Err(RecoveryError::NoSuchEntry));
        assert_eq!(doc.restore_checkpoint(7), Err(RecoveryError::NoSuchEntry));
        assert!(!doc.is_dirty());
        assert_eq!(doc.recovery().checkpoints().len(), 1);
        // Deleting a missing node does not touch recovery state either.
        let gone = projects;
        assert!(doc.delete(gone).is_err());
        assert_eq!(doc.recovery().trash().len(), 1);
        assert_eq!(doc.recovery().checkpoints().len(), 1);
    }

    // ---- dirty domains and save scope ----

    use crate::storage::{dump_table, install_write_guards, remove_write_guards, RECOVERY_TABLES};

    fn recovery_dumps(path: &Path) -> Vec<Vec<String>> {
        RECOVERY_TABLES
            .iter()
            .map(|t| dump_table(path, t))
            .collect()
    }

    fn file_user_version(path: &Path) -> i64 {
        rusqlite::Connection::open(path)
            .unwrap()
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap()
    }

    fn table_exists(path: &Path, table: &str) -> bool {
        let conn = rusqlite::Connection::open(path).unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |r| r.get(0),
            )
            .unwrap();
        n == 1
    }

    /// A saved, clean version 2 notebook whose Trash and checkpoints are not
    /// empty: Projects was deleted, Inbox and a note remain.
    fn file_with_recovery_history(dir: &TempDir, name: &str) -> PathBuf {
        let path = dir.join(name);
        let mut doc = Document::open(&path).unwrap();
        let projects = doc.create_root("Projects").unwrap();
        doc.create_child(projects, "Child").unwrap();
        doc.create_root("Inbox").unwrap();
        doc.delete(projects).unwrap();
        let note = doc.create_root("Note").unwrap();
        doc.set_body(note, "body").unwrap();
        doc.save().unwrap();
        assert_eq!(doc.dirty_domains(), (false, false));
        assert!(!doc.recovery().trash().is_empty());
        assert!(!doc.recovery().checkpoints().is_empty());
        path
    }

    fn clean_sample() -> (Document, [NodeId; 5]) {
        let (mut doc, ids) = sample();
        doc.save_as_dirty_reset_for_test();
        (doc, ids)
    }

    #[test]
    fn ordinary_edits_mark_only_the_active_state_dirty() {
        let (mut doc, [projects, _, omatree, ..]) = clean_sample();
        assert_eq!(doc.dirty_domains(), (false, false));

        doc.set_body(projects, "edited").unwrap();
        assert_eq!(doc.dirty_domains(), (true, false), "body edit");

        doc.save_as_dirty_reset_for_test();
        doc.rename(omatree, "Renamed").unwrap();
        assert_eq!(doc.dirty_domains(), (true, false), "rename");

        doc.save_as_dirty_reset_for_test();
        doc.create_root("another").unwrap();
        assert_eq!(doc.dirty_domains(), (true, false), "create root");

        doc.save_as_dirty_reset_for_test();
        doc.create_child(projects, "child").unwrap();
        assert_eq!(doc.dirty_domains(), (true, false), "create child");
        assert!(doc.is_dirty());
    }

    #[test]
    fn structural_operations_mark_both_states_dirty() {
        let (mut doc, [projects, _, _, threat, _]) = clean_sample();
        doc.delete(threat).unwrap();
        assert_eq!(doc.dirty_domains(), (true, true), "delete");

        doc.save_as_dirty_reset_for_test();
        doc.restore_trash(0).unwrap();
        assert_eq!(doc.dirty_domains(), (true, true), "restore from Trash");

        doc.save_as_dirty_reset_for_test();
        doc.restore_checkpoint(0).unwrap();
        assert_eq!(doc.dirty_domains(), (true, true), "restore checkpoint");

        doc.save_as_dirty_reset_for_test();
        let _ = projects;
        doc.create_checkpoint("explicit, for a future structural operation");
        assert_eq!(
            doc.dirty_domains(),
            (false, true),
            "a bare checkpoint is recovery state"
        );
    }

    #[test]
    fn unchanged_and_failed_mutations_leave_both_flags_alone() {
        let (mut doc, [projects, _, omatree, ..]) = clean_sample();
        let title = doc.notebook().get(omatree).unwrap().title().to_string();
        let body = doc.notebook().get(omatree).unwrap().body().to_string();
        doc.rename(omatree, &title).unwrap();
        doc.set_body(omatree, &body).unwrap();
        assert_eq!(doc.dirty_domains(), (false, false), "identical text");

        doc.delete(projects).unwrap();
        doc.save_as_dirty_reset_for_test();
        let missing = projects; // now in Trash, not active
        assert!(doc.create_child(missing, "x").is_err());
        assert!(doc.rename(missing, "x").is_err());
        assert!(doc.set_body(missing, "x").is_err());
        assert!(doc.delete(missing).is_err());
        assert!(doc.restore_trash(9).is_err());
        assert!(doc.restore_checkpoint(9).is_err());
        assert_eq!(doc.dirty_domains(), (false, false), "failed operations");
    }

    #[test]
    fn an_ordinary_save_writes_only_active_state() {
        let dir = TempDir::new();
        let path = file_with_recovery_history(&dir, "a.omatree");
        let before = recovery_dumps(&path);
        install_write_guards(&path, &RECOVERY_TABLES);

        let mut doc = Document::open_existing(&path).unwrap();
        let root = doc.notebook().roots()[0].id();
        doc.set_body(root, "edited body").unwrap();
        doc.rename(root, "Renamed").unwrap();
        let fresh = doc.create_root("Fresh").unwrap();
        doc.create_child(fresh, "Fresh child").unwrap();
        assert_eq!(doc.dirty_domains(), (true, false));

        doc.save()
            .expect("an ordinary save must not touch recovery tables");
        assert_eq!(
            doc.dirty_domains(),
            (false, false),
            "clears the active flag"
        );
        drop(doc);

        assert_eq!(recovery_dumps(&path), before, "rows and rowids unchanged");
        let reopened = Document::open_existing(&path).unwrap();
        assert_eq!(reopened.notebook().get(root).unwrap().body(), "edited body");
        assert_eq!(reopened.notebook().get(root).unwrap().title(), "Renamed");
        assert_eq!(titles(reopened.notebook(), Some(fresh)), ["Fresh child"]);
        assert_eq!(reopened.recovery().trash().len(), 1);
    }

    #[test]
    fn trash_and_checkpoints_survive_many_ordinary_saves() {
        let dir = TempDir::new();
        let path = file_with_recovery_history(&dir, "many.omatree");
        let original = Document::open_existing(&path).unwrap();
        let (trash, checkpoints) = (
            original.recovery().trash().to_vec(),
            original.recovery().checkpoints().to_vec(),
        );
        drop(original);
        install_write_guards(&path, &RECOVERY_TABLES);

        let mut doc = Document::open_existing(&path).unwrap();
        for i in 0..6 {
            doc.create_root(&format!("n{i}")).unwrap();
            doc.save().unwrap();
            let root = doc.notebook().roots()[0].id();
            doc.set_body(root, &format!("edit {i}")).unwrap();
            doc.save().unwrap();
        }
        drop(doc);

        let reopened = Document::open_existing(&path).unwrap();
        assert_eq!(reopened.recovery().trash(), &trash[..]);
        assert_eq!(reopened.recovery().checkpoints(), &checkpoints[..]);
        assert_eq!(reopened.notebook().roots().len(), 2 + 6);
    }

    #[test]
    fn delete_then_save_is_a_full_save_clearing_both_flags() {
        let dir = TempDir::new();
        let path = file_with_recovery_history(&dir, "d.omatree");
        let mut doc = Document::open_existing(&path).unwrap();
        let (trash_before, cps_before) = (
            doc.recovery().trash().len(),
            doc.recovery().checkpoints().len(),
        );
        let victim = doc.notebook().roots()[0].id();
        doc.delete(victim).unwrap();
        assert_eq!(doc.dirty_domains(), (true, true));
        doc.save().unwrap();
        assert_eq!(doc.dirty_domains(), (false, false));
        drop(doc);

        let reopened = Document::open_existing(&path).unwrap();
        assert_eq!(reopened.recovery().trash().len(), trash_before + 1);
        assert_eq!(reopened.recovery().checkpoints().len(), cps_before + 1);
        assert!(reopened.notebook().get(victim).is_none());
    }

    #[test]
    fn recovery_dirty_alone_forces_a_full_save() {
        let dir = TempDir::new();
        let path = file_with_recovery_history(&dir, "r.omatree");
        let mut doc = Document::open_existing(&path).unwrap();
        doc.create_checkpoint("manual checkpoint");
        assert_eq!(doc.dirty_domains(), (false, true));
        doc.save().unwrap();
        assert_eq!(doc.dirty_domains(), (false, false));
        drop(doc);
        let reopened = Document::open_existing(&path).unwrap();
        assert_eq!(
            reopened.recovery().checkpoints().last().unwrap().reason(),
            "manual checkpoint"
        );
    }

    #[test]
    fn a_failed_full_save_leaves_both_flags_set_and_the_file_unchanged() {
        let dir = TempDir::new();
        let path = file_with_recovery_history(&dir, "ff.omatree");
        install_write_guards(&path, &["checkpoints"]); // the full save will hit this
        let before = std::fs::read(&path).unwrap();

        let mut doc = Document::open_existing(&path).unwrap();
        let victim = doc.notebook().roots()[0].id();
        doc.delete(victim).unwrap();
        assert!(doc.save().is_err());
        assert_eq!(
            doc.dirty_domains(),
            (true, true),
            "flags survive the failure"
        );
        assert_eq!(std::fs::read(&path).unwrap(), before, "all-or-nothing");

        remove_write_guards(&path);
        // Removing the guards was a commit by another connection.
        doc.resync_for_test();

        doc.save().unwrap();
        assert_eq!(doc.dirty_domains(), (false, false));
    }

    #[test]
    fn a_failed_active_save_leaves_the_dirty_state_and_old_data_intact() {
        let dir = TempDir::new();
        let path = file_with_recovery_history(&dir, "fa.omatree");
        install_write_guards(&path, &["notebook_meta"]); // nodes are rewritten first
        let before = std::fs::read(&path).unwrap();
        let old_titles = {
            let d = Document::open_existing(&path).unwrap();
            titles(d.notebook(), None)
        };

        let mut doc = Document::open_existing(&path).unwrap();
        doc.create_root("will not stick").unwrap();
        assert_eq!(doc.dirty_domains(), (true, false));
        assert!(doc.save().is_err());
        assert_eq!(
            doc.dirty_domains(),
            (true, false),
            "unchanged by the failure"
        );
        drop(doc);
        assert_eq!(std::fs::read(&path).unwrap(), before, "rolled back");
        let d = Document::open_existing(&path).unwrap();
        assert_eq!(titles(d.notebook(), None), old_titles);
    }

    #[test]
    fn a_clean_save_writes_nothing_at_all() {
        let dir = TempDir::new();
        let path = file_with_recovery_history(&dir, "clean.omatree");
        install_write_guards(
            &path,
            &[
                "nodes",
                "notebook_meta",
                "trash_entries",
                "trash_nodes",
                "checkpoints",
                "checkpoint_nodes",
            ],
        );
        let before = std::fs::read(&path).unwrap();

        let mut doc = Document::open_existing(&path).unwrap();
        doc.save().expect("a clean save is a successful no-op");
        doc.save().unwrap();
        assert_eq!(doc.dirty_domains(), (false, false));
        drop(doc);
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn a_clean_version_1_notebook_is_not_migrated_by_saving() {
        let dir = TempDir::new();
        let path = dir.join("v1.omatree");
        write_v1(&path);
        let before = std::fs::read(&path).unwrap();
        let mut doc = Document::open_existing(&path).unwrap();
        doc.save().unwrap();
        drop(doc);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(file_user_version(&path), 1);
    }

    /// A version 1 notebook, written as an older OmaTree would have.
    fn write_v1(path: &Path) {
        let conn = rusqlite::Connection::open(path).unwrap();
        conn.execute_batch(
            "CREATE TABLE nodes (
                 id INTEGER PRIMARY KEY,
                 parent_id INTEGER REFERENCES nodes(id) ON DELETE CASCADE,
                 position INTEGER NOT NULL CHECK (position >= 0),
                 title TEXT NOT NULL,
                 body TEXT NOT NULL);
             CREATE INDEX nodes_parent ON nodes(parent_id, position);
             INSERT INTO nodes VALUES (3, NULL, 0, 'Old', 'old body');
             INSERT INTO nodes VALUES (7, 3, 0, 'Old child', '');
             PRAGMA user_version = 1;",
        )
        .unwrap();
    }

    #[test]
    fn the_first_save_of_a_version_1_notebook_is_the_full_migration() {
        let dir = TempDir::new();
        let path = dir.join("v1.omatree");
        write_v1(&path);
        let mut doc = Document::open_existing(&path).unwrap();
        assert_eq!(file_user_version(&path), 1, "opening does not migrate");

        // An ordinary edit (active state only) still migrates on first save.
        let old = NodeId::from_raw(3);
        doc.set_body(old, "edited").unwrap();
        assert_eq!(doc.dirty_domains(), (true, false));
        doc.save().unwrap();
        assert_eq!(doc.dirty_domains(), (false, false));
        drop(doc);

        assert_eq!(file_user_version(&path), 3);
        for table in RECOVERY_TABLES {
            assert!(table_exists(&path, table), "{table}");
        }
        assert!(table_exists(&path, "notebook_meta"));
        let migrated = Document::open_existing(&path).unwrap();
        assert_eq!(migrated.notebook().get(old).unwrap().body(), "edited");
        assert_eq!(titles(migrated.notebook(), Some(old)), ["Old child"]);
        assert_eq!(migrated.notebook().next_id(), 8);
        assert!(migrated.recovery().trash().is_empty());
    }

    #[test]
    fn saves_after_a_version_1_migration_can_be_active_only() {
        let dir = TempDir::new();
        let path = dir.join("v1.omatree");
        write_v1(&path);
        let mut doc = Document::open_existing(&path).unwrap();
        // Populate recovery state with a structural save, which migrates.
        doc.delete(NodeId::from_raw(7)).unwrap();
        doc.save().unwrap();
        assert_eq!(file_user_version(&path), 3);
        let before = recovery_dumps(&path);
        assert!(before.iter().any(|rows| !rows.is_empty()));
        install_write_guards(&path, &RECOVERY_TABLES);
        doc.resync_for_test();

        doc.set_body(NodeId::from_raw(3), "later edit").unwrap();
        doc.save()
            .expect("active-only now that the file is version 3");
        drop(doc);
        assert_eq!(recovery_dumps(&path), before);
    }

    #[test]
    fn save_as_writes_a_complete_independent_document() {
        let dir = TempDir::new();
        let source = file_with_recovery_history(&dir, "source.omatree");
        let target = dir.join("copy.omatree");
        let mut doc = Document::open_existing(&source).unwrap();
        let (trash, checkpoints) = (
            doc.recovery().trash().to_vec(),
            doc.recovery().checkpoints().to_vec(),
        );
        assert_eq!(doc.dirty_domains(), (false, false), "nothing is dirty");

        doc.save_as(&target, false).unwrap();
        assert_eq!(doc.dirty_domains(), (false, false));
        drop(doc);
        std::fs::remove_file(&source).unwrap(); // the copy must not depend on it

        let copy = Document::open_existing(&target).unwrap();
        assert_eq!(copy.recovery().trash(), &trash[..]);
        assert_eq!(copy.recovery().checkpoints(), &checkpoints[..]);
        assert_eq!(titles(copy.notebook(), None), ["Inbox", "Note"]);
    }

    #[test]
    fn save_as_clears_both_dirty_domains() {
        let dir = TempDir::new();
        let target = dir.join("both.omatree");
        let (mut doc, [projects, ..]) = sample();
        doc.delete(projects).unwrap();
        assert_eq!(doc.dirty_domains(), (true, true));
        doc.save_as(&target, false).unwrap();
        assert_eq!(doc.dirty_domains(), (false, false));

        // A failed Save As clears nothing.
        doc.set_body(doc.notebook().roots()[0].id(), "x").unwrap();
        doc.delete(doc.notebook().roots()[0].id()).unwrap();
        let bad = dir.join("no-such-dir").join("x.omatree");
        assert!(doc.save_as(&bad, false).is_err());
        assert_eq!(doc.dirty_domains(), (true, true));
    }

    #[test]
    fn node_ids_stay_monotonic_across_ordinary_and_full_saves() {
        let dir = TempDir::new();
        let path = dir.join("ids.omatree");
        let mut doc = Document::open(&path).unwrap();
        let a = doc.create_root("a").unwrap();
        let b = doc.create_root("b").unwrap();
        doc.delete(b).unwrap(); // highest id now only in Trash
        doc.save().unwrap(); // full
        let c = doc.create_root("c").unwrap();
        doc.save().unwrap(); // active-only
        assert!(c > b && c > a);
        drop(doc);

        let mut doc = Document::open_existing(&path).unwrap();
        let d = doc.create_root("d").unwrap();
        assert!(d > c, "{d:?} must be above {c:?}");
        doc.save().unwrap();
        drop(doc);
        let doc = Document::open_existing(&path).unwrap();
        assert_eq!(doc.recovery().trash()[0].root().id(), b);
        assert_eq!(titles(doc.notebook(), None), ["a", "c", "d"]);
    }

    // ---- moving nodes ----

    #[test]
    fn a_real_move_creates_exactly_one_checkpoint_naming_the_node() {
        let (mut doc, [projects, inbox, omatree, ..]) = clean_sample();
        assert!(doc.recovery().checkpoints().is_empty());
        assert!(doc.move_node(omatree, Some(inbox), 0).unwrap());
        let checkpoints = doc.recovery().checkpoints();
        assert_eq!(checkpoints.len(), 1);
        assert_eq!(checkpoints[0].reason(), "Before moving \"OmaTree\"");
        assert!(doc.recovery().trash().is_empty(), "moving is not deleting");
        assert_eq!(
            doc.notebook().get(omatree).unwrap().parent_id(),
            Some(inbox)
        );
        assert_eq!(titles(doc.notebook(), Some(projects)), ["Threatwright"]);
    }

    #[test]
    fn no_op_and_failed_moves_change_nothing() {
        let (mut doc, [projects, inbox, omatree, _, ideas]) = clean_sample();
        let before = doc.notebook().snapshot_nodes();
        // no-ops
        assert!(!doc.move_node(omatree, Some(projects), 0).unwrap());
        assert!(!doc.move_node(projects, None, 0).unwrap());
        // failures: cycle, missing node, missing parent
        assert!(doc.move_node(projects, Some(ideas), 0).is_err());
        assert!(doc.move_node(projects, Some(projects), 0).is_err());
        let gone = doc.create_root("gone").unwrap();
        doc.delete(gone).unwrap();
        doc.save_as_dirty_reset_for_test();
        let checkpoints = doc.recovery().checkpoints().len();
        assert!(doc.move_node(gone, None, 0).is_err());
        assert!(doc.move_node(inbox, Some(gone), 0).is_err());

        assert_eq!(doc.dirty_domains(), (false, false));
        assert_eq!(doc.recovery().checkpoints().len(), checkpoints);
        assert_eq!(sorted(doc.notebook().snapshot_nodes()), sorted(before));
    }

    #[test]
    fn a_move_marks_active_and_recovery_state_dirty() {
        let (mut doc, [_, inbox, omatree, ..]) = clean_sample();
        assert_eq!(doc.dirty_domains(), (false, false));
        doc.move_node(omatree, Some(inbox), 0).unwrap();
        assert_eq!(doc.dirty_domains(), (true, true));
        assert!(doc.is_dirty());
    }

    #[test]
    fn restoring_the_checkpoint_undoes_the_move_exactly() {
        let (mut doc, [projects, inbox, omatree, ..]) = clean_sample();
        let before = doc.notebook().snapshot_nodes();
        doc.move_node(projects, Some(inbox), 0).unwrap();
        doc.move_node(omatree, None, 0).unwrap();
        assert_ne!(
            sorted(doc.notebook().snapshot_nodes()),
            sorted(before.clone())
        );
        // The oldest checkpoint is the tree before the first move.
        doc.restore_checkpoint(0).unwrap();
        assert_eq!(sorted(doc.notebook().snapshot_nodes()), sorted(before));
    }

    #[test]
    fn a_moved_structure_survives_save_and_reopen() {
        let dir = TempDir::new();
        let path = dir.join("moved.omatree");
        let (mut doc, [projects, inbox, omatree, threat, _]) = sample();
        doc.save_as(&path, false).unwrap();
        doc.move_node(omatree, Some(inbox), 0).unwrap();
        doc.move_node(threat, None, 0).unwrap();
        doc.move_node(projects, None, 99).unwrap();
        let expected = doc.notebook().snapshot_nodes();
        doc.save().unwrap(); // recovery dirty: the full save
        assert_eq!(doc.dirty_domains(), (false, false));
        drop(doc);

        let reopened = Document::open_existing(&path).unwrap();
        assert_eq!(
            sorted(reopened.notebook().snapshot_nodes()),
            sorted(expected)
        );
        assert_eq!(reopened.recovery().checkpoints().len(), 3);
        assert_eq!(
            titles(reopened.notebook(), None),
            ["Threatwright", "Inbox", "Projects"]
        );
    }

    #[test]
    fn a_moved_structure_survives_save_as() {
        let dir = TempDir::new();
        let (mut doc, [_, inbox, omatree, ..]) = sample();
        doc.move_node(omatree, Some(inbox), 0).unwrap();
        let expected = doc.notebook().snapshot_nodes();
        let target = dir.join("copy.omatree");
        doc.save_as(&target, false).unwrap();
        drop(doc);
        let copy = Document::open_existing(&target).unwrap();
        assert_eq!(sorted(copy.notebook().snapshot_nodes()), sorted(expected));
        assert_eq!(copy.recovery().checkpoints().len(), 1);
    }

    #[test]
    fn trash_restore_still_works_after_nodes_were_moved() {
        let (mut doc, [projects, inbox, omatree, threat, ideas]) = sample();
        doc.move_node(threat, Some(inbox), 0).unwrap();
        doc.delete(omatree).unwrap(); // OmaTree and Ideas go to Trash
        assert!(doc.notebook().get(ideas).is_none());
        // Its parent (Projects) is still there, so it returns beneath it.
        doc.restore_trash(0).unwrap();
        assert_eq!(
            doc.notebook().get(omatree).unwrap().parent_id(),
            Some(projects)
        );
        assert_eq!(
            doc.notebook().get(ideas).unwrap().parent_id(),
            Some(omatree)
        );
        assert_eq!(doc.notebook().get(threat).unwrap().parent_id(), Some(inbox));
    }

    // ---- sibling title uniqueness ----

    /// A saved-looking version 2 file as an older OmaTree could have left it:
    /// roots "Ideas" and "ideas " and, under the first, children "x" and "X".
    fn write_legacy_file(path: &Path) {
        Storage::create(path).unwrap();
        let conn = rusqlite::Connection::open(path).unwrap();
        conn.execute_batch(
            "INSERT INTO nodes VALUES (0, NULL, 0, 'Ideas', 'first body');
             INSERT INTO nodes VALUES (1, NULL, 1, 'ideas ', 'second body');
             INSERT INTO nodes VALUES (2, 0, 0, 'x', '');
             INSERT INTO nodes VALUES (3, 0, 1, 'X', '');",
        )
        .unwrap();
    }

    fn legacy_node(id: u64, parent: Option<u64>, pos: usize, title: &str) -> crate::notebook::Node {
        crate::notebook::Node::from_parts(
            NodeId::from_raw(id),
            parent.map(NodeId::from_raw),
            pos,
            title.to_string(),
            String::new(),
        )
    }

    #[test]
    fn new_notes_get_unique_default_names_independently_per_parent() {
        let mut doc = Document::untitled();
        let names: Vec<String> = (0..3)
            .map(|_| {
                let id = doc.create_default_root().unwrap();
                doc.notebook().get(id).unwrap().title().to_string()
            })
            .collect();
        assert_eq!(names, ["New note", "New note 2", "New note 3"]);
        let parent = doc.notebook().roots()[0].id();
        let first = doc.create_default_child(parent).unwrap();
        let second = doc.create_default_child(parent).unwrap();
        assert_eq!(doc.notebook().get(first).unwrap().title(), "New note");
        assert_eq!(doc.notebook().get(second).unwrap().title(), "New note 2");
        assert_eq!(doc.dirty_domains(), (true, false));
    }

    #[test]
    fn a_refused_rename_changes_nothing_at_all() {
        let (mut doc, [projects, inbox, ..]) = clean_sample();
        assert_eq!(
            doc.rename(inbox, "projects"),
            Err(NotebookError::TitleConflict)
        );
        assert_eq!(doc.rename(inbox, "   "), Err(NotebookError::EmptyTitle));
        assert_eq!(doc.notebook().get(inbox).unwrap().title(), "Inbox");
        assert_eq!(doc.dirty_domains(), (false, false));
        assert!(doc.recovery().checkpoints().is_empty());
        let _ = projects;
        // A case-only rename of the same note is a real change.
        assert!(doc.rename(inbox, "INBOX").unwrap());
        assert_eq!(doc.dirty_domains(), (true, false));
    }

    #[test]
    fn a_title_collision_move_is_refused_before_any_checkpoint_or_dirty_state() {
        let mut doc = Document::untitled();
        let projects = doc.create_root("Projects").unwrap();
        let archive = doc.create_root("Archive").unwrap();
        let ideas = doc.create_child(projects, "Ideas").unwrap();
        doc.create_child(archive, "ideas").unwrap();
        doc.save_as_dirty_reset_for_test();
        let before = doc.notebook().snapshot_nodes();

        assert_eq!(
            doc.move_node(ideas, Some(archive), 0),
            Err(NotebookError::TitleConflict)
        );
        assert_eq!(
            doc.plan_move(ideas, Some(archive), 0),
            Err(NotebookError::TitleConflict)
        );
        assert!(doc.recovery().checkpoints().is_empty(), "no checkpoint");
        assert_eq!(doc.dirty_domains(), (false, false), "no dirty state");
        assert_eq!(doc.notebook().snapshot_nodes(), before, "tree unchanged");
        // Reordering inside the parent is still fine.
        assert!(doc.move_node(ideas, Some(projects), 0).is_ok());
    }

    #[test]
    fn trash_restore_into_a_collision_is_refused_and_changes_nothing() {
        let mut doc = Document::untitled();
        let projects = doc.create_root("Projects").unwrap();
        let ideas = doc.create_child(projects, "Ideas").unwrap();
        doc.delete(ideas).unwrap();
        doc.create_child(projects, "ideas").unwrap(); // took the name meanwhile
        doc.save_as_dirty_reset_for_test();
        let (checkpoints, notebook) = (
            doc.recovery().checkpoints().len(),
            doc.notebook().snapshot_nodes(),
        );

        assert_eq!(doc.plan_trash_restore(0), Err(RecoveryError::TitleConflict));
        assert_eq!(doc.restore_trash(0), Err(RecoveryError::TitleConflict));
        assert_eq!(doc.recovery().trash().len(), 1, "the entry stays in Trash");
        assert_eq!(
            doc.recovery().checkpoints().len(),
            checkpoints,
            "no checkpoint"
        );
        assert_eq!(doc.dirty_domains(), (false, false));
        assert_eq!(doc.notebook().snapshot_nodes(), notebook);
        assert!(!RecoveryError::TitleConflict.message().is_empty());
    }

    #[test]
    fn the_root_fallback_of_a_trash_restore_is_checked_against_the_roots() {
        let mut doc = Document::untitled();
        let parent = doc.create_root("P").unwrap();
        let child = doc.create_child(parent, "Notes").unwrap();
        doc.delete(child).unwrap();
        doc.delete(parent).unwrap(); // the child's old parent is gone too
        doc.create_root("notes").unwrap();
        doc.save_as_dirty_reset_for_test();
        let index = doc
            .recovery()
            .trash()
            .iter()
            .position(|e| e.title() == "Notes")
            .unwrap();
        assert_eq!(
            doc.plan_trash_restore(index),
            Err(RecoveryError::TitleConflict)
        );
        assert_eq!(doc.restore_trash(index), Err(RecoveryError::TitleConflict));
        assert_eq!(doc.recovery().trash().len(), 2);
        assert_eq!(doc.dirty_domains(), (false, false));
    }

    #[test]
    fn a_legacy_trash_subtree_with_internal_duplicates_is_still_recoverable() {
        let mut doc = Document::untitled();
        let entry = TrashEntry::new(
            1,
            vec![
                legacy_node(10, None, 0, "Old"),
                legacy_node(11, Some(10), 0, "dup"),
                legacy_node(12, Some(10), 1, "DUP"),
            ],
        )
        .unwrap();
        doc.recovery = Recovery::from_parts(vec![entry], vec![]);
        assert_eq!(doc.notebook().sibling_title_conflicts(), 0);
        doc.restore_trash(0).unwrap();
        // Restored exactly as stored; the conflict is reported, not repaired.
        assert_eq!(
            titles(doc.notebook(), Some(NodeId::from_raw(10))),
            ["dup", "DUP"]
        );
        assert_eq!(doc.notebook().sibling_title_conflicts(), 1);
    }

    #[test]
    fn a_legacy_checkpoint_restores_exactly_and_its_conflicts_are_detected() {
        let mut doc = Document::untitled();
        doc.create_root("current").unwrap();
        let legacy = vec![
            legacy_node(0, None, 0, "Same"),
            legacy_node(1, None, 1, "same"),
            legacy_node(2, Some(0), 0, "a"),
            legacy_node(3, Some(0), 1, " a "),
        ];
        let checkpoint =
            Checkpoint::from_stored(5, "From an old version".into(), legacy.clone()).unwrap();
        doc.recovery = Recovery::from_parts(vec![], vec![checkpoint]);
        assert_eq!(doc.notebook().sibling_title_conflicts(), 0);

        doc.restore_checkpoint(0)
            .expect("recovery must not be blocked by the new rule");
        assert_eq!(sorted(doc.notebook().snapshot_nodes()), sorted(legacy));
        assert_eq!(doc.notebook().sibling_title_conflicts(), 2);
        assert_eq!(doc.dirty_domains(), (true, true));
        // The state it replaced was checkpointed as usual.
        assert_eq!(doc.recovery().checkpoints().len(), 2);
    }

    #[test]
    fn a_legacy_file_opens_without_being_touched_and_its_conflicts_are_counted() {
        let dir = TempDir::new();
        let path = dir.join("legacy.omatree");
        write_legacy_file(&path);
        let before = std::fs::read(&path).unwrap();

        let doc = Document::open_existing(&path).unwrap();
        assert_eq!(doc.notebook().sibling_title_conflicts(), 2);
        assert_eq!(titles(doc.notebook(), None), ["Ideas", "ideas "]);
        assert!(!doc.is_dirty());
        drop(doc);
        assert_eq!(
            std::fs::read(&path).unwrap(),
            before,
            "opening writes nothing"
        );

        let clean = file_with_recovery_history(&dir, "clean.omatree");
        let doc = Document::open_existing(&clean).unwrap();
        assert_eq!(doc.notebook().sibling_title_conflicts(), 0);
    }

    #[test]
    fn saving_and_reopening_legacy_data_never_renames_it() {
        let dir = TempDir::new();
        let path = dir.join("legacy.omatree");
        write_legacy_file(&path);
        let mut doc = Document::open_existing(&path).unwrap();
        doc.set_body(NodeId::from_raw(0), "edited").unwrap();
        doc.save().unwrap();
        drop(doc);

        let doc = Document::open_existing(&path).unwrap();
        assert_eq!(titles(doc.notebook(), None), ["Ideas", "ideas "]);
        assert_eq!(
            titles(doc.notebook(), Some(NodeId::from_raw(0))),
            ["x", "X"]
        );
        assert_eq!(
            doc.notebook().get(NodeId::from_raw(0)).unwrap().body(),
            "edited"
        );
        assert_eq!(doc.notebook().sibling_title_conflicts(), 2);
    }

    #[test]
    fn legacy_conflicts_can_be_repaired_by_rename_delete_and_move() {
        let dir = TempDir::new();
        let path = dir.join("legacy.omatree");
        write_legacy_file(&path);
        let mut doc = Document::open_existing(&path).unwrap();
        // rename one root, move one child to the (childless) other root, then
        // nothing is left in conflict.
        doc.rename(NodeId::from_raw(1), "Renamed").unwrap();
        assert_eq!(doc.notebook().sibling_title_conflicts(), 1);
        assert!(doc
            .move_node(NodeId::from_raw(3), Some(NodeId::from_raw(1)), 0)
            .unwrap());
        assert_eq!(doc.notebook().sibling_title_conflicts(), 0);

        // Deleting is the third way out.
        let mut other = Document::open_existing(&path).unwrap();
        other.delete(NodeId::from_raw(3)).unwrap();
        other.rename(NodeId::from_raw(1), "Other").unwrap();
        assert_eq!(other.notebook().sibling_title_conflicts(), 0);
    }

    #[test]
    fn new_duplicates_stay_blocked_after_loading_a_legacy_file() {
        let dir = TempDir::new();
        let path = dir.join("legacy.omatree");
        write_legacy_file(&path);
        let mut doc = Document::open_existing(&path).unwrap();
        assert_eq!(doc.create_root("IDEAS"), Err(NotebookError::TitleConflict));
        assert_eq!(
            doc.create_child(NodeId::from_raw(0), " x "),
            Err(NotebookError::TitleConflict)
        );
        assert_eq!(
            doc.rename(NodeId::from_raw(1), "ideas"),
            Err(NotebookError::TitleConflict)
        );
        assert_eq!(doc.dirty_domains(), (false, false));
        let id = doc.create_default_root().unwrap();
        assert_eq!(doc.notebook().get(id).unwrap().title(), "New note");
    }

    // ---- a fresh document (what New Notebook swaps in) ----

    #[test]
    fn a_fresh_document_is_empty_untitled_clean_and_has_no_recovery_history() {
        let doc = Document::untitled();
        assert!(doc.notebook().roots().is_empty());
        assert!(!doc.has_path());
        assert_eq!(doc.display_name(), "Untitled");
        assert!(!doc.is_dirty());
        assert_eq!(doc.dirty_domains(), (false, false));
        assert!(doc.recovery().trash().is_empty());
        assert!(doc.recovery().checkpoints().is_empty());
        assert_eq!(doc.notebook().sibling_title_conflicts(), 0);
    }

    #[test]
    fn a_fresh_document_allocates_node_ids_from_the_start() {
        // Nothing carries over from a previous document, including ids.
        let (mut old, _) = sample();
        old.create_root("one more").unwrap();
        assert!(old.notebook().next_id() > 0);
        let mut fresh = Document::untitled();
        assert_eq!(fresh.notebook().next_id(), 0);
        let first = fresh.create_root("First").unwrap();
        assert_eq!(first, NodeId::from_raw(0));
    }

    #[test]
    fn replacing_a_populated_document_with_a_fresh_one_leaves_its_file_alone() {
        let dir = TempDir::new();
        let path = file_with_recovery_history(&dir, "old.omatree");
        let mut doc = Document::open_existing(&path).unwrap();
        assert!(!doc.recovery().trash().is_empty());
        // Unsaved edits that Discard would throw away.
        doc.create_root("never saved").unwrap();
        assert!(doc.is_dirty());
        let before = std::fs::read(&path).unwrap();

        doc = Document::untitled();
        assert!(doc.notebook().roots().is_empty());
        assert!(doc.recovery().trash().is_empty() && doc.recovery().checkpoints().is_empty());
        assert!(!doc.has_path() && !doc.is_dirty());
        drop(doc);

        assert_eq!(
            std::fs::read(&path).unwrap(),
            before,
            "the old file is untouched"
        );
        let reopened = Document::open_existing(&path).unwrap();
        assert!(
            !reopened.recovery().trash().is_empty(),
            "its own history is intact"
        );
        assert!(titles(reopened.notebook(), None)
            .iter()
            .all(|t| t != "never saved"));
    }

    #[test]
    fn the_first_note_in_a_fresh_document_makes_it_dirty_but_it_cannot_be_saved_in_place() {
        let mut doc = Document::untitled();
        doc.create_root("First note").unwrap();
        assert_eq!(doc.dirty_domains(), (true, false));
        assert!(doc.is_dirty());
        assert!(
            matches!(doc.save(), Err(DocumentError::NoPath)),
            "Ctrl+S means Save As here"
        );
        assert!(!doc.has_path());
    }

    #[test]
    fn a_fresh_document_can_be_saved_as_and_then_saves_in_place() {
        let dir = TempDir::new();
        let target = dir.join("new.omatree");
        let mut doc = Document::untitled();
        doc.create_root("Hello").unwrap();
        doc.save_as(&target, false).unwrap();
        assert!(!doc.is_dirty() && doc.has_path());
        let id = doc.notebook().roots()[0].id();
        doc.set_body(id, "later edit").unwrap();
        doc.save().unwrap();
        drop(doc);
        let reopened = Document::open_existing(&target).unwrap();
        assert_eq!(reopened.notebook().get(id).unwrap().body(), "later edit");
        assert!(
            reopened.recovery().checkpoints().is_empty(),
            "a new notebook starts with none"
        );
    }

    // ---- expanded nodes: view state, schema version 3 ----

    /// A saved, clean notebook at `name` with the `sample()` tree.
    fn saved_sample(dir: &TempDir, name: &str) -> (PathBuf, [NodeId; 5]) {
        let (mut doc, ids) = sample();
        let path = dir.join(name);
        doc.save_as(&path, false).unwrap();
        (path, ids)
    }

    fn expanded_rows(path: &Path) -> Vec<String> {
        let conn = rusqlite::Connection::open(path).unwrap();
        let mut stmt = conn
            .prepare("SELECT node_id FROM expanded_nodes ORDER BY node_id")
            .unwrap();
        let rows = stmt.query_map([], |r| r.get::<_, i64>(0)).unwrap();
        rows.map(|r| r.unwrap().to_string()).collect()
    }

    /// A version 2 notebook as the previous release wrote it.
    fn write_v2_from(path: &Path, doc: &Document) {
        Storage::create(path)
            .unwrap()
            .save_document_with_view(doc.notebook(), doc.recovery(), &BTreeSet::new())
            .unwrap();
        let conn = rusqlite::Connection::open(path).unwrap();
        conn.execute_batch("DROP TABLE expanded_nodes; PRAGMA user_version = 2;")
            .unwrap();
    }

    fn file_bytes(path: &Path) -> Vec<u8> {
        std::fs::read(path).unwrap()
    }

    #[test]
    fn expansion_changes_only_the_named_note() {
        let (mut doc, [projects, inbox, omatree, threat, ideas]) = sample();
        assert!(doc.set_expanded(projects, true));
        assert!(doc.is_expanded(projects));
        for other in [inbox, omatree, threat, ideas] {
            assert!(!doc.is_expanded(other));
        }
        assert!(!doc.set_expanded(projects, true), "no change, no news");
        assert!(doc.set_expanded(projects, false));
        assert!(!doc.is_expanded(projects));
    }

    #[test]
    fn recursive_expansion_covers_every_branch_below_and_nothing_else() {
        let (mut doc, [projects, inbox, omatree, threat, ideas]) = sample();
        doc.set_subtree_expanded(projects, true);
        assert!(doc.is_expanded(projects) && doc.is_expanded(omatree));
        assert!(!doc.is_expanded(threat), "a leaf has nothing to open");
        assert!(!doc.is_expanded(ideas));
        assert!(!doc.is_expanded(inbox), "outside the subtree");
        doc.set_subtree_expanded(projects, false);
        assert!(!doc.is_expanded(projects) && !doc.is_expanded(omatree));
    }

    #[test]
    fn recursive_expansion_of_a_leaf_is_harmless() {
        let (mut doc, [_, inbox, ..]) = sample();
        doc.set_subtree_expanded(inbox, true);
        doc.set_subtree_expanded(inbox, false);
        assert!(!doc.view_dirty());
    }

    #[test]
    fn expand_all_and_collapse_all() {
        let (mut doc, [projects, inbox, omatree, threat, _]) = sample();
        doc.set_all_expanded(true);
        assert!(doc.is_expanded(projects) && doc.is_expanded(omatree));
        assert!(!doc.is_expanded(inbox) && !doc.is_expanded(threat));
        doc.set_all_expanded(false);
        assert!(!doc.is_expanded(projects) && !doc.is_expanded(omatree));
    }

    #[test]
    fn revealing_a_note_opens_its_ancestors() {
        let (mut doc, [projects, _, omatree, threat, ideas]) = sample();
        doc.expand_ancestors(ideas);
        assert!(doc.is_expanded(omatree) && doc.is_expanded(projects));
        assert!(!doc.is_expanded(ideas) && !doc.is_expanded(threat));
    }

    #[test]
    fn unknown_notes_cannot_be_recorded_as_expanded() {
        let (mut doc, _) = sample();
        assert!(!doc.set_expanded(NodeId::from_raw(999), true));
        assert!(!doc.view_dirty());
    }

    #[test]
    fn expansion_is_not_content() {
        let (mut doc, [projects, _, omatree, ..]) = clean_sample();
        let checkpoints = doc.recovery().checkpoints().len();
        doc.set_expanded(projects, true);
        doc.set_subtree_expanded(omatree, true);
        doc.set_all_expanded(true);
        doc.set_all_expanded(false);
        assert!(!doc.is_dirty(), "no star");
        assert_eq!(doc.dirty_domains(), (false, false));
        assert_eq!(doc.recovery().checkpoints().len(), checkpoints);
        assert!(doc.recovery().trash().is_empty());
    }

    #[test]
    fn expanded_notes_are_saved_and_collapsed_ones_are_not() {
        let dir = TempDir::new();
        let (path, [projects, _, omatree, ..]) = saved_sample(&dir, "a.omatree");
        let mut doc = Document::open_existing(&path).unwrap();
        doc.set_expanded(projects, true);
        doc.set_expanded(omatree, true);
        doc.save().unwrap();
        assert_eq!(
            expanded_rows(&path),
            [projects.get().to_string(), omatree.get().to_string()]
        );
        doc.set_expanded(omatree, false);
        doc.save().unwrap();
        assert_eq!(expanded_rows(&path), [projects.get().to_string()]);
    }

    #[test]
    fn reopening_restores_the_saved_expansion() {
        let dir = TempDir::new();
        let (path, [projects, inbox, omatree, ..]) = saved_sample(&dir, "a.omatree");
        let mut doc = Document::open_existing(&path).unwrap();
        doc.set_expanded(projects, true);
        doc.set_expanded(omatree, true);
        doc.save().unwrap();
        drop(doc);
        let reopened = Document::open_existing(&path).unwrap();
        assert!(reopened.is_expanded(projects) && reopened.is_expanded(omatree));
        assert!(!reopened.is_expanded(inbox));
        assert!(!reopened.view_dirty(), "just loaded");
    }

    #[test]
    fn a_plain_save_with_only_expansion_changed_writes_it() {
        let dir = TempDir::new();
        let (path, [projects, ..]) = saved_sample(&dir, "a.omatree");
        let mut doc = Document::open_existing(&path).unwrap();
        assert!(!doc.is_dirty());
        doc.set_expanded(projects, true);
        assert!(!doc.is_dirty());
        doc.save().unwrap();
        assert_eq!(expanded_rows(&path), [projects.get().to_string()]);
        assert!(!doc.view_dirty());
    }

    #[test]
    fn a_clean_save_with_no_view_change_still_writes_nothing() {
        let dir = TempDir::new();
        let (path, _) = saved_sample(&dir, "a.omatree");
        let mut doc = Document::open_existing(&path).unwrap();
        install_write_guards(&path, &["nodes", "expanded_nodes", "notebook_meta"]);
        doc.save().expect("nothing to write, so nothing is touched");
    }

    #[test]
    fn a_content_save_carries_pending_expansion_along() {
        let dir = TempDir::new();
        let (path, [projects, _, omatree, ..]) = saved_sample(&dir, "a.omatree");
        let mut doc = Document::open_existing(&path).unwrap();
        doc.set_expanded(projects, true);
        doc.set_body(omatree, "changed").unwrap();
        install_write_guards(&path, &RECOVERY_TABLES);
        doc.resync_for_test();
        doc.save().unwrap();
        remove_write_guards(&path);
        assert_eq!(expanded_rows(&path), [projects.get().to_string()]);
        assert!(!doc.view_dirty() && !doc.is_dirty());
    }

    #[test]
    fn a_failed_save_keeps_the_view_state_pending() {
        let dir = TempDir::new();
        let (path, [projects, ..]) = saved_sample(&dir, "a.omatree");
        let mut doc = Document::open_existing(&path).unwrap();
        doc.set_expanded(projects, true);
        install_write_guards(&path, &["expanded_nodes"]);
        doc.resync_for_test();
        assert!(matches!(
            doc.save(),
            Err(DocumentError::Storage(StorageError::Sqlite(_)))
        ));
        assert!(doc.view_dirty());
        remove_write_guards(&path);
        doc.resync_for_test();
        assert!(expanded_rows(&path).is_empty(), "the old state is intact");
        doc.save().unwrap();
        assert_eq!(expanded_rows(&path), [projects.get().to_string()]);
    }

    #[test]
    fn save_as_carries_the_expansion() {
        let dir = TempDir::new();
        let (mut doc, [projects, _, omatree, ..]) = sample();
        doc.set_expanded(projects, true);
        doc.set_expanded(omatree, true);
        let copy = dir.join("copy.omatree");
        doc.save_as(&copy, false).unwrap();
        assert!(!doc.view_dirty());
        let reopened = Document::open_existing(&copy).unwrap();
        assert!(reopened.is_expanded(projects) && reopened.is_expanded(omatree));
    }

    #[test]
    fn a_failed_save_as_leaves_the_view_state_pending() {
        let dir = TempDir::new();
        let (mut doc, [projects, ..]) = sample();
        doc.set_expanded(projects, true);
        let bad = dir.join("missing-dir").join("x.omatree");
        assert!(doc.save_as(&bad, false).is_err());
        assert!(doc.view_dirty());
        assert!(doc.is_expanded(projects));
    }

    #[test]
    fn each_document_has_its_own_expansion() {
        let dir = TempDir::new();
        let (a, [projects, ..]) = saved_sample(&dir, "a.omatree");
        let (b, _) = saved_sample(&dir, "b.omatree");
        let mut first = Document::open_existing(&a).unwrap();
        first.set_expanded(projects, true);
        first.save().unwrap();
        let second = Document::open_existing(&b).unwrap();
        assert!(!second.is_expanded(projects), "nothing leaks across files");
        assert!(Document::untitled().expanded.is_empty());
    }

    #[test]
    fn a_version_2_notebook_opens_collapsed_and_is_not_written() {
        let dir = TempDir::new();
        let (doc, [projects, ..]) = sample();
        let path = dir.join("v2.omatree");
        write_v2_from(&path, &doc);
        let before = file_bytes(&path);
        let opened = Document::open_existing(&path).unwrap();
        assert!(!opened.is_expanded(projects));
        assert!(opened.expanded.is_empty());
        drop(opened);
        assert_eq!(file_bytes(&path), before, "opening never rewrites a file");
        assert_eq!(file_user_version(&path), 2);
    }

    #[test]
    fn the_first_save_migrates_version_2_to_3() {
        let dir = TempDir::new();
        let (doc, [projects, ..]) = sample();
        let path = dir.join("v2.omatree");
        write_v2_from(&path, &doc);
        let mut opened = Document::open_existing(&path).unwrap();
        opened.save().unwrap();
        assert_eq!(
            file_user_version(&path),
            2,
            "nothing changed, nothing saved"
        );
        opened.set_expanded(projects, true);
        opened.save().unwrap();
        assert_eq!(file_user_version(&path), 3);
        assert_eq!(expanded_rows(&path), [projects.get().to_string()]);
        let again = Document::open_existing(&path).unwrap();
        assert_eq!(again.notebook().roots().len(), 2);
        assert!(again.is_expanded(projects));
    }

    #[test]
    fn a_content_save_also_migrates_version_2() {
        let dir = TempDir::new();
        let (doc, [_, inbox, ..]) = sample();
        let path = dir.join("v2.omatree");
        write_v2_from(&path, &doc);
        let mut opened = Document::open_existing(&path).unwrap();
        opened.set_body(inbox, "x").unwrap();
        opened.save().unwrap();
        assert_eq!(file_user_version(&path), 3);
    }

    #[test]
    fn a_failed_migration_leaves_the_version_2_file_untouched() {
        let dir = TempDir::new();
        let (doc, [projects, ..]) = sample();
        let path = dir.join("v2.omatree");
        write_v2_from(&path, &doc);
        let before = file_bytes(&path);
        let mut opened = Document::open_existing(&path).unwrap();
        opened.set_expanded(projects, true);
        install_write_guards(&path, &["notebook_meta"]);
        let guarded = file_bytes(&path);
        assert!(opened.save().is_err());
        assert_eq!(file_user_version(&path), 2);
        assert!(!table_exists(&path, "expanded_nodes"), "rolled back");
        assert_eq!(file_bytes(&path), guarded);
        remove_write_guards(&path);
        assert_ne!(before.len(), 0);
        assert!(opened.view_dirty());
    }

    #[test]
    fn stored_ids_of_missing_notes_are_ignored() {
        let dir = TempDir::new();
        let (path, [projects, ..]) = saved_sample(&dir, "a.omatree");
        // A foreign-key-free connection can leave a stray row behind.
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.pragma_update(None, "foreign_keys", false).unwrap();
        conn.execute("INSERT INTO expanded_nodes VALUES (999)", [])
            .unwrap();
        conn.execute(
            "INSERT INTO expanded_nodes VALUES (?1)",
            [projects.get() as i64],
        )
        .unwrap();
        drop(conn);
        let doc = Document::open_existing(&path).unwrap();
        assert!(doc.is_expanded(projects));
        assert!(!doc.is_expanded(NodeId::from_raw(999)));
        assert_eq!(doc.expanded.len(), 1);
    }

    #[test]
    fn stale_ids_are_dropped_when_saving() {
        let dir = TempDir::new();
        let (path, [projects, _, omatree, ..]) = saved_sample(&dir, "a.omatree");
        let mut doc = Document::open_existing(&path).unwrap();
        doc.set_expanded(projects, true);
        doc.set_expanded(omatree, true);
        doc.delete(omatree).unwrap();
        assert!(
            doc.is_expanded(omatree),
            "kept for now, in case of a restore"
        );
        doc.save().unwrap();
        assert_eq!(expanded_rows(&path), [projects.get().to_string()]);
        assert!(!doc.is_expanded(omatree), "reconciled by the save");
    }

    #[test]
    fn a_moved_note_keeps_its_expansion() {
        let (mut doc, [projects, inbox, omatree, ..]) = sample();
        doc.set_expanded(omatree, true);
        doc.move_node(omatree, Some(inbox), 0).unwrap();
        assert_eq!(
            doc.notebook().get(omatree).unwrap().parent_id(),
            Some(inbox)
        );
        assert!(doc.is_expanded(omatree));
        let dir = TempDir::new();
        let path = dir.join("m.omatree");
        doc.save_as(&path, false).unwrap();
        assert_eq!(expanded_rows(&path), [omatree.get().to_string()]);
        assert!(!doc.is_expanded(projects));
    }

    #[test]
    fn a_restored_note_regains_its_expansion_until_the_next_save() {
        let (mut doc, [_, _, omatree, _, ideas]) = sample();
        doc.set_expanded(omatree, true);
        doc.delete(omatree).unwrap();
        doc.restore_trash(0).unwrap();
        assert!(doc.notebook().get(ideas).is_some());
        assert!(doc.is_expanded(omatree));
    }

    #[test]
    fn a_note_deleted_and_saved_is_forgotten() {
        let dir = TempDir::new();
        let (path, [_, _, omatree, ..]) = saved_sample(&dir, "a.omatree");
        let mut doc = Document::open_existing(&path).unwrap();
        doc.set_expanded(omatree, true);
        doc.delete(omatree).unwrap();
        doc.save().unwrap();
        doc.restore_trash(0).unwrap();
        assert!(!doc.is_expanded(omatree));
    }

    #[test]
    fn restoring_a_checkpoint_keeps_expansion_only_for_notes_that_remain() {
        let (mut doc, [projects, _, omatree, ..]) = sample();
        doc.create_checkpoint("before");
        let extra = doc.create_root("Extra").unwrap();
        doc.create_child(extra, "Kid").unwrap();
        doc.set_expanded(extra, true);
        doc.set_expanded(projects, true);
        doc.restore_checkpoint(0).unwrap();
        assert!(doc.is_expanded(projects));
        assert!(doc.notebook().get(extra).is_none());
        assert_eq!(doc.valid_expanded().len(), 1);
        assert!(!doc.is_expanded(omatree));
    }

    // ---- the checkpoint byte budget, through files ----

    const MIB: usize = 1024 * 1024;

    fn checkpoint_rows(path: &Path) -> i64 {
        let conn = rusqlite::Connection::open(path).unwrap();
        conn.query_row("SELECT count(*) FROM checkpoints", [], |r| r.get(0))
            .unwrap()
    }

    /// A saved notebook with five 3 MiB checkpoints (15 MiB: within the
    /// budget), then every checkpoint row copied once more, as an older
    /// version with a larger or no budget could have left it: 30 MiB.
    fn file_with_excess_history(dir: &TempDir) -> PathBuf {
        let path = dir.join("legacy.omatree");
        let mut doc = Document::open(&path).unwrap();
        let big = doc.create_root("Big").unwrap();
        doc.set_body(big, &"x".repeat(3 * MIB)).unwrap();
        doc.create_root("Other").unwrap();
        for i in 0..5 {
            doc.create_checkpoint(&format!("c{i}"));
        }
        doc.save().unwrap();
        drop(doc);
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "INSERT INTO checkpoints (id, created_at, reason)
                 SELECT id + 100, created_at, reason || 'b' FROM checkpoints;
             INSERT INTO checkpoint_nodes
                 SELECT checkpoint_id + 100, node_id, parent_id, position, title, body
                 FROM checkpoint_nodes;",
        )
        .unwrap();
        assert_eq!(checkpoint_rows(&path), 10);
        path
    }

    #[test]
    fn a_file_with_excess_history_opens_pruned_in_memory_and_is_not_written() {
        let dir = TempDir::new();
        let path = file_with_excess_history(&dir);
        let before = file_bytes(&path);
        let doc = Document::open_existing(&path).unwrap();
        assert_eq!(
            doc.recovery().checkpoints().len(),
            5,
            "pruned to the budget"
        );
        assert!(doc.recovery().checkpoint_bytes() <= crate::recovery::MAX_CHECKPOINT_BYTES);
        assert!(!doc.is_dirty(), "opening is not a change");
        drop(doc);
        assert_eq!(file_bytes(&path), before, "opening wrote nothing");
        assert_eq!(checkpoint_rows(&path), 10);
    }

    #[test]
    fn the_next_full_save_persists_the_pruned_history_and_trash_is_untouched() {
        let dir = TempDir::new();
        let path = file_with_excess_history(&dir);
        let mut doc = Document::open_existing(&path).unwrap();
        let other = doc.notebook().roots()[1].id();
        doc.delete(other).unwrap(); // a trash entry, and one more checkpoint
        doc.save().unwrap();
        drop(doc);
        let reopened = Document::open_existing(&path).unwrap();
        assert!(
            checkpoint_rows(&path) <= 5,
            "the file now holds the pruned set"
        );
        assert_eq!(reopened.recovery().trash().len(), 1);
        assert_eq!(reopened.recovery().trash()[0].title(), "Other");
    }

    #[test]
    fn restoring_a_checkpoint_still_works_after_pruning() {
        let dir = TempDir::new();
        let path = file_with_excess_history(&dir);
        let mut doc = Document::open_existing(&path).unwrap();
        let newest = doc.recovery().checkpoints().len() - 1;
        doc.restore_checkpoint(newest).unwrap();
        assert_eq!(doc.notebook().roots().len(), 2);
        assert!(doc.recovery().checkpoint_bytes() <= crate::recovery::MAX_CHECKPOINT_BYTES.max(1));
    }

    // ---- two instances of one notebook ----

    fn two_instances(dir: &TempDir) -> (PathBuf, Document, Document, [NodeId; 5]) {
        let (path, ids) = saved_sample(dir, "shared.omatree");
        let a = Document::open_existing(&path).unwrap();
        let b = Document::open_existing(&path).unwrap();
        (path, a, b, ids)
    }

    fn body_on_disk(path: &Path, id: NodeId) -> String {
        Document::open_existing(path)
            .unwrap()
            .notebook()
            .get(id)
            .unwrap()
            .body()
            .to_string()
    }

    #[test]
    fn a_stale_instance_cannot_overwrite_a_newer_save() {
        let dir = TempDir::new();
        let (path, mut a, mut b, [projects, inbox, ..]) = two_instances(&dir);
        a.set_body(projects, "from A").unwrap();
        a.save().unwrap();

        b.set_body(inbox, "from B").unwrap();
        let err = b.save().unwrap_err();
        assert!(err.is_conflict(), "{err}");
        assert!(err.save_message().contains("changed on disk"));
        assert!(err.save_as_message().contains("Save As"));

        // A's saved work is intact, and B's change did not reach the file.
        assert_eq!(body_on_disk(&path, projects), "from A");
        assert_eq!(body_on_disk(&path, inbox), "");
        // B's own state is intact and still unsaved.
        assert_eq!(b.notebook().get(inbox).unwrap().body(), "from B");
        assert!(b.is_dirty());
        // The refusal repeats until it is resolved.
        assert!(b.save().unwrap_err().is_conflict());
    }

    #[test]
    fn the_stale_instance_can_save_as_and_reopening_clears_the_conflict() {
        let dir = TempDir::new();
        let (path, mut a, mut b, [projects, inbox, ..]) = two_instances(&dir);
        a.set_body(projects, "from A").unwrap();
        a.save().unwrap();
        b.set_body(inbox, "from B").unwrap();
        assert!(b.save().is_err());

        let copy = dir.join("mine.omatree");
        b.save_as(&copy, false).unwrap();
        assert!(!b.is_dirty());
        assert_eq!(body_on_disk(&copy, inbox), "from B");
        assert_eq!(
            body_on_disk(&path, projects),
            "from A",
            "the original was not touched"
        );

        // Reopening the original gives a normal, working instance.
        let mut again = Document::open_existing(&path).unwrap();
        assert_eq!(again.notebook().get(projects).unwrap().body(), "from A");
        again.set_body(inbox, "later").unwrap();
        again.save().unwrap();
        assert_eq!(body_on_disk(&path, inbox), "later");
    }

    #[test]
    fn an_expansion_only_save_cannot_overwrite_external_content() {
        let dir = TempDir::new();
        let (path, mut a, mut b, [projects, _, omatree, ..]) = two_instances(&dir);
        a.set_body(omatree, "from A").unwrap();
        a.save().unwrap();
        b.set_expanded(projects, true);
        assert!(b.save().unwrap_err().is_conflict());
        assert_eq!(body_on_disk(&path, omatree), "from A");
        assert!(expanded_rows(&path).is_empty());
        assert!(b.view_dirty(), "the view state is still pending");
    }

    #[test]
    fn a_structural_save_cannot_overwrite_external_content() {
        let dir = TempDir::new();
        let (path, mut a, mut b, [projects, inbox, ..]) = two_instances(&dir);
        a.set_body(projects, "from A").unwrap();
        a.save().unwrap();
        b.delete(inbox).unwrap();
        assert_eq!(b.dirty_domains(), (true, true));
        assert!(b.save().unwrap_err().is_conflict());
        let disk = Document::open_existing(&path).unwrap();
        assert!(
            disk.notebook().get(inbox).is_some(),
            "the delete did not reach the file"
        );
        assert!(disk.recovery().trash().is_empty());
        assert_eq!(b.dirty_domains(), (true, true), "both flags survive");
        assert_eq!(
            b.recovery().trash().len(),
            1,
            "B's Trash entry is still in memory"
        );
    }

    #[test]
    fn instances_that_only_look_cause_no_conflict() {
        let dir = TempDir::new();
        let (path, mut a, mut b, [projects, ..]) = two_instances(&dir);
        // Neither changed anything: nothing is written, nothing conflicts.
        a.save().unwrap();
        b.save().unwrap();
        let mut c = Document::open_existing(&path).unwrap();
        c.save().unwrap();
        // Expanding and collapsing without saving is not a write either.
        a.set_expanded(projects, true);
        b.set_expanded(projects, false);
        // A third instance opened after A's later save sees A's work.
        a.save().unwrap();
        let d = Document::open_existing(&path).unwrap();
        assert!(d.is_expanded(projects));
    }

    #[test]
    fn an_instances_own_saves_never_look_like_someone_elses() {
        let dir = TempDir::new();
        let (_, mut a, _, [projects, inbox, ..]) = two_instances(&dir);
        for round in 0..5 {
            a.set_body(projects, &format!("p{round}")).unwrap();
            a.save().unwrap(); // active-only
            a.delete(inbox).unwrap_or(());
            a.set_expanded(projects, round % 2 == 0);
            a.save().unwrap(); // structural / view
            let _ = a.restore_trash(0);
            a.save().unwrap();
        }
    }

    #[test]
    fn a_save_as_over_an_existing_notebook_is_not_a_conflict_with_the_old_file() {
        let dir = TempDir::new();
        let (_, mut a, mut b, [projects, ..]) = two_instances(&dir);
        a.set_body(projects, "from A").unwrap();
        a.save().unwrap();
        // B is stale for its own file, but Save As elsewhere is unaffected.
        b.set_body(projects, "from B").unwrap();
        let (other, _) = saved_sample(&dir, "other.omatree");
        b.save_as(&other, true).unwrap();
        assert_eq!(body_on_disk(&other, projects), "from B");
        // And B now owns `other`, so it saves there normally.
        b.set_body(projects, "again").unwrap();
        b.save().unwrap();
    }

    /// The pre-release audit probe, kept as a manual benchmark (never part of a
    /// normal run, and no timing is asserted): 1000 notes of 2 KB, then 100
    /// structural changes, each saved. Run with
    /// `cargo test --release checkpoint_benchmark -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn checkpoint_benchmark() {
        let dir = TempDir::new();
        let path = dir.join("bench.omatree");
        let mut doc = Document::open(&path).unwrap();
        let body = "x".repeat(2000);
        let mut ids = Vec::new();
        for i in 0..1000 {
            let id = doc.create_root(&format!("note {i}")).unwrap();
            doc.set_body(id, &body).unwrap();
            ids.push(id);
        }
        doc.save().unwrap();
        let start = std::fs::metadata(&path).unwrap().len();
        let began = std::time::Instant::now();
        let mut last = std::time::Duration::ZERO;
        for id in ids.iter().take(100) {
            doc.move_node(*id, None, 999).unwrap();
            let one = std::time::Instant::now();
            doc.save().unwrap();
            last = one.elapsed();
        }
        let end = std::fs::metadata(&path).unwrap().len();
        println!(
            "BENCH initial {} KB, after 100 moves {} KB ({:.1}x), 100 moves+saves {:?}, final save {:?}, checkpoints kept {}, retained snapshot bytes {} KB",
            start / 1024,
            end / 1024,
            end as f64 / start as f64,
            began.elapsed(),
            last,
            doc.recovery().checkpoints().len(),
            doc.recovery().checkpoint_bytes() / 1024
        );
    }
}
