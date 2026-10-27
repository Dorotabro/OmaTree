//! Compact Notebook, core half (ticket E9b): which attachments nothing refers to any more, an
//! explicit pruning that deletes exactly those, and a separate space reclamation. No user
//! interface, no confirmation dialog, nothing automatic: every function here is something a
//! caller asks for by name, and nothing else in the crate ever deletes an attachment.
//!
//! # What is scanned
//!
//! Every body that can still be recovered, by the plain-text rule of
//! [`scan_references`](crate::attachments::scan_references):
//!
//! - the in-memory notebook, Trash (every node of every entry) and checkpoints, which is the
//!   truth about what the user has now, saved or not;
//! - the same three kinds of body as stored in the file, which is what a crash, a failed save or
//!   a "close without saving" would leave behind.
//!
//! An attachment is protected when *either* mentions it. So a reference the user has deleted
//! but not saved still protects it (the file would need it), and an unsaved reference protects
//! it too. Pending attachments are not stored yet, so they cannot be pruned; they are reported.
//!
//! # Authorization
//!
//! A preview carries an opaque [`PruneToken`]: a 128-bit digest of everything the decision rests
//! on (the path, every body of the notebook, Trash and checkpoints in memory, the pending
//! attachments, the stored inventory with sizes, the references found in the file). Pruning
//! recomputes it and refuses a different one (`Stale`); it also needs a saved notebook
//! (`NeedsSave`: it never saves by itself), and then deletes in one checked transaction that
//! re-derives the unreferenced set from the file and refuses unless it is exactly the reviewed
//! one. A caller cannot name ids to delete. The digest is change detection, not a signature:
//! the transaction's own recheck is what the safety rests on.

use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::hash::Hasher;
use std::time::Duration;

use super::Document;
use crate::attachments::{scan_references, AttachmentId, MediaType};
use crate::storage::{PhysicalStats, StorageError};

/// How many attachments, and how many BLOB bytes. Categories of a [`PruneReport`] **overlap**
/// (one image used by an active note and a checkpoint is in both): never add them up.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tally {
    pub count: usize,
    pub bytes: u64,
}

impl Tally {
    fn add(&mut self, bytes: u64) {
        self.count += 1;
        self.bytes += bytes;
    }
}

/// One stored attachment that nothing refers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prunable {
    pub id: AttachmentId,
    pub bytes: u64,
    pub media_type: MediaType,
}

/// The read-only answer to "what could be removed?". Counts are of *unique stored attachments*.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PruneReport {
    /// Everything stored in the file.
    pub stored: Tally,
    /// Stored attachments mentioned by an active note / a Trash entry / a retained checkpoint of
    /// the notebook as it is in memory now. These three overlap each other.
    pub in_active: Tally,
    pub in_trash: Tally,
    pub in_checkpoints: Tally,
    /// Stored attachments mentioned only by unsaved in-memory bodies (not by anything in the file).
    pub only_in_unsaved_changes: Tally,
    /// Stored attachments mentioned only by what is in the file and no longer in memory (the
    /// user removed the reference or a checkpoint was evicted, and has not saved).
    pub only_in_saved_file: Tally,
    /// Attachments put since the last save: not stored, so not part of `stored`, never prunable.
    pub pending: Tally,
    /// Distinct ids mentioned somewhere that are neither stored nor pending. Never a reason to
    /// delete anything.
    pub missing_references: usize,
    /// Stored attachments mentioned anywhere recoverable: what a prune keeps.
    pub retained: Tally,
    /// Stored attachments mentioned nowhere: what a prune would delete, by id.
    pub eligible: Vec<Prunable>,
    pub eligible_total: Tally,
    /// The file's size on disk right now. Deleting rows does not shrink it by itself; see
    /// [`ReclaimPreview`].
    pub file_size_bytes: Option<u64>,
    /// The notebook has unsaved changes or pending attachments: the report is conservative
    /// (the file's references protect too) and a prune will be refused until it is saved.
    pub requires_save: bool,
}

/// Opaque proof that a particular report was shown. See the module documentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PruneToken([u64; 2]);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrunePreview {
    pub report: PruneReport,
    pub token: PruneToken,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PruneResult {
    /// Rows actually deleted, with their BLOB bytes, as committed.
    pub removed: Tally,
    /// What is stored afterwards.
    pub retained: Tally,
    /// A write happened (false when there was nothing to delete).
    pub wrote: bool,
    pub file_size_before: Option<u64>,
    pub file_size_after: Option<u64>,
    /// Set when rows were removed: the file keeps its size until space is reclaimed.
    pub warning: Option<&'static str>,
}

const SIZE_WARNING: &str = "The notebook file is not smaller yet: SQLite keeps the freed space \
     inside the file until space is reclaimed, which is a separate step.";

#[derive(Debug)]
pub enum PruneError {
    /// An untitled notebook has no file and no stored attachments.
    NoFile,
    /// Unsaved changes or pending attachments. Save (or discard), then preview again.
    NeedsSave,
    /// The notebook changed since the preview.
    Stale,
    /// Includes `ExternallyModified`: someone else wrote to the file.
    Storage(StorageError),
}

impl PruneError {
    pub fn is_conflict(&self) -> bool {
        matches!(self, PruneError::Storage(StorageError::ExternallyModified))
    }
}

impl fmt::Display for PruneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PruneError::NoFile => write!(f, "the notebook has no file yet"),
            PruneError::NeedsSave => write!(f, "save the notebook first, then review again"),
            PruneError::Stale => write!(f, "the notebook changed since it was reviewed"),
            PruneError::Storage(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for PruneError {}

impl From<StorageError> for PruneError {
    fn from(e: StorageError) -> Self {
        PruneError::Storage(e)
    }
}

/// 128 bits from two SipHash streams that start differently.
struct Digest(DefaultHasher, DefaultHasher);

impl Digest {
    fn new() -> Self {
        let mut a = DefaultHasher::new();
        let mut b = DefaultHasher::new();
        a.write(b"omatree-compact-a");
        b.write(b"omatree-compact-b\x01");
        Digest(a, b)
    }

    fn bytes(&mut self, v: &[u8]) {
        // Length-prefixed so that field boundaries cannot shift.
        self.u64(v.len() as u64);
        self.0.write(v);
        self.1.write(v);
    }

    fn u64(&mut self, v: u64) {
        self.0.write(&v.to_le_bytes());
        self.1.write(&v.to_le_bytes());
    }

    fn finish(self) -> [u64; 2] {
        [self.0.finish(), self.1.finish()]
    }
}

struct Analysis {
    report: PruneReport,
    digest: [u64; 2],
}

/// Records every id of `body` in `into`.
fn note_references(body: &str, into: &mut BTreeSet<String>) {
    scan_references(body, |id| {
        if !into.contains(id) {
            into.insert(id.to_owned());
        }
    });
}

impl Document {
    /// Everything `preview_attachment_prune` and `prune_attachments` decide from. Read-only.
    fn analyze_attachments(&self) -> Result<Analysis, PruneError> {
        let storage = self.storage.as_ref().ok_or(PruneError::NoFile)?;
        let inventory = storage.attachment_inventory()?;
        let stored: BTreeMap<&str, (u64, MediaType)> = inventory
            .iter()
            .map(|(id, len, media)| (id.as_str(), (*len, *media)))
            .collect();

        let mut digest = Digest::new();
        if let Some(p) = &self.path {
            digest.bytes(p.as_os_str().as_encoded_bytes());
        }

        // The notebook as it is in memory: truth about what the user has now.
        let (mut active, mut trash, mut checkpoints) =
            (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
        digest.u64(1);
        for node in self.notebook.depth_first() {
            digest.u64(node.id().get());
            digest.bytes(node.body().as_bytes());
            note_references(node.body(), &mut active);
        }
        digest.u64(2);
        for entry in self.recovery.trash() {
            digest.u64(entry.deleted_at() as u64);
            for node in entry.nodes() {
                digest.u64(node.id().get());
                digest.bytes(node.body().as_bytes());
                note_references(node.body(), &mut trash);
            }
        }
        digest.u64(3);
        for checkpoint in self.recovery.checkpoints() {
            digest.u64(checkpoint.created_at() as u64);
            for node in checkpoint.nodes() {
                digest.u64(node.id().get());
                digest.bytes(node.body().as_bytes());
                note_references(node.body(), &mut checkpoints);
            }
        }

        // The same bodies as the file holds them: what a crash would leave. Only when there is
        // something stored that could need protecting.
        let saved = if inventory.is_empty() {
            Default::default()
        } else {
            storage.stored_references()?
        };
        digest.u64(4);
        for set in [&saved.active, &saved.trash, &saved.checkpoints] {
            digest.u64(set.len() as u64);
            for id in set {
                digest.bytes(id.as_bytes());
            }
        }

        digest.u64(5);
        let pending = self.pending.borrow();
        let mut pending_tally = Tally::default();
        for (id, a) in pending.iter() {
            digest.bytes(id.as_str().as_bytes());
            digest.u64(a.data.len() as u64);
            pending_tally.add(a.data.len() as u64);
        }

        digest.u64(6);
        for (id, len, _) in &inventory {
            digest.bytes(id.as_str().as_bytes());
            digest.u64(*len);
        }

        let in_memory: BTreeSet<&str> = active
            .iter()
            .chain(&trash)
            .chain(&checkpoints)
            .map(String::as_str)
            .collect();
        let in_file: BTreeSet<&str> = saved
            .active
            .iter()
            .chain(&saved.trash)
            .chain(&saved.checkpoints)
            .map(String::as_str)
            .collect();

        let tally_of = |ids: &mut dyn Iterator<Item = &str>| {
            let mut t = Tally::default();
            for id in ids {
                if let Some((len, _)) = stored.get(id) {
                    t.add(*len);
                }
            }
            t
        };
        let in_active = tally_of(&mut active.iter().map(String::as_str));
        let in_trash = tally_of(&mut trash.iter().map(String::as_str));
        let in_checkpoints = tally_of(&mut checkpoints.iter().map(String::as_str));
        let only_in_unsaved_changes = tally_of(&mut in_memory.difference(&in_file).copied());
        let only_in_saved_file = tally_of(&mut in_file.difference(&in_memory).copied());

        let mut retained = Tally::default();
        let mut eligible = Vec::new();
        let mut eligible_total = Tally::default();
        for (id, len, media) in &inventory {
            let key = id.as_str();
            if in_memory.contains(key) || in_file.contains(key) {
                retained.add(*len);
            } else {
                eligible_total.add(*len);
                eligible.push(Prunable {
                    id: id.clone(),
                    bytes: *len,
                    media_type: *media,
                });
            }
        }
        let missing_references = in_memory
            .union(&in_file)
            .filter(|id| !stored.contains_key(**id) && !pending.keys().any(|p| p.as_str() == **id))
            .count();

        let file_size_bytes = self
            .path
            .as_ref()
            .and_then(|p| std::fs::metadata(p).ok())
            .map(|m| m.len());
        Ok(Analysis {
            digest: digest.finish(),
            report: PruneReport {
                stored: Tally {
                    count: inventory.len(),
                    bytes: inventory.iter().map(|a| a.1).sum(),
                },
                in_active,
                in_trash,
                in_checkpoints,
                only_in_unsaved_changes,
                only_in_saved_file,
                pending: pending_tally,
                missing_references,
                retained,
                eligible,
                eligible_total,
                file_size_bytes,
                requires_save: self.is_dirty(),
            },
        })
    }
}

// --- Space reclamation ------------------------------------------------------------------------

/// What `VACUUM` would be working with. Authorization is separate from pruning's: this token
/// cannot be used to prune and a prune token cannot reclaim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReclaimToken([u64; 2]);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReclaimPreview {
    /// The notebook file's size on disk right now (the main file; a `-wal` file is not counted).
    pub file_size_bytes: u64,
    pub page_size: u64,
    pub page_count: u64,
    /// Pages SQLite holds free inside the file. At most this many bytes can come back, and
    /// `VACUUM` may free less or more (it also repacks fragmented pages): not a promise.
    pub free_bytes: u64,
    /// Nothing is free, so a reclaim would not run.
    pub nothing_to_reclaim: bool,
    /// `delete` (the default for OmaTree files) or, if some other tool changed it, `wal`.
    pub journal_mode: String,
    pub token: ReclaimToken,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReclaimResult {
    /// `VACUUM` ran. False when no page was free.
    pub ran: bool,
    pub size_before: u64,
    pub size_after: u64,
    pub free_bytes_before: u64,
    pub free_bytes_after: u64,
}

impl ReclaimResult {
    /// Bytes the file actually shrank by; negative if it grew (it can, slightly).
    pub fn size_difference(&self) -> i64 {
        self.size_before as i64 - self.size_after as i64
    }
}

#[derive(Debug)]
pub enum ReclaimError {
    NoFile,
    /// The file changed since the preview.
    Stale,
    /// Another connection holds a lock on the file. Nothing changed; try again later.
    Busy,
    /// SQLite ran out of disk while rewriting the file. The original is intact.
    OutOfSpace,
    Storage(StorageError),
}

impl ReclaimError {
    pub fn is_conflict(&self) -> bool {
        matches!(
            self,
            ReclaimError::Storage(StorageError::ExternallyModified)
        )
    }
}

impl fmt::Display for ReclaimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReclaimError::NoFile => write!(f, "the notebook has no file yet"),
            ReclaimError::Stale => write!(f, "the notebook file changed since it was reviewed"),
            ReclaimError::Busy => write!(f, "the notebook file is in use by another program"),
            ReclaimError::OutOfSpace => write!(f, "not enough free disk space to compact"),
            ReclaimError::Storage(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ReclaimError {}

fn classify(e: StorageError) -> ReclaimError {
    use rusqlite::ErrorCode;
    match &e {
        StorageError::Sqlite(rusqlite::Error::SqliteFailure(f, _)) => match f.code {
            ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked => ReclaimError::Busy,
            ErrorCode::DiskFull => ReclaimError::OutOfSpace,
            _ => ReclaimError::Storage(e),
        },
        _ => ReclaimError::Storage(e),
    }
}

impl Document {
    /// A read-only report of what a prune would delete and keep, with the token that authorizes
    /// exactly that. Writes, saves and migrates nothing.
    pub fn preview_attachment_prune(&self) -> Result<PrunePreview, PruneError> {
        let a = self.analyze_attachments()?;
        Ok(PrunePreview {
            token: PruneToken(a.digest),
            report: a.report,
        })
    }

    /// Deletes the attachments `preview` reported as unreferenced, and only those, in one checked
    /// transaction. It never saves, never reclaims space and never accepts ids from the caller.
    ///
    /// Refused with `NeedsSave` while there are unsaved changes or pending attachments (nothing
    /// is lost or saved by being refused), with `Stale` if anything the preview rested on has
    /// changed, and with `ExternallyModified` if someone else has written to the file. A failure
    /// leaves the file, the pending attachments and the dirty state exactly as they were. With
    /// nothing to delete there is no write at all.
    pub fn prune_attachments(&mut self, token: &PruneToken) -> Result<PruneResult, PruneError> {
        if self.storage.is_none() {
            return Err(PruneError::NoFile);
        }
        if self.is_dirty() {
            return Err(PruneError::NeedsSave);
        }
        let analysis = self.analyze_attachments()?;
        if analysis.digest != token.0 {
            return Err(PruneError::Stale);
        }
        let report = analysis.report;
        if report.eligible.is_empty() {
            return Ok(PruneResult {
                removed: Tally::default(),
                retained: report.retained,
                wrote: false,
                file_size_before: report.file_size_bytes,
                file_size_after: report.file_size_bytes,
                warning: None,
            });
        }
        let ids: Vec<AttachmentId> = report.eligible.iter().map(|e| e.id.clone()).collect();
        let storage = self.storage.as_mut().ok_or(PruneError::NoFile)?;
        let outcome = storage.prune_attachments(&ids)?.ok_or(PruneError::Stale)?;
        let after = self
            .path
            .as_ref()
            .and_then(|p| std::fs::metadata(p).ok())
            .map(|m| m.len());
        Ok(PruneResult {
            removed: Tally {
                count: outcome.removed,
                bytes: outcome.removed_bytes,
            },
            retained: Tally {
                count: outcome.retained,
                bytes: outcome.retained_bytes,
            },
            wrote: true,
            file_size_before: report.file_size_bytes,
            file_size_after: after,
            warning: Some(SIZE_WARNING),
        })
    }

    /// What `reclaim_space` would do, read-only.
    pub fn preview_reclaim(&self) -> Result<ReclaimPreview, ReclaimError> {
        let (storage, path) = match (&self.storage, &self.path) {
            (Some(s), Some(p)) => (s, p),
            _ => return Err(ReclaimError::NoFile),
        };
        let stats: PhysicalStats = storage.physical_stats().map_err(classify)?;
        let file_size_bytes = std::fs::metadata(path)
            .map_err(|e| ReclaimError::Storage(StorageError::InvalidData(e.to_string())))?
            .len();
        let mut digest = Digest::new();
        digest.bytes(path.as_os_str().as_encoded_bytes());
        for v in [
            file_size_bytes,
            stats.page_size,
            stats.page_count,
            stats.freelist_pages,
            storage.schema_version() as u64,
        ] {
            digest.u64(v);
        }
        digest.bytes(stats.journal_mode.as_bytes());
        Ok(ReclaimPreview {
            file_size_bytes,
            page_size: stats.page_size,
            page_count: stats.page_count,
            free_bytes: stats.freelist_pages * stats.page_size,
            nothing_to_reclaim: stats.freelist_pages == 0,
            journal_mode: stats.journal_mode,
            token: ReclaimToken(digest.finish()),
        })
    }

    /// Rewrites the notebook file without its free pages (`VACUUM`, on the document's own
    /// connection). Authorized by a [`ReclaimToken`], independent of pruning, and never run by
    /// anything else. Does not need a saved notebook (it changes no content, and pending and
    /// dirty state are untouched). A failure leaves the original file: SQLite builds the new copy
    /// aside and only then replaces the pages, under its rollback journal.
    pub fn reclaim_space(&mut self, token: &ReclaimToken) -> Result<ReclaimResult, ReclaimError> {
        self.reclaim_space_waiting(token, Duration::from_secs(5))
    }

    pub(crate) fn reclaim_space_waiting(
        &mut self,
        token: &ReclaimToken,
        busy: Duration,
    ) -> Result<ReclaimResult, ReclaimError> {
        let before = self.preview_reclaim()?;
        if before.token != *token {
            return Err(ReclaimError::Stale);
        }
        if before.nothing_to_reclaim {
            return Ok(ReclaimResult {
                ran: false,
                size_before: before.file_size_bytes,
                size_after: before.file_size_bytes,
                free_bytes_before: 0,
                free_bytes_after: 0,
            });
        }
        let (Some(storage), Some(path)) = (self.storage.as_mut(), self.path.as_ref()) else {
            return Err(ReclaimError::NoFile);
        };
        storage.reclaim_space(path, busy).map_err(classify)?;
        let after = self.preview_reclaim()?;
        Ok(ReclaimResult {
            ran: true,
            size_before: before.file_size_bytes,
            size_after: after.file_size_bytes,
            free_bytes_before: before.free_bytes,
            free_bytes_after: after.free_bytes,
        })
    }
}
