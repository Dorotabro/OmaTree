//! The open notebook: its tree, the file it came from, and whether it has
//! unsaved changes. Pure Rust; the Qt model wraps this.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::notebook::{NodeId, Notebook, NotebookError};
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

impl DocumentError {
    /// Short human-readable text for a failed open. The raw error is for logs.
    pub fn open_message(&self) -> String {
        match self {
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
    dirty: bool,
}

impl Document {
    /// Empty in-memory notebook with no file.
    pub fn untitled() -> Self {
        Document {
            notebook: Notebook::new(),
            recovery: Recovery::default(),
            storage: None,
            path: None,
            dirty: false,
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
                dirty: false,
            }),
            Err(e) => Err(DocumentError::Io(e)),
        }
    }

    /// Opens an existing notebook. A missing file is an error, never created.
    pub fn open_existing(path: &Path) -> Result<Self, DocumentError> {
        std::fs::metadata(path).map_err(DocumentError::Io)?;
        let storage = Storage::open(path)?;
        let (notebook, recovery) = storage.load_document()?;
        Ok(Document {
            notebook,
            recovery,
            storage: Some(storage),
            path: Some(path.to_path_buf()),
            dirty: false,
        })
    }

    /// Writes the whole notebook. Dirty is cleared only on success.
    pub fn save(&mut self) -> Result<(), DocumentError> {
        let storage = self.storage.as_mut().ok_or(DocumentError::NoPath)?;
        storage.save_document(&self.notebook, &self.recovery)?;
        self.dirty = false;
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
        if let Err(e) = target.save_document(&self.notebook, &self.recovery) {
            drop(target);
            if !existed {
                let _ = std::fs::remove_file(path);
            }
            return Err(e.into());
        }

        self.storage = Some(target);
        self.path = Some(path.to_path_buf());
        self.dirty = false;
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

    pub fn is_dirty(&self) -> bool {
        self.dirty
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
    // actually changed something.

    pub fn create_root(&mut self, title: &str) -> NodeId {
        self.dirty = true;
        self.notebook.create_root(title)
    }

    pub fn create_child(&mut self, parent: NodeId, title: &str) -> Result<NodeId, NotebookError> {
        let id = self.notebook.create_child(parent, title)?;
        self.dirty = true;
        Ok(id)
    }

    pub fn rename(&mut self, id: NodeId, title: &str) -> Result<(), NotebookError> {
        let unchanged = self.notebook.get(id).is_some_and(|n| n.title() == title);
        self.notebook.rename(id, title)?;
        self.dirty |= !unchanged;
        Ok(())
    }

    pub fn set_body(&mut self, id: NodeId, body: &str) -> Result<(), NotebookError> {
        let unchanged = self.notebook.get(id).is_some_and(|n| n.body() == body);
        self.notebook.set_body(id, body)?;
        self.dirty |= !unchanged;
        Ok(())
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
        self.dirty = true;
        Ok(())
    }

    /// Pretends the document was just saved. Tests only.
    #[cfg(test)]
    fn save_as_dirty_reset_for_test(&mut self) {
        self.dirty = false;
    }

    // Recovery.

    pub fn recovery(&self) -> &Recovery {
        &self.recovery
    }

    /// Records the current notebook as a recovery checkpoint (keeping only
    /// the newest few). Call this before any operation that restructures
    /// or replaces the tree. It does not itself make the document dirty.
    pub fn create_checkpoint(&mut self, reason: &str) {
        let checkpoint = Checkpoint::of(&self.notebook, reason, unix_now());
        self.recovery.add_checkpoint(checkpoint);
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
        self.dirty = true;
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
        self.dirty = true;
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
        doc.create_root("a");
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
        let projects = doc.create_root("Projects");
        let inbox = doc.create_root("Inbox");
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
        let fresh = doc.create_root("new");
        assert!(fresh > inbox && fresh > ideas);
    }

    #[test]
    fn dirty_follows_successful_mutations_and_saves() {
        let file = TempFile::new();
        let mut doc = Document::open(file.path()).unwrap();
        assert!(!doc.is_dirty());

        let a = doc.create_root("a");
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
    fn failed_or_noop_mutations_do_not_mark_dirty() {
        let file = TempFile::new();
        let mut doc = Document::open(file.path()).unwrap();
        let a = doc.create_root("a");
        doc.set_body(a, "same").unwrap();
        doc.save().unwrap();

        let missing = {
            let gone = doc.create_root("gone");
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
            doc.create_root("kept");
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
        doc.create_root("unsaved");
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
        let a = doc.create_root("Projects");
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
        doc.create_root("unsaved");

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
        untitled.create_root("x");
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
        doc.create_root("one");
        doc.save().unwrap();
        let before = std::fs::read(&first).unwrap();

        doc.create_root("two");
        doc.save_as(&second, false).unwrap();
        assert!(doc.is_current_path(&second));
        assert_eq!(std::fs::read(&first).unwrap(), before);
        assert_eq!(stored_titles(&first), ["one"]);
        assert_eq!(stored_titles(&second), ["one", "two"]);

        // Later saves go to the new file only.
        doc.create_root("three");
        doc.save().unwrap();
        assert_eq!(stored_titles(&first), ["one"]);
        assert_eq!(stored_titles(&second), ["one", "two", "three"]);
    }

    #[test]
    fn save_as_to_the_current_path_is_a_normal_save() {
        let dir = TempDir::new();
        let path = dir.join("same.omatree");
        let mut doc = Document::open(&path).unwrap();
        doc.create_root("kept");
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
            other.create_root("old content");
            other.save().unwrap();
        }
        let before = std::fs::read(&target).unwrap();

        let mut doc = Document::untitled();
        doc.create_root("new content");
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
            other.create_root("precious");
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
        doc.create_root("replacement");
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
        doc.create_root("mine");
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
        let projects = doc.create_root("Projects");
        let inbox = doc.create_root("Inbox");
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
        let parent = doc.create_root("P");
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
        let parent = doc.create_root("P");
        doc.create_root("other");
        let child = doc.create_child(parent, "child").unwrap();
        doc.delete(child).unwrap();
        doc.delete(parent).unwrap(); // its subtree no longer contains `child`
        doc.create_root("later");
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
        let parent = doc.create_root("P");
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
        doc.create_root("first");
        let middle = doc.create_root("middle");
        doc.create_root("last");
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
        let a = doc.create_root("a");
        let b = doc.create_root("b");
        doc.delete(b).unwrap(); // the highest id now lives only in Trash
        doc.save().unwrap();
        drop(doc);

        let mut doc = Document::open_existing(&path).unwrap();
        let c = doc.create_root("c");
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
        doc.create_root("another");
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
}
