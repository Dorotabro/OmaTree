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
            DocumentError::NoPath => "No notebook file was given.",
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
        let (storage, notebook) = match std::fs::metadata(path) {
            Ok(_) => {
                let storage = Storage::open(path)?;
                let notebook = storage.load()?;
                (storage, notebook)
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                (Storage::create(path)?, Notebook::new())
            }
            Err(e) => return Err(DocumentError::Io(e)),
        };
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
}
