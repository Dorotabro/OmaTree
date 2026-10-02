//! The open notebook: its tree, the file it came from, and whether it has
//! unsaved changes. Pure Rust; the Qt model wraps this.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::notebook::{NodeId, Notebook, NotebookError};
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
    storage: Option<Storage>,
    path: Option<PathBuf>,
    dirty: bool,
}

impl Document {
    /// Empty in-memory notebook with no file.
    pub fn untitled() -> Self {
        Document {
            notebook: Notebook::new(),
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
        let notebook = storage.load()?;
        Ok(Document {
            notebook,
            storage: Some(storage),
            path: Some(path.to_path_buf()),
            dirty: false,
        })
    }

    /// Writes the whole notebook. Dirty is cleared only on success.
    pub fn save(&mut self) -> Result<(), DocumentError> {
        let storage = self.storage.as_mut().ok_or(DocumentError::NoPath)?;
        storage.save(&self.notebook)?;
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
        if let Err(e) = target.save(&self.notebook) {
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

    pub fn delete(&mut self, id: NodeId) -> Result<(), NotebookError> {
        self.notebook.delete(id)?;
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
}
