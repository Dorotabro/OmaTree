//! The attachment backend until SQLite attachments exist (ticket E9): nothing can be stored and
//! nothing can be read. It never touches the filesystem and never fetches anything, so a reference
//! in a note (`https://...`, `/etc/x`, `assets/x.png`) is shown as unavailable and a pasted image
//! is refused before any Markdown reference is written.

use omatree_editor::attachments::{AttachmentError, AttachmentInfo, AttachmentStore};

pub struct UnavailableStore;

impl AttachmentStore for UnavailableStore {
    fn put(&self, _bytes: &[u8], _ext: &str) -> Result<String, AttachmentError> {
        Err(AttachmentError::Io(
            "images inside notebooks are not supported yet".to_owned(),
        ))
    }

    fn get(&self, reference: &str) -> Result<Vec<u8>, AttachmentError> {
        Err(AttachmentError::Invalid(reference.to_owned()))
    }

    fn info(&self, reference: &str) -> Result<AttachmentInfo, AttachmentError> {
        Err(AttachmentError::Invalid(reference.to_owned()))
    }
}
