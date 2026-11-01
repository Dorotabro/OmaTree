//! The editor's attachment backend: a thin view of the open notebook's [`Document`] (ticket E10).
//!
//! The editor library asks an [`AttachmentStore`] to `put`, `get` and `info`; this adapter forwards
//! each call to the `Document`'s own attachment API (E9), so there is exactly one authority over
//! the notebook's images, one SQLite connection, one pending store and one set of limits. It owns
//! nothing: a `DocStore` is built for each frame from `&Document`, which is why no unsafe code or
//! duplicated state is needed (the editor takes `&dyn AttachmentStore`, and `Document`'s attachment
//! calls take `&self`). It never reads a file or a URL: every reference goes to
//! `Document::attachment`, which accepts only `assets/<32 hex>.png|jpg` and answers from the
//! notebook.
//!
//! The editor reduces every failure to its own four error kinds and shows a fixed sentence, and it
//! treats any failed `get` as "this image is not available". The adapter therefore also records
//! what really went wrong ([`ImageReport`]) so the host can say it properly and can tell a damaged
//! or unreadable image from a missing one.

use omatree_core::attachments::{AttachmentError as Core, MediaType};
use omatree_core::document::Document;
use omatree_editor::attachments::{AttachmentError, AttachmentInfo, AttachmentStore};
use std::cell::RefCell;

/// What the last frame's image operations really reported, for the host to turn into words.
#[derive(Default)]
pub struct ImageReport {
    /// The error of the last refused `put` (a paste that stored nothing).
    put: RefCell<Option<Core>>,
    /// Reads that failed for a reason other than "no such image" or "not a valid reference":
    /// stored data that is damaged, or a read the database could not do right now.
    read: RefCell<Option<Core>>,
}

impl ImageReport {
    pub fn take_put(&self) -> Option<Core> {
        self.put.borrow_mut().take()
    }

    pub fn take_read(&self) -> Option<Core> {
        self.read.borrow_mut().take()
    }
}

pub struct DocStore<'a> {
    doc: &'a Document,
    report: &'a ImageReport,
}

impl<'a> DocStore<'a> {
    pub fn new(doc: &'a Document, report: &'a ImageReport) -> Self {
        DocStore { doc, report }
    }
}

/// The editor-side error for a core error (the exact wording is the host's: see `friendly`).
fn editor_error(reference: &str, e: &Core) -> AttachmentError {
    match e {
        Core::InvalidReference(_) => AttachmentError::Invalid(reference.to_owned()),
        Core::NotFound(_) => AttachmentError::NotFound(reference.to_owned()),
        Core::TooLarge(n) => AttachmentError::TooLarge(*n),
        // Damage, a refused image and a failing database all carry their own sentence.
        other => AttachmentError::Io(other.to_string()),
    }
}

/// A restrained sentence for the status bar, or `None` when the editor's own is good enough.
pub fn friendly_put_error(e: &Core) -> String {
    match e {
        Core::PendingFull(_) => format!("Image not added: {e}."),
        Core::TooLarge(n) => format!(
            "Image not added: it is {} MiB and the limit is {} MiB.",
            n / (1024 * 1024),
            omatree_core::attachments::MAX_ATTACHMENT_BYTES / (1024 * 1024)
        ),
        Core::UnsupportedFormat | Core::Malformed(_) | Core::Empty => {
            "Image not added: only valid PNG and JPEG images can be pasted.".to_owned()
        }
        Core::Storage(_) => format!("Image paste failed: {e}. The note was not changed."),
        other => format!("Image paste failed: {other}. The note was not changed."),
    }
}

impl AttachmentStore for DocStore<'_> {
    fn put(&self, bytes: &[u8], _extension_hint: &str) -> Result<String, AttachmentError> {
        // The bytes decide the type (the document sniffs them); the hint is not trusted.
        self.doc.put_attachment(bytes).map_err(|e| {
            let mapped = editor_error("", &e);
            *self.report.put.borrow_mut() = Some(e);
            mapped
        })
    }

    fn get(&self, reference: &str) -> Result<Vec<u8>, AttachmentError> {
        self.doc.attachment(reference).map_err(|e| {
            if !matches!(e, Core::NotFound(_) | Core::InvalidReference(_)) {
                *self.report.read.borrow_mut() = Some(e.clone());
            }
            editor_error(reference, &e)
        })
    }

    fn info(&self, reference: &str) -> Result<AttachmentInfo, AttachmentError> {
        self.doc
            .attachment_info(reference)
            .map(|i| AttachmentInfo {
                len: i.len,
                media_type: match i.media_type {
                    MediaType::Png => "image/png",
                    MediaType::Jpeg => "image/jpeg",
                },
            })
            .map_err(|e| editor_error(reference, &e))
    }
}
