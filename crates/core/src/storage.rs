//! SQLite persistence for notebooks. One notebook is one database file.
//!
//! Schema version 1 stores only the active nodes. Version 2 adds the id
//! counter, Trash and recovery checkpoints. Version 3 adds the set of
//! expanded nodes, which is view state rather than content. Version 4 adds
//! the `attachments` table (image BLOBs). Older files stay readable. A save
//! migrates a file older than 3 to 3; a file only becomes version 4 when a save
//! has an attachment to write, in the same transaction, so a notebook that never
//! holds an image stays a version 3 file. Migration is one explicit step per
//! version (`migrate`), never "re-run everything newer than the file".

use std::collections::BTreeSet;
use std::fmt;
use std::path::Path;
use std::time::Duration;

use rusqlite::{params, Connection, OpenFlags, Transaction};

use crate::attachments::{scan_references, Attachment, AttachmentId, AttachmentInfo, MediaType};
use crate::notebook::{Node, NodeId, Notebook, NotebookError};
use crate::recovery::{Checkpoint, Recovery, TrashEntry};

/// The newest schema this build understands.
const SCHEMA_VERSION: i64 = 4;
/// What a notebook without attachments is: new files are created at this version and a
/// text-only save never raises a file above it.
const BASE_VERSION: i64 = 3;
/// The first version that has the `attachments` table.
const ATTACHMENT_VERSION: i64 = 4;
/// The first version that has the recovery tables.
const RECOVERY_VERSION: i64 = 2;
const OLDEST_READABLE_VERSION: i64 = 1;

/// The active notebook, as in version 1.
const NODES_SCHEMA: &str = "
    CREATE TABLE nodes (
        id        INTEGER PRIMARY KEY,
        parent_id INTEGER REFERENCES nodes(id) ON DELETE CASCADE,
        position  INTEGER NOT NULL CHECK (position >= 0),
        title     TEXT NOT NULL,
        body      TEXT NOT NULL
    );
    CREATE INDEX nodes_parent ON nodes(parent_id, position);
";

/// What version 2 adds. Node rows of a trash entry or checkpoint are plain
/// rows too; their `parent_id` is not a foreign key because a trashed
/// subtree's original parent may no longer exist.
const RECOVERY_SCHEMA: &str = "
    CREATE TABLE notebook_meta (
        id           INTEGER PRIMARY KEY CHECK (id = 1),
        next_node_id INTEGER NOT NULL CHECK (next_node_id >= 0)
    );
    CREATE TABLE trash_entries (
        id           INTEGER PRIMARY KEY,
        deleted_at   INTEGER NOT NULL,
        root_node_id INTEGER NOT NULL
    );
    CREATE TABLE trash_nodes (
        entry_id  INTEGER NOT NULL REFERENCES trash_entries(id) ON DELETE CASCADE,
        node_id   INTEGER NOT NULL,
        parent_id INTEGER,
        position  INTEGER NOT NULL CHECK (position >= 0),
        title     TEXT NOT NULL,
        body      TEXT NOT NULL,
        PRIMARY KEY (entry_id, node_id)
    );
    CREATE TABLE checkpoints (
        id         INTEGER PRIMARY KEY,
        created_at INTEGER NOT NULL,
        reason     TEXT NOT NULL
    );
    CREATE TABLE checkpoint_nodes (
        checkpoint_id INTEGER NOT NULL REFERENCES checkpoints(id) ON DELETE CASCADE,
        node_id       INTEGER NOT NULL,
        parent_id     INTEGER,
        position      INTEGER NOT NULL CHECK (position >= 0),
        title         TEXT NOT NULL,
        body          TEXT NOT NULL,
        PRIMARY KEY (checkpoint_id, node_id)
    );
";

/// What version 3 adds: which notes are open in the tree. Only a note that
/// exists can be listed, so a deleted note takes its row with it.
const VIEW_SCHEMA: &str = "
    CREATE TABLE expanded_nodes (
        node_id INTEGER PRIMARY KEY REFERENCES nodes(id) ON DELETE CASCADE
    );
";

/// What version 4 adds. No foreign key to nodes, Trash or checkpoints: what an attachment
/// belongs to is derived from the Markdown bodies, and `DELETE FROM nodes` (every save) must
/// never reach it. `byte_len` must agree with the BLOB.
const ATTACHMENT_SCHEMA: &str = "
    CREATE TABLE attachments (
        id         TEXT PRIMARY KEY CHECK (length(id) = 32),
        media_type TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        byte_len   INTEGER NOT NULL CHECK (byte_len > 0),
        data       BLOB NOT NULL,
        CHECK (byte_len = length(data))
    ) WITHOUT ROWID;
";

/// What a save should do about attachments besides keeping those already stored.
#[derive(Default)]
pub struct AttachmentPlan<'a> {
    /// Held in memory until now; inserted by this save.
    pub pending: Vec<&'a Attachment>,
    /// Another notebook file whose stored attachments are copied into this one (Save As).
    pub copy_from: Option<&'a Storage>,
    /// This file is an existing notebook being replaced: what it stored is dropped first.
    pub replace_existing: bool,
}

/// Which kind of stored body mentions which attachment ids (the sets overlap).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StoredReferences {
    pub active: BTreeSet<String>,
    pub trash: BTreeSet<String>,
    pub checkpoints: BTreeSet<String>,
}

/// What a committed prune removed and left.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PruneOutcome {
    pub removed: usize,
    pub removed_bytes: u64,
    pub retained: usize,
    pub retained_bytes: u64,
}

/// SQLite's own view of the file's pages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicalStats {
    pub page_size: u64,
    pub page_count: u64,
    pub freelist_pages: u64,
    pub journal_mode: String,
}

#[derive(Debug)]
pub enum StorageError {
    Sqlite(rusqlite::Error),
    /// Another connection has committed to the file since this one last
    /// looked, so a write from what this process holds could undo it.
    ExternallyModified,
    UnsupportedSchemaVersion(i64),
    InvalidData(String),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageError::Sqlite(e) => write!(f, "database error: {e}"),
            StorageError::ExternallyModified => {
                write!(f, "the notebook file was changed by someone else")
            }
            StorageError::UnsupportedSchemaVersion(v) => {
                write!(f, "unsupported notebook schema version {v}")
            }
            StorageError::InvalidData(why) => write!(f, "invalid notebook data: {why}"),
        }
    }
}

impl std::error::Error for StorageError {}

impl From<rusqlite::Error> for StorageError {
    fn from(e: rusqlite::Error) -> Self {
        StorageError::Sqlite(e)
    }
}

impl From<NotebookError> for StorageError {
    fn from(e: NotebookError) -> Self {
        StorageError::InvalidData(e.to_string())
    }
}

pub struct Storage {
    conn: Connection,
    /// Schema version of the file as it is on disk right now (1 to 4).
    version: i64,
    /// `PRAGMA data_version` of `conn`, as last seen. SQLite changes that
    /// number for a connection when *another* connection commits, and not for
    /// the connection's own commits, so comparing it with itself over time (on
    /// this one connection, never with another's number) says whether anyone
    /// else has written since. Taken when the file is opened, before it is
    /// read, so anything slipping in between can only cause a refusal, never a
    /// missed overwrite.
    observed: i64,
}

impl Storage {
    /// Creates a version 3 notebook in a new (or empty) database file (attachments make it 4
    /// later, with the save that stores the first one).
    /// Refuses to touch a database that already contains anything.
    pub fn create(path: &Path) -> Result<Self, StorageError> {
        let mut conn = Connection::open(path)?;
        conn.pragma_update(None, "foreign_keys", true)?;

        let version = user_version(&conn)?;
        if version != 0 {
            return Err(StorageError::UnsupportedSchemaVersion(version));
        }
        let tables: i64 =
            conn.query_row("SELECT count(*) FROM sqlite_master", [], |row| row.get(0))?;
        if tables != 0 {
            return Err(StorageError::InvalidData(
                "database is not empty".to_string(),
            ));
        }

        let tx = conn.transaction()?;
        tx.execute_batch(NODES_SCHEMA)?;
        tx.execute_batch(RECOVERY_SCHEMA)?;
        tx.execute_batch(VIEW_SCHEMA)?;
        tx.execute(
            "INSERT INTO notebook_meta (id, next_node_id) VALUES (1, 0)",
            [],
        )?;
        tx.pragma_update(None, "user_version", BASE_VERSION)?;
        tx.commit()?;
        let observed = data_version(&conn)?;
        Ok(Storage {
            conn,
            version: BASE_VERSION,
            observed,
        })
    }

    /// Opens an existing notebook (schema version 1 to 4); never creates a
    /// file and never modifies one. An older file is only upgraded by a
    /// later successful `save_document`.
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.pragma_update(None, "foreign_keys", true)?;

        let version = user_version(&conn)?;
        if !(OLDEST_READABLE_VERSION..=SCHEMA_VERSION).contains(&version) {
            return Err(StorageError::UnsupportedSchemaVersion(version));
        }
        let observed = data_version(&conn)?;
        Ok(Storage {
            conn,
            version,
            observed,
        })
    }

    /// Replaces everything stored (active notebook, id counter, Trash,
    /// checkpoints and the expanded nodes) with the given state in a single
    /// transaction. Attachments already stored are kept. An older file is migrated inside the same
    /// transaction, so on any failure the previous valid file is left exactly as it was. Expanded
    /// ids of nodes that do not exist are left out.
    pub fn save_document_with_view(
        &mut self,
        notebook: &Notebook,
        recovery: &Recovery,
        expanded: &BTreeSet<NodeId>,
    ) -> Result<(), StorageError> {
        self.save_document_with_attachments(
            notebook,
            recovery,
            expanded,
            &AttachmentPlan::default(),
        )
    }

    /// `save_document_with_view` that also writes attachments (see [`AttachmentPlan`]) in the
    /// same checked transaction on the same connection: all of it commits or none of it does,
    /// and the external-modification guard covers it. The file becomes version 4 here, inside
    /// that transaction, if (and only if) there is anything to store in the attachments table.
    pub fn save_document_with_attachments(
        &mut self,
        notebook: &Notebook,
        recovery: &Recovery,
        expanded: &BTreeSet<NodeId>,
        plan: &AttachmentPlan,
    ) -> Result<(), StorageError> {
        let from = self.version;
        let wants = !plan.pending.is_empty() || plan.copy_from.is_some_and(Storage::has_stored);
        let to = target_version(from.max(BASE_VERSION), wants);
        let tx = self.begin_checked_write()?;
        migrate(&tx, from, to)?;
        write_attachments(&tx, to, plan)?;

        // Children first for the nodes foreign key; the other tables cascade.
        tx.execute("DELETE FROM expanded_nodes", [])?;
        tx.execute("DELETE FROM checkpoints", [])?;
        tx.execute("DELETE FROM trash_entries", [])?;
        tx.execute("DELETE FROM notebook_meta", [])?;
        tx.execute("DELETE FROM nodes", [])?;

        write_nodes(&tx, notebook)?;
        write_meta(&tx, full_next_id(notebook, recovery)?)?;
        write_trash(&tx, recovery)?;
        write_checkpoints(&tx, recovery)?;
        write_expanded(&tx, notebook, expanded)?;

        if to != from {
            tx.pragma_update(None, "user_version", to)?;
        }
        tx.commit()?;
        self.version = to;
        Ok(())
    }

    /// Accepts the file as it is now as the baseline. Tests only: the test
    /// helpers that install write guards are commits by another connection.
    #[cfg(test)]
    pub(crate) fn resync(&mut self) {
        self.observed = data_version(&self.conn).unwrap();
    }

    /// Starts a write transaction that holds the write lock from the start
    /// (so nobody can commit between the check and the writes) and refuses
    /// when the file has been changed by another connection since this one
    /// last saw it.
    fn begin_checked_write(&mut self) -> Result<Transaction<'_>, StorageError> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if data_version(&tx)? != self.observed {
            // Dropping `tx` rolls back; nothing was written.
            return Err(StorageError::ExternallyModified);
        }
        Ok(tx)
    }

    /// Like `save_document_with_view`, with nothing expanded. Tests only.
    #[cfg(test)]
    pub fn save_document(
        &mut self,
        notebook: &Notebook,
        recovery: &Recovery,
    ) -> Result<(), StorageError> {
        self.save_document_with_view(notebook, recovery, &BTreeSet::new())
    }

    /// Whether the file already has the recovery and view tables (version 3 or 4) and supports `save_active_with_view`.
    pub fn is_current_schema(&self) -> bool {
        self.version >= BASE_VERSION
    }

    /// Persists only the active state: the `nodes` table, the next node id
    /// and the expanded nodes, in one transaction. Trash and checkpoint
    /// tables are not touched at all. Only for a file that is already at the
    /// current schema; an older file needs the migrating save instead.
    pub fn save_active_with_view(
        &mut self,
        notebook: &Notebook,
        expanded: &BTreeSet<NodeId>,
    ) -> Result<(), StorageError> {
        self.save_active_with_attachments(notebook, expanded, &[])
    }

    /// `save_active_with_view` that also inserts `pending` attachments, in the same checked
    /// transaction. A version 3 file is raised to 4 inside it when there is something to insert.
    pub fn save_active_with_attachments(
        &mut self,
        notebook: &Notebook,
        expanded: &BTreeSet<NodeId>,
        pending: &[&Attachment],
    ) -> Result<(), StorageError> {
        if !self.is_current_schema() {
            return Err(StorageError::UnsupportedSchemaVersion(self.version));
        }
        let from = self.version;
        let to = target_version(from, !pending.is_empty());
        let tx = self.begin_checked_write()?;
        migrate(&tx, from, to)?;
        write_attachments(
            &tx,
            to,
            &AttachmentPlan {
                pending: pending.to_vec(),
                ..AttachmentPlan::default()
            },
        )?;
        // Deleting the nodes also deletes their expanded rows (cascade); the
        // current set is written back below.
        tx.execute("DELETE FROM expanded_nodes", [])?;
        tx.execute("DELETE FROM nodes", [])?;
        write_nodes(&tx, notebook)?;
        // The stored counter only ever goes up, so ids that live only in
        // Trash or checkpoints can never become reusable through this path.
        let next = i64::try_from(notebook.next_id())
            .map_err(|_| StorageError::InvalidData("node id counter is too large".into()))?;
        tx.execute(
            "INSERT INTO notebook_meta (id, next_node_id) VALUES (1, ?1)
             ON CONFLICT (id) DO UPDATE SET
                 next_node_id = max(next_node_id, excluded.next_node_id)",
            [next],
        )?;
        write_expanded(&tx, notebook, expanded)?;
        if to != from {
            tx.pragma_update(None, "user_version", to)?;
        }
        tx.commit()?;
        self.version = to;
        Ok(())
    }

    /// Like `save_active_with_view`, with nothing expanded. Tests only.
    #[cfg(test)]
    pub fn save_active(&mut self, notebook: &Notebook) -> Result<(), StorageError> {
        self.save_active_with_view(notebook, &BTreeSet::new())
    }

    /// The stored expanded nodes that exist in `notebook`. A file older than
    /// version 3 has none. Ids of missing nodes are ignored, not an error.
    pub fn load_expanded(&self, notebook: &Notebook) -> Result<BTreeSet<NodeId>, StorageError> {
        if self.version < BASE_VERSION {
            return Ok(BTreeSet::new());
        }
        let mut query = self.conn.prepare("SELECT node_id FROM expanded_nodes")?;
        let rows = query.query_map([], |row| row.get::<_, i64>(0))?;
        let mut expanded = BTreeSet::new();
        for row in rows {
            let id = id_from_sql(row?)?;
            if notebook.get(id).is_some() {
                expanded.insert(id);
            }
        }
        Ok(expanded)
    }

    /// The schema version of the file as it is now.
    pub fn schema_version(&self) -> i64 {
        self.version
    }

    /// Whether the attachments table holds anything.
    pub fn has_stored(&self) -> bool {
        self.version >= ATTACHMENT_VERSION
            && self
                .conn
                .query_row("SELECT EXISTS (SELECT 1 FROM attachments)", [], |r| {
                    r.get(0)
                })
                .unwrap_or(false)
    }

    /// The ids of every stored attachment (none before version 4).
    pub fn attachment_ids(&self) -> Result<Vec<AttachmentId>, StorageError> {
        if self.version < ATTACHMENT_VERSION {
            return Ok(Vec::new());
        }
        let mut q = self
            .conn
            .prepare("SELECT id FROM attachments ORDER BY id")?;
        let rows = q.query_map([], |r| r.get::<_, String>(0))?;
        let mut ids = Vec::new();
        for row in rows {
            ids.push(
                AttachmentId::parse(&row?).map_err(|e| StorageError::InvalidData(e.to_string()))?,
            );
        }
        Ok(ids)
    }

    /// Size and media type of a stored attachment, without reading the bytes.
    pub fn attachment_info(
        &self,
        id: &AttachmentId,
    ) -> Result<Option<AttachmentInfo>, StorageError> {
        if self.version < ATTACHMENT_VERSION {
            return Ok(None);
        }
        let row = self
            .conn
            .query_row(
                "SELECT media_type, byte_len FROM attachments WHERE id = ?1",
                [id.as_str()],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        row.map(|(mime, len)| {
            Ok(AttachmentInfo {
                len: u64::try_from(len)
                    .map_err(|_| StorageError::InvalidData("negative attachment size".into()))?,
                media_type: MediaType::from_mime(&mime).ok_or_else(|| {
                    StorageError::InvalidData(format!("attachment of type {mime}"))
                })?,
            })
        })
        .transpose()
    }

    /// A stored attachment with its bytes, checked against its recorded length.
    pub fn attachment(&self, id: &AttachmentId) -> Result<Option<Attachment>, StorageError> {
        if self.version < ATTACHMENT_VERSION {
            return Ok(None);
        }
        let row = self
            .conn
            .query_row(
                "SELECT media_type, created_at, byte_len, data FROM attachments WHERE id = ?1",
                [id.as_str()],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, Vec<u8>>(3)?,
                    ))
                },
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        let Some((mime, created_at, len, data)) = row else {
            return Ok(None);
        };
        if i64::try_from(data.len()).ok() != Some(len) {
            return Err(StorageError::InvalidData(format!(
                "attachment {id} records {len} bytes but holds {}",
                data.len()
            )));
        }
        let media_type = MediaType::from_mime(&mime)
            .ok_or_else(|| StorageError::InvalidData(format!("attachment of type {mime}")))?;
        Ok(Some(Attachment {
            id: id.clone(),
            media_type,
            created_at,
            data,
        }))
    }

    /// Id, size and media type of every stored attachment, by id, without reading any BLOB.
    pub fn attachment_inventory(
        &self,
    ) -> Result<Vec<(AttachmentId, u64, MediaType)>, StorageError> {
        inventory(&self.conn, self.version)
    }

    /// Every attachment id mentioned by a stored body: active notes, Trash (all descendants)
    /// and checkpoints, by the plain-text rule of `scan_references`. Reads bodies only, never
    /// a BLOB, and writes nothing. Empty before version 4 (there is nothing to protect).
    pub fn stored_references(&self) -> Result<StoredReferences, StorageError> {
        references(&self.conn, self.version)
    }

    /// Deletes exactly the attachments in `expected`, in one checked transaction on this
    /// connection, and only if they are, inside that transaction, exactly the attachments that no
    /// stored body mentions. Any difference (a reference appeared, an attachment is gone or new)
    /// is `Ok(None)` and nothing is deleted. Notes, Trash, checkpoints and the expanded
    /// nodes are not written. An empty `expected` does nothing, not even open a transaction.
    pub fn prune_attachments(
        &mut self,
        expected: &[AttachmentId],
    ) -> Result<Option<PruneOutcome>, StorageError> {
        if expected.is_empty() {
            let all = inventory(&self.conn, self.version)?;
            return Ok(Some(PruneOutcome {
                removed: 0,
                removed_bytes: 0,
                retained: all.len(),
                retained_bytes: all.iter().map(|a| a.1).sum(),
            }));
        }
        let version = self.version;
        let tx = self.begin_checked_write()?;
        let refs = references(&tx, version)?;
        let mut removable = Vec::new();
        let mut retained = 0;
        let mut retained_bytes = 0;
        for (id, len, _) in inventory(&tx, version)? {
            let id_text = id.as_str();
            if refs.active.contains(id_text)
                || refs.trash.contains(id_text)
                || refs.checkpoints.contains(id_text)
            {
                retained += 1;
                retained_bytes += len;
            } else {
                removable.push((id, len));
            }
        }
        // `inventory` is ordered by id, so `removable` is too.
        let same = removable.len() == expected.len()
            && removable
                .iter()
                .map(|r| &r.0)
                .zip(expected)
                .all(|(a, b)| a == b);
        if !same {
            return Ok(None);
        }
        let mut removed_bytes = 0;
        for (id, len) in &removable {
            let n = tx.execute("DELETE FROM attachments WHERE id = ?1", [id.as_str()])?;
            if n != 1 {
                return Err(StorageError::InvalidData(format!(
                    "attachment {id} could not be removed"
                )));
            }
            removed_bytes += len;
        }
        tx.commit()?;
        Ok(Some(PruneOutcome {
            removed: removable.len(),
            removed_bytes,
            retained,
            retained_bytes,
        }))
    }

    /// Page counts and journal mode of the file, from SQLite itself.
    pub fn physical_stats(&self) -> Result<PhysicalStats, StorageError> {
        let n = |name: &str| -> Result<u64, StorageError> {
            let v: i64 = self.conn.pragma_query_value(None, name, |r| r.get(0))?;
            u64::try_from(v).map_err(|_| StorageError::InvalidData(format!("negative {name}")))
        };
        Ok(PhysicalStats {
            page_size: n("page_size")?,
            page_count: n("page_count")?,
            freelist_pages: n("freelist_count")?,
            journal_mode: self
                .conn
                .pragma_query_value(None, "journal_mode", |r| r.get(0))?,
        })
    }

    /// `VACUUM` on this connection: rewrites the file without its free pages. Refused when
    /// another connection has committed since this one last looked (`ExternallyModified`),
    /// like a save. `VACUUM` cannot run inside a transaction, so a commit by someone else between
    /// that check and SQLite taking its lock is not prevented, but it is never accepted either:
    /// this connection's baseline (`observed`) is left alone, so the next save is refused rather
    /// than overwriting the other writer. `busy` is how long to wait for a lock held elsewhere.
    ///
    /// A *failed* `VACUUM` rolls back, but SQLite then counts it in `data_version` as if someone
    /// else had written (measured: 1 to 2 on a read-only failure). That would make this
    /// document's next save a false conflict. So after a failure the baseline is re-accepted only
    /// when the file's own change counter (header bytes 24..28, which every commit in rollback
    /// journal mode increments) proves nobody committed meanwhile; in WAL mode, where the header
    /// proves nothing, it is never re-accepted (a refusal, the safe side).
    pub(crate) fn reclaim_space(
        &mut self,
        path: &Path,
        busy: Duration,
    ) -> Result<(), StorageError> {
        if data_version(&self.conn)? != self.observed {
            return Err(StorageError::ExternallyModified);
        }
        let counter_before = change_counter(path);
        self.conn.busy_timeout(busy)?;
        let ran = self.conn.execute_batch("VACUUM");
        // Always put the default wait back, whatever happened.
        let restored = self.conn.busy_timeout(Duration::from_secs(5));
        if let Err(e) = ran {
            let rollback_journal = self
                .physical_stats()
                .is_ok_and(|s| s.journal_mode.eq_ignore_ascii_case("delete"));
            if rollback_journal
                && counter_before.is_some()
                && change_counter(path) == counter_before
            {
                if let Ok(now) = data_version(&self.conn) {
                    self.observed = now;
                }
            }
            return Err(e.into());
        }
        restored?;
        // In WAL mode the vacuum lives in the log until a checkpoint moves it into the file.
        if self
            .physical_stats()?
            .journal_mode
            .eq_ignore_ascii_case("wal")
        {
            self.conn
                .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        }
        Ok(())
    }

    /// Loads and validates the stored notebook and recovery state. A
    /// version 1 file yields empty recovery state and an id counter derived
    /// from its nodes.
    pub fn load_document(&self) -> Result<(Notebook, Recovery), StorageError> {
        let nodes = read_nodes(
            &self.conn,
            "SELECT id, parent_id, position, title, body FROM nodes",
            [],
        )?;
        if self.version < RECOVERY_VERSION {
            return Ok((Notebook::from_nodes(nodes)?, Recovery::default()));
        }

        let next_id: i64 = self
            .conn
            .query_row(
                "SELECT next_node_id FROM notebook_meta WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    StorageError::InvalidData("missing notebook metadata".into())
                }
                other => other.into(),
            })?;
        let next_id = u64::try_from(next_id)
            .map_err(|_| StorageError::InvalidData("negative node id counter".into()))?;
        let mut notebook = Notebook::from_nodes_with_next_id(nodes, next_id)?;

        let recovery = self.load_recovery()?;
        // Ids that only exist in Trash or checkpoints must never be reused,
        // even if the stored counter is behind.
        if let Some(max) = recovery.max_node_id() {
            notebook.reserve_ids_through(max)?;
        }
        Ok((notebook, recovery))
    }

    fn load_recovery(&self) -> Result<Recovery, StorageError> {
        let mut trash = Vec::new();
        let mut entries = self
            .conn
            .prepare("SELECT id, deleted_at, root_node_id FROM trash_entries ORDER BY id")?;
        let entries = entries.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        for entry in entries {
            let (entry_id, deleted_at, root) = entry?;
            let root = id_from_sql(root)?;
            let mut nodes = read_nodes(
                &self.conn,
                "SELECT node_id, parent_id, position, title, body
                 FROM trash_nodes WHERE entry_id = ?1 ORDER BY node_id",
                [entry_id],
            )?;
            // The entry's root goes first.
            let at = nodes
                .iter()
                .position(|n| n.id() == root)
                .ok_or_else(|| StorageError::InvalidData("trash entry has no root".into()))?;
            nodes.swap(0, at);
            trash.push(TrashEntry::new(deleted_at, nodes)?);
        }

        let mut checkpoints = Vec::new();
        let mut rows = self
            .conn
            .prepare("SELECT id, created_at, reason FROM checkpoints ORDER BY id")?;
        let rows = rows.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (checkpoint_id, created_at, reason) = row?;
            let nodes = read_nodes(
                &self.conn,
                "SELECT node_id, parent_id, position, title, body
                 FROM checkpoint_nodes WHERE checkpoint_id = ?1 ORDER BY node_id",
                [checkpoint_id],
            )?;
            checkpoints.push(Checkpoint::from_stored(created_at, reason, nodes)?);
        }
        Ok(Recovery::from_parts(trash, checkpoints))
    }

    /// Saves just a notebook with empty recovery state. Tests only.
    #[cfg(test)]
    pub fn save(&mut self, notebook: &Notebook) -> Result<(), StorageError> {
        self.save_document(notebook, &Recovery::default())
    }

    /// Loads just the active notebook. Tests only.
    #[cfg(test)]
    pub fn load(&self) -> Result<Notebook, StorageError> {
        Ok(self.load_document()?.0)
    }
}

/// The version a save leaves the file at: it never goes down, and only a save that has something
/// for the attachments table takes a version 3 file to 4.
fn target_version(at_least: i64, has_attachments: bool) -> i64 {
    if has_attachments {
        at_least.max(ATTACHMENT_VERSION)
    } else {
        at_least
    }
}

/// One explicit step per version. `to` is never lower than `from` and never above the newest.
fn migrate(tx: &Transaction, from: i64, to: i64) -> Result<(), StorageError> {
    if from < RECOVERY_VERSION && to >= RECOVERY_VERSION {
        tx.execute_batch(RECOVERY_SCHEMA)?;
    }
    if from < BASE_VERSION && to >= BASE_VERSION {
        tx.execute_batch(VIEW_SCHEMA)?;
    }
    if from < ATTACHMENT_VERSION && to >= ATTACHMENT_VERSION {
        tx.execute_batch(ATTACHMENT_SCHEMA)?;
    }
    Ok(())
}

/// The attachment part of a save, inside its transaction: drop what an existing file held when
/// it is being replaced, copy another file's attachments, insert the pending ones. A plain
/// INSERT: an id that already exists fails the whole save rather than replacing data.
fn write_attachments(
    tx: &Transaction,
    version: i64,
    plan: &AttachmentPlan,
) -> Result<(), StorageError> {
    if version < ATTACHMENT_VERSION {
        return Ok(());
    }
    if plan.replace_existing {
        tx.execute("DELETE FROM attachments", [])?;
    }
    let mut insert = tx.prepare(
        "INSERT INTO attachments (id, media_type, created_at, byte_len, data)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    let mut put = |a: &Attachment| -> Result<(), StorageError> {
        let len = i64::try_from(a.data.len())
            .map_err(|_| StorageError::InvalidData("attachment too large".into()))?;
        insert.execute(params![
            a.id.as_str(),
            a.media_type.mime(),
            a.created_at,
            len,
            a.data
        ])?;
        Ok(())
    };
    if let Some(source) = plan.copy_from {
        for id in source.attachment_ids()? {
            let a = source
                .attachment(&id)?
                .ok_or_else(|| StorageError::InvalidData(format!("attachment {id} vanished")))?;
            put(&a)?;
        }
    }
    for a in &plan.pending {
        put(a)?;
    }
    Ok(())
}

fn write_nodes(tx: &Transaction, notebook: &Notebook) -> Result<(), StorageError> {
    let mut insert = tx.prepare(
        "INSERT INTO nodes (id, parent_id, position, title, body)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    // Parents come before children, as the foreign key requires.
    for node in notebook.in_tree_order() {
        insert_node(&mut insert, &[], node)?;
    }
    Ok(())
}

/// The id counter must stay above every id that exists anywhere.
fn full_next_id(notebook: &Notebook, recovery: &Recovery) -> Result<u64, StorageError> {
    let mut next = notebook.next_id();
    if let Some(max) = recovery.max_node_id() {
        let needed = max
            .checked_add(1)
            .ok_or_else(|| StorageError::InvalidData("node id out of range".into()))?;
        next = next.max(needed);
    }
    Ok(next)
}

/// Writes the expanded nodes that exist in the notebook; others are dropped.
fn write_expanded(
    tx: &Transaction,
    notebook: &Notebook,
    expanded: &BTreeSet<NodeId>,
) -> Result<(), StorageError> {
    let mut insert = tx.prepare("INSERT INTO expanded_nodes (node_id) VALUES (?1)")?;
    for id in expanded.iter().filter(|id| notebook.get(**id).is_some()) {
        insert.execute([id_to_sql(*id)?])?;
    }
    Ok(())
}

fn write_meta(tx: &Transaction, next_id: u64) -> Result<(), StorageError> {
    let next = i64::try_from(next_id)
        .map_err(|_| StorageError::InvalidData("node id counter is too large".into()))?;
    tx.execute(
        "INSERT INTO notebook_meta (id, next_node_id) VALUES (1, ?1)",
        [next],
    )?;
    Ok(())
}

fn write_trash(tx: &Transaction, recovery: &Recovery) -> Result<(), StorageError> {
    let mut entry =
        tx.prepare("INSERT INTO trash_entries (id, deleted_at, root_node_id) VALUES (?1, ?2, ?3)")?;
    let mut node = tx.prepare(
        "INSERT INTO trash_nodes (entry_id, node_id, parent_id, position, title, body)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    // Row ids follow list order (oldest first), which load reads back.
    for (i, trashed) in recovery.trash().iter().enumerate() {
        let entry_id = i64::try_from(i + 1)
            .map_err(|_| StorageError::InvalidData("too many trash entries".into()))?;
        entry.execute(params![
            entry_id,
            trashed.deleted_at(),
            id_to_sql(trashed.root().id())?
        ])?;
        for n in trashed.nodes() {
            insert_node(&mut node, &[entry_id.into()], n)?;
        }
    }
    Ok(())
}

fn write_checkpoints(tx: &Transaction, recovery: &Recovery) -> Result<(), StorageError> {
    let mut checkpoint =
        tx.prepare("INSERT INTO checkpoints (id, created_at, reason) VALUES (?1, ?2, ?3)")?;
    let mut node = tx.prepare(
        "INSERT INTO checkpoint_nodes
             (checkpoint_id, node_id, parent_id, position, title, body)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for (i, saved) in recovery.checkpoints().iter().enumerate() {
        let checkpoint_id = i64::try_from(i + 1)
            .map_err(|_| StorageError::InvalidData("too many checkpoints".into()))?;
        checkpoint.execute(params![checkpoint_id, saved.created_at(), saved.reason()])?;
        for n in saved.nodes() {
            insert_node(&mut node, &[checkpoint_id.into()], n)?;
        }
    }
    Ok(())
}

/// Executes a node insert whose leading parameters (`lead`, e.g. an entry
/// id) are followed by the five node columns.
fn insert_node(
    insert: &mut rusqlite::Statement,
    lead: &[rusqlite::types::Value],
    node: &Node,
) -> Result<(), StorageError> {
    use rusqlite::types::Value;
    let mut values: Vec<Value> = lead.to_vec();
    values.push(Value::Integer(id_to_sql(node.id())?));
    values.push(
        node.parent_id()
            .map(id_to_sql)
            .transpose()?
            .map_or(Value::Null, Value::Integer),
    );
    values.push(Value::Integer(i64::try_from(node.position()).map_err(
        |_| StorageError::InvalidData("position too large".into()),
    )?));
    values.push(Value::Text(node.title().to_string()));
    values.push(Value::Text(node.body().to_string()));
    insert.execute(rusqlite::params_from_iter(values))?;
    Ok(())
}

/// Reads rows of (id, parent_id, position, title, body) into nodes.
fn read_nodes<P: rusqlite::Params>(
    conn: &Connection,
    sql: &str,
    params: P,
) -> Result<Vec<Node>, StorageError> {
    let mut query = conn.prepare(sql)?;
    let rows = query.query_map(params, |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<i64>>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;

    let mut nodes = Vec::new();
    for row in rows {
        let (id, parent_id, position, title, body) = row?;
        let position = usize::try_from(position)
            .map_err(|_| StorageError::InvalidData("negative position".into()))?;
        nodes.push(Node::from_parts(
            id_from_sql(id)?,
            parent_id.map(id_from_sql).transpose()?,
            position,
            title,
            body,
        ));
    }
    Ok(nodes)
}

fn data_version(conn: &Connection) -> Result<i64, StorageError> {
    Ok(conn.pragma_query_value(None, "data_version", |row| row.get(0))?)
}

/// The file change counter of the database header (bytes 24..28), if the file can be read.
fn change_counter(path: &Path) -> Option<u32> {
    use std::io::Read;
    let mut header = [0u8; 28];
    std::fs::File::open(path)
        .ok()?
        .read_exact(&mut header)
        .ok()?;
    Some(u32::from_be_bytes([
        header[24], header[25], header[26], header[27],
    ]))
}

fn inventory(
    conn: &Connection,
    version: i64,
) -> Result<Vec<(AttachmentId, u64, MediaType)>, StorageError> {
    if version < ATTACHMENT_VERSION {
        return Ok(Vec::new());
    }
    let mut q = conn.prepare("SELECT id, byte_len, media_type FROM attachments ORDER BY id")?;
    let rows = q.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    let mut all = Vec::new();
    for row in rows {
        let (id, len, mime) = row?;
        all.push((
            AttachmentId::parse(&id).map_err(|e| StorageError::InvalidData(e.to_string()))?,
            u64::try_from(len)
                .map_err(|_| StorageError::InvalidData("negative attachment size".into()))?,
            MediaType::from_mime(&mime)
                .ok_or_else(|| StorageError::InvalidData(format!("attachment of type {mime}")))?,
        ));
    }
    Ok(all)
}

fn references(conn: &Connection, version: i64) -> Result<StoredReferences, StorageError> {
    let mut out = StoredReferences::default();
    if version < ATTACHMENT_VERSION {
        return Ok(out);
    }
    collect_references(conn, "SELECT body FROM nodes", &mut out.active)?;
    collect_references(conn, "SELECT body FROM trash_nodes", &mut out.trash)?;
    collect_references(
        conn,
        "SELECT body FROM checkpoint_nodes",
        &mut out.checkpoints,
    )?;
    Ok(out)
}

fn collect_references(
    conn: &Connection,
    sql: &str,
    into: &mut BTreeSet<String>,
) -> Result<(), StorageError> {
    let mut q = conn.prepare(sql)?;
    let mut rows = q.query([])?;
    while let Some(row) = rows.next()? {
        // Borrowed straight from SQLite's buffer: no copy of the body.
        let body = row.get_ref(0)?.as_str().map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?;
        scan_references(body, |id| {
            if !into.contains(id) {
                into.insert(id.to_owned());
            }
        });
    }
    Ok(())
}

fn user_version(conn: &Connection) -> Result<i64, StorageError> {
    Ok(conn.pragma_query_value(None, "user_version", |row| row.get(0))?)
}

fn id_to_sql(id: NodeId) -> Result<i64, StorageError> {
    i64::try_from(id.get())
        .map_err(|_| StorageError::InvalidData(format!("node id {} is too large", id.get())))
}

fn id_from_sql(raw: i64) -> Result<NodeId, StorageError> {
    u64::try_from(raw)
        .map(NodeId::from_raw)
        .map_err(|_| StorageError::InvalidData(format!("negative node id {raw}")))
}

/// Test support: makes any INSERT, UPDATE or DELETE on `tables` abort, so a
/// test can prove a save never writes there (rather than merely that the
/// contents compare equal afterwards).
#[cfg(test)]
pub(crate) fn install_write_guards(path: &Path, tables: &[&str]) {
    let conn = Connection::open(path).unwrap();
    for table in tables {
        for event in ["INSERT", "UPDATE", "DELETE"] {
            conn.execute_batch(&format!(
                "CREATE TRIGGER guard_{table}_{event} BEFORE {event} ON {table}
                 BEGIN SELECT RAISE(ABORT, 'guarded table written: {table}'); END;"
            ))
            .unwrap();
        }
    }
}

/// Test support: removes every trigger installed by `install_write_guards`.
#[cfg(test)]
pub(crate) fn remove_write_guards(path: &Path) {
    let conn = Connection::open(path).unwrap();
    let names: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'trigger'")
            .unwrap();
        let rows = stmt.query_map([], |row| row.get(0)).unwrap();
        rows.map(Result::unwrap).collect()
    };
    for name in names {
        conn.execute_batch(&format!("DROP TRIGGER {name}")).unwrap();
    }
}

/// Test support: every row of `table` with its rowid, as text, in rowid order.
#[cfg(test)]
pub(crate) fn dump_table(path: &Path, table: &str) -> Vec<String> {
    let conn = Connection::open(path).unwrap();
    let mut stmt = conn
        .prepare(&format!("SELECT rowid, * FROM {table} ORDER BY rowid"))
        .unwrap();
    let columns = stmt.column_count();
    let rows = stmt
        .query_map([], |row| {
            let mut values = Vec::new();
            for i in 0..columns {
                values.push(format!("{:?}", row.get::<_, rusqlite::types::Value>(i)?));
            }
            Ok(values.join(","))
        })
        .unwrap();
    rows.map(Result::unwrap).collect()
}

/// The four tables that make up recovery state.
#[cfg(test)]
pub(crate) const RECOVERY_TABLES: [&str; 4] = [
    "trash_entries",
    "trash_nodes",
    "checkpoints",
    "checkpoint_nodes",
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A unique database path that is removed again on drop.
    struct TempDb(PathBuf);

    impl TempDb {
        fn new() -> Self {
            static COUNTER: AtomicU32 = AtomicU32::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let name = format!("omatree-test-{}-{n}.db", std::process::id());
            let path = std::env::temp_dir().join(name);
            let _ = std::fs::remove_file(&path);
            TempDb(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDb {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn round_trip(nb: &Notebook) -> Notebook {
        let db = TempDb::new();
        Storage::create(db.path()).unwrap().save(nb).unwrap();
        Storage::open(db.path()).unwrap().load().unwrap()
    }

    fn titles(nodes: Vec<&Node>) -> Vec<&str> {
        nodes.iter().map(|n| n.title()).collect()
    }

    /// Writes raw rows, bypassing the model and foreign keys.
    fn write_raw(path: &Path, sql: &str) {
        let conn = Connection::open(path).unwrap();
        conn.pragma_update(None, "foreign_keys", false).unwrap();
        conn.execute_batch(sql).unwrap();
    }

    #[test]
    fn creates_new_empty_database() {
        let db = TempDb::new();
        let storage = Storage::create(db.path()).unwrap();
        assert!(db.path().exists());
        assert_eq!(user_version(&storage.conn).unwrap(), 3);
        assert!(storage.load().unwrap().roots().is_empty());
    }

    #[test]
    fn saves_and_reopens_empty_notebook() {
        let loaded = round_trip(&Notebook::new());
        assert!(loaded.roots().is_empty());
    }

    #[test]
    fn round_trips_multiple_roots() {
        let mut nb = Notebook::new();
        nb.create_root("A").unwrap();
        nb.create_root("B").unwrap();
        let loaded = round_trip(&nb);
        assert_eq!(titles(loaded.roots()), ["A", "B"]);
    }

    #[test]
    fn round_trips_nested_children() {
        let mut nb = Notebook::new();
        let root = nb.create_root("Projects").unwrap();
        let child = nb.create_child(root, "OmaTree").unwrap();
        let grandchild = nb.create_child(child, "Ideas").unwrap();
        let loaded = round_trip(&nb);
        assert_eq!(loaded.get(child).unwrap().parent_id(), Some(root));
        assert_eq!(loaded.get(grandchild).unwrap().parent_id(), Some(child));
        assert_eq!(titles(loaded.children(root).unwrap()), ["OmaTree"]);
        assert_eq!(titles(loaded.children(child).unwrap()), ["Ideas"]);
    }

    #[test]
    fn preserves_root_and_child_ordering() {
        let mut nb = Notebook::new();
        let first = nb.create_root("first").unwrap();
        let second = nb.create_root("second").unwrap();
        nb.create_root("third").unwrap();
        for t in ["x", "y", "z"] {
            nb.create_child(second, t).unwrap();
        }
        nb.create_child(first, "only").unwrap();
        // Deleting a middle sibling must not disturb the saved order.
        let gone = nb.create_root("gone").unwrap();
        nb.delete(gone).unwrap();

        let loaded = round_trip(&nb);
        assert_eq!(titles(loaded.roots()), ["first", "second", "third"]);
        assert_eq!(titles(loaded.children(second).unwrap()), ["x", "y", "z"]);
    }

    #[test]
    fn preserves_titles_and_bodies() {
        let mut nb = Notebook::new();
        let id = nb.create_root("Tïtle \u{1F333}").unwrap();
        nb.set_body(id, "line one\n\nline três 'quoted' \"too\"\n")
            .unwrap();
        let loaded = round_trip(&nb);
        let node = loaded.get(id).unwrap();
        assert_eq!(node.title(), "Tïtle \u{1F333}");
        assert_eq!(node.body(), "line one\n\nline três 'quoted' \"too\"\n");
    }

    #[test]
    fn preserves_node_ids() {
        let mut nb = Notebook::new();
        let a = nb.create_root("a").unwrap();
        let b = nb.create_child(a, "b").unwrap();
        let c = nb.create_root("c").unwrap();
        // Ids with gaps must survive unchanged.
        nb.delete(b).unwrap();
        let d = nb.create_child(a, "d").unwrap();
        let loaded = round_trip(&nb);
        for id in [a, c, d] {
            assert_eq!(loaded.get(id).unwrap().id(), id);
        }
        assert!(loaded.get(b).is_none());
        assert_eq!(loaded.get(d).unwrap().title(), "d");
    }

    #[test]
    fn new_node_after_reload_does_not_reuse_ids() {
        let mut nb = Notebook::new();
        let a = nb.create_root("a").unwrap();
        let b = nb.create_root("b").unwrap();
        let mut loaded = round_trip(&nb);
        let c = loaded.create_root("c").unwrap();
        assert!(c > a && c > b);
        assert_eq!(loaded.get(a).unwrap().title(), "a");
    }

    #[test]
    fn reopens_existing_valid_database() {
        let db = TempDb::new();
        let mut nb = Notebook::new();
        let root = nb.create_root("root").unwrap();
        {
            let mut storage = Storage::create(db.path()).unwrap();
            storage.save(&nb).unwrap();
        }
        // Save again from a second session; contents are replaced.
        let mut storage = Storage::open(db.path()).unwrap();
        nb.create_child(root, "child").unwrap();
        storage.save(&nb).unwrap();
        drop(storage);

        let loaded = Storage::open(db.path()).unwrap().load().unwrap();
        assert_eq!(titles(loaded.children(root).unwrap()), ["child"]);
    }

    #[test]
    fn unsupported_schema_version_fails() {
        let db = TempDb::new();
        Storage::create(db.path()).unwrap();
        write_raw(db.path(), "PRAGMA user_version = 99;");
        match Storage::open(db.path()) {
            Err(StorageError::UnsupportedSchemaVersion(99)) => {}
            other => panic!("unexpected: {:?}", other.err()),
        }
    }

    #[test]
    fn open_does_not_create_missing_file() {
        let db = TempDb::new();
        assert!(matches!(
            Storage::open(db.path()),
            Err(StorageError::Sqlite(_))
        ));
        assert!(!db.path().exists());
    }

    #[test]
    fn create_refuses_existing_notebook() {
        let db = TempDb::new();
        Storage::create(db.path()).unwrap();
        assert!(Storage::create(db.path()).is_err());
    }

    #[test]
    fn invalid_parent_relationship_fails() {
        let db = TempDb::new();
        Storage::create(db.path()).unwrap();
        write_raw(
            db.path(),
            "INSERT INTO nodes VALUES (1, 42, 0, 'orphan', '');",
        );
        let storage = Storage::open(db.path()).unwrap();
        assert!(matches!(storage.load(), Err(StorageError::InvalidData(_))));
    }

    #[test]
    fn parent_cycle_fails() {
        let db = TempDb::new();
        Storage::create(db.path()).unwrap();
        write_raw(
            db.path(),
            "INSERT INTO nodes VALUES (1, 2, 0, 'a', '');
             INSERT INTO nodes VALUES (2, 1, 0, 'b', '');",
        );
        let storage = Storage::open(db.path()).unwrap();
        assert!(matches!(storage.load(), Err(StorageError::InvalidData(_))));
    }

    #[test]
    fn non_contiguous_positions_fail() {
        let db = TempDb::new();
        Storage::create(db.path()).unwrap();
        write_raw(
            db.path(),
            "INSERT INTO nodes VALUES (1, NULL, 0, 'a', '');
             INSERT INTO nodes VALUES (2, NULL, 5, 'b', '');",
        );
        let storage = Storage::open(db.path()).unwrap();
        assert!(matches!(storage.load(), Err(StorageError::InvalidData(_))));
    }

    #[test]
    fn negative_id_fails() {
        let db = TempDb::new();
        Storage::create(db.path()).unwrap();
        write_raw(
            db.path(),
            "INSERT INTO nodes VALUES (-1, NULL, 0, 'a', '');",
        );
        let storage = Storage::open(db.path()).unwrap();
        assert!(matches!(storage.load(), Err(StorageError::InvalidData(_))));
    }

    #[test]
    fn failed_save_keeps_previous_contents() {
        let db = TempDb::new();
        let mut storage = Storage::create(db.path()).unwrap();
        let mut good = Notebook::new();
        good.create_root("kept").unwrap();
        storage.save(&good).unwrap();

        // An id above i64::MAX cannot be stored; the save must roll back.
        let too_big = Notebook::from_nodes(vec![Node::from_parts(
            NodeId::from_raw(u64::MAX - 1),
            None,
            0,
            "big".into(),
            String::new(),
        )])
        .unwrap();
        assert!(matches!(
            storage.save(&too_big),
            Err(StorageError::InvalidData(_))
        ));
        assert_eq!(titles(storage.load().unwrap().roots()), ["kept"]);
    }

    // ---- schema version 2 and version 1 compatibility ----

    /// Writes a version 1 notebook directly, as an older OmaTree would have.
    fn write_v1_file(path: &Path) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(NODES_SCHEMA).unwrap();
        conn.execute_batch(
            "INSERT INTO nodes VALUES (3, NULL, 0, 'Projects', 'projects body');
             INSERT INTO nodes VALUES (7, NULL, 1, 'Inbox', 'inbox body');
             INSERT INTO nodes VALUES (9, 3, 0, 'OmaTree', 'child body');",
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
    }

    fn table_names(path: &Path) -> Vec<String> {
        let conn = Connection::open(path).unwrap();
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .unwrap();
        let rows = stmt.query_map([], |row| row.get::<_, String>(0)).unwrap();
        rows.map(Result::unwrap).collect()
    }

    fn file_version(path: &Path) -> i64 {
        user_version(&Connection::open(path).unwrap()).unwrap()
    }

    fn sample_state() -> (Notebook, Recovery) {
        let mut nb = Notebook::new();
        let projects = nb.create_root("Projects").unwrap();
        let ideas = nb.create_child(projects, "Ideas").unwrap();
        nb.set_body(ideas, "ideas \u{1F333} body").unwrap();
        nb.create_root("Inbox").unwrap();
        // Trash: a subtree whose original parent (id 50) is long gone.
        let trashed = TrashEntry::new(
            1_700_000_000,
            vec![
                Node::from_parts(
                    NodeId::from_raw(60),
                    Some(NodeId::from_raw(50)),
                    4,
                    "Old".into(),
                    "old body".into(),
                ),
                Node::from_parts(
                    NodeId::from_raw(61),
                    Some(NodeId::from_raw(60)),
                    0,
                    "Old child".into(),
                    "x".into(),
                ),
                Node::from_parts(
                    NodeId::from_raw(62),
                    Some(NodeId::from_raw(60)),
                    1,
                    "Old child 2".into(),
                    "y".into(),
                ),
            ],
        )
        .unwrap();
        let second = TrashEntry::new(
            1_700_000_100,
            vec![Node::from_parts(
                NodeId::from_raw(70),
                None,
                0,
                "Root once".into(),
                String::new(),
            )],
        )
        .unwrap();
        let checkpoints = vec![
            Checkpoint::of(&Notebook::new(), "Empty \"start\"", 100),
            Checkpoint::of(&nb, "Before deleting \"Projects\" \u{00e9}", 200),
        ];
        (nb, Recovery::from_parts(vec![trashed, second], checkpoints))
    }

    #[test]
    fn version_1_loads_with_empty_recovery_state_and_a_derived_id_counter() {
        let db = TempDb::new();
        write_v1_file(db.path());
        let storage = Storage::open(db.path()).unwrap();
        let (nb, recovery) = storage.load_document().unwrap();
        assert_eq!(titles(nb.roots()), ["Projects", "Inbox"]);
        assert_eq!(nb.get(NodeId::from_raw(9)).unwrap().body(), "child body");
        assert!(recovery.trash().is_empty());
        assert!(recovery.checkpoints().is_empty());
        assert_eq!(nb.next_id(), 10);
    }

    #[test]
    fn opening_and_loading_version_1_does_not_modify_the_file() {
        let db = TempDb::new();
        write_v1_file(db.path());
        let before = std::fs::read(db.path()).unwrap();
        {
            let storage = Storage::open(db.path()).unwrap();
            storage.load_document().unwrap();
        }
        assert_eq!(std::fs::read(db.path()).unwrap(), before);
        assert_eq!(file_version(db.path()), 1);
        assert!(!table_names(db.path()).contains(&"trash_entries".to_string()));
    }

    #[test]
    fn saving_version_1_migrates_it_to_version_3() {
        let db = TempDb::new();
        write_v1_file(db.path());
        let mut storage = Storage::open(db.path()).unwrap();
        let (mut nb, recovery) = storage.load_document().unwrap();
        let fresh = nb.create_root("Added after opening").unwrap();
        assert_eq!(fresh, NodeId::from_raw(10));
        storage.save_document(&nb, &recovery).unwrap();
        drop(storage);

        assert_eq!(file_version(db.path()), 3);
        let tables = table_names(db.path());
        for t in [
            "nodes",
            "notebook_meta",
            "trash_entries",
            "trash_nodes",
            "checkpoints",
            "checkpoint_nodes",
        ] {
            assert!(tables.contains(&t.to_string()), "missing table {t}");
        }
        let (loaded, _) = Storage::open(db.path()).unwrap().load_document().unwrap();
        assert_eq!(
            titles(loaded.roots()),
            ["Projects", "Inbox", "Added after opening"]
        );
        assert_eq!(
            loaded.get(NodeId::from_raw(9)).unwrap().body(),
            "child body"
        );
        assert_eq!(loaded.next_id(), 11);
    }

    #[test]
    fn a_failed_migration_leaves_the_version_1_file_valid_and_untouched() {
        let db = TempDb::new();
        write_v1_file(db.path());
        let before = std::fs::read(db.path()).unwrap();

        let mut storage = Storage::open(db.path()).unwrap();
        // An id above i64::MAX cannot be stored, so the save fails midway.
        let too_big = Notebook::from_nodes(vec![Node::from_parts(
            NodeId::from_raw(u64::MAX - 1),
            None,
            0,
            "big".into(),
            String::new(),
        )])
        .unwrap();
        assert!(matches!(
            storage.save_document(&too_big, &Recovery::default()),
            Err(StorageError::InvalidData(_))
        ));
        drop(storage);

        assert_eq!(std::fs::read(db.path()).unwrap(), before);
        assert_eq!(file_version(db.path()), 1);
        assert!(!table_names(db.path()).contains(&"notebook_meta".to_string()));
        let (nb, _) = Storage::open(db.path()).unwrap().load_document().unwrap();
        assert_eq!(titles(nb.roots()), ["Projects", "Inbox"]);
    }

    #[test]
    fn new_notebooks_are_created_as_version_3() {
        let db = TempDb::new();
        Storage::create(db.path()).unwrap();
        assert_eq!(file_version(db.path()), 3);
        let tables = table_names(db.path());
        assert!(tables.contains(&"checkpoint_nodes".to_string()));
        assert!(tables.contains(&"expanded_nodes".to_string()));
    }

    #[test]
    fn version_2_round_trips_the_active_notebook_trash_and_checkpoints() {
        let db = TempDb::new();
        let (nb, recovery) = sample_state();
        Storage::create(db.path())
            .unwrap()
            .save_document(&nb, &recovery)
            .unwrap();

        let (loaded, loaded_recovery) = Storage::open(db.path()).unwrap().load_document().unwrap();
        assert_eq!(loaded.snapshot_nodes(), nb.snapshot_nodes());
        // Trash: ids, titles, bodies, hierarchy, original parent/position, time.
        assert_eq!(loaded_recovery.trash(), recovery.trash());
        let entry = &loaded_recovery.trash()[0];
        assert_eq!(entry.root().id(), NodeId::from_raw(60));
        assert_eq!(entry.root().parent_id(), Some(NodeId::from_raw(50)));
        assert_eq!(entry.root().position(), 4);
        assert_eq!(entry.deleted_at(), 1_700_000_000);
        assert_eq!(entry.node_count(), 3);
        // Checkpoints: order, reasons, timestamps, trees.
        assert_eq!(loaded_recovery.checkpoints(), recovery.checkpoints());
        assert_eq!(loaded_recovery.checkpoints()[1].created_at(), 200);
        assert_eq!(
            loaded_recovery.checkpoints()[1].reason(),
            "Before deleting \"Projects\" \u{00e9}"
        );
    }

    #[test]
    fn version_2_persists_the_id_counter_beyond_surviving_nodes() {
        let db = TempDb::new();
        let mut nb = Notebook::new();
        nb.create_root("a").unwrap();
        nb.create_root("b").unwrap();
        let c = nb.create_root("c").unwrap();
        nb.delete(c).unwrap(); // highest id is gone from the active tree
        assert_eq!(nb.next_id(), 3);
        Storage::create(db.path())
            .unwrap()
            .save_document(&nb, &Recovery::default())
            .unwrap();

        let (mut loaded, _) = Storage::open(db.path()).unwrap().load_document().unwrap();
        assert_eq!(loaded.next_id(), 3);
        assert_eq!(loaded.create_root("d").unwrap(), NodeId::from_raw(3));
    }

    #[test]
    fn ids_that_only_exist_in_trash_or_checkpoints_are_never_reused() {
        let db = TempDb::new();
        let (nb, recovery) = sample_state(); // trash holds ids up to 70
        Storage::create(db.path())
            .unwrap()
            .save_document(&nb, &recovery)
            .unwrap();
        let (mut loaded, _) = Storage::open(db.path()).unwrap().load_document().unwrap();
        assert!(loaded.create_root("new").unwrap().get() > 70);

        // Even if the stored counter is lower than the ids in Trash.
        Connection::open(db.path())
            .unwrap()
            .execute("UPDATE notebook_meta SET next_node_id = 1", [])
            .unwrap();
        let (mut healed, _) = Storage::open(db.path()).unwrap().load_document().unwrap();
        assert!(healed.create_root("new").unwrap().get() > 70);
    }

    #[test]
    fn a_failed_version_2_save_changes_nothing() {
        let db = TempDb::new();
        let (nb, recovery) = sample_state();
        let mut storage = Storage::create(db.path()).unwrap();
        storage.save_document(&nb, &recovery).unwrap();
        let before = std::fs::read(db.path()).unwrap();

        // New active nodes and Trash would be fine; the checkpoint part
        // fails (id above i64::MAX) after they were already written.
        let mut changed = nb.clone();
        changed.create_root("brand new").unwrap();
        let huge = Notebook::from_nodes(vec![Node::from_parts(
            NodeId::from_raw(u64::MAX - 5),
            None,
            0,
            "huge".into(),
            String::new(),
        )])
        .unwrap();
        let mut bad = Recovery::default();
        bad.add_checkpoint(Checkpoint::of(&huge, "bad", 1));
        assert!(storage.save_document(&changed, &bad).is_err());
        drop(storage);

        assert_eq!(std::fs::read(db.path()).unwrap(), before);
        let (loaded, loaded_recovery) = Storage::open(db.path()).unwrap().load_document().unwrap();
        assert_eq!(loaded.snapshot_nodes(), nb.snapshot_nodes());
        assert_eq!(loaded_recovery.trash(), recovery.trash());
        assert_eq!(loaded_recovery.checkpoints(), recovery.checkpoints());
    }

    #[test]
    fn schema_versions_above_4_and_empty_files_stay_unsupported() {
        let db = TempDb::new();
        Storage::create(db.path()).unwrap();
        for version in [5, 99] {
            write_raw(db.path(), &format!("PRAGMA user_version = {version};"));
            match Storage::open(db.path()) {
                Err(StorageError::UnsupportedSchemaVersion(v)) => assert_eq!(v, version),
                other => panic!("unexpected: {:?}", other.err()),
            }
        }
        let empty = TempDb::new();
        Connection::open(empty.path()).unwrap();
        assert!(matches!(
            Storage::open(empty.path()),
            Err(StorageError::UnsupportedSchemaVersion(0))
        ));
    }

    #[test]
    fn corrupt_recovery_rows_fail_cleanly() {
        let db = TempDb::new();
        let (nb, recovery) = sample_state();
        Storage::create(db.path())
            .unwrap()
            .save_document(&nb, &recovery)
            .unwrap();
        // A trash node whose parent is not in its entry.
        write_raw(
            db.path(),
            "INSERT INTO trash_nodes VALUES (1, 999, 12345, 0, 'stray', '');",
        );
        let storage = Storage::open(db.path()).unwrap();
        assert!(matches!(
            storage.load_document(),
            Err(StorageError::InvalidData(_))
        ));
    }

    // ---- active-only saves ----

    fn recovery_dumps(path: &Path) -> Vec<Vec<String>> {
        RECOVERY_TABLES
            .iter()
            .map(|t| dump_table(path, t))
            .collect()
    }

    /// A version 2 file holding `sample_state()`, plus the state itself.
    fn populated_v2_file(db: &TempDb) -> (Notebook, Recovery) {
        let (nb, recovery) = sample_state();
        Storage::create(db.path())
            .unwrap()
            .save_document(&nb, &recovery)
            .unwrap();
        (nb, recovery)
    }

    #[test]
    fn active_save_persists_the_nodes_and_the_id_counter() {
        let db = TempDb::new();
        populated_v2_file(&db);
        // As the application does: work on the notebook loaded from the file.
        let (mut nb, _) = Storage::open(db.path()).unwrap().load_document().unwrap();
        let ids: Vec<NodeId> = nb.roots().iter().map(|n| n.id()).collect();
        nb.rename(ids[0], "Renamed").unwrap();
        nb.set_body(ids[1], "new body").unwrap();
        let fresh = nb.create_root("Fresh").unwrap();
        let doomed = nb.create_root("Doomed").unwrap();
        nb.delete(doomed).unwrap(); // the highest id is gone, the counter is not
        let counter = nb.next_id();
        assert!(counter > doomed.get());

        let mut storage = Storage::open(db.path()).unwrap();
        storage.save_active(&nb).unwrap();
        drop(storage);

        let (mut loaded, _) = Storage::open(db.path()).unwrap().load_document().unwrap();
        assert_eq!(loaded.snapshot_nodes(), nb.snapshot_nodes());
        assert_eq!(loaded.get(fresh).unwrap().title(), "Fresh");
        assert_eq!(loaded.next_id(), counter);
        assert!(loaded.create_root("next").unwrap().get() >= counter);
        let stored: i64 = Connection::open(db.path())
            .unwrap()
            .query_row("SELECT next_node_id FROM notebook_meta", [], |r| r.get(0))
            .unwrap();
        assert_eq!(stored as u64, counter);
    }

    #[test]
    fn active_save_never_lowers_the_stored_id_counter() {
        let db = TempDb::new();
        let (_, recovery) = populated_v2_file(&db); // counter is 71: Trash holds id 70
        let low = Notebook::new(); // a notebook whose own counter is 0
        let mut storage = Storage::open(db.path()).unwrap();
        storage.save_active(&low).unwrap();
        drop(storage);
        let (mut loaded, loaded_recovery) =
            Storage::open(db.path()).unwrap().load_document().unwrap();
        assert!(loaded.next_id() >= 71);
        assert!(loaded.create_root("x").unwrap().get() > 70);
        assert_eq!(loaded_recovery.trash(), recovery.trash());
    }

    #[test]
    fn active_save_never_writes_a_recovery_table() {
        let db = TempDb::new();
        let (mut nb, recovery) = populated_v2_file(&db);
        assert!(!recovery.trash().is_empty() && !recovery.checkpoints().is_empty());
        let before = recovery_dumps(db.path());
        assert!(before.iter().all(|rows| !rows.is_empty()));
        install_write_guards(db.path(), &RECOVERY_TABLES);

        nb.create_root("one more").unwrap();
        let mut storage = Storage::open(db.path()).unwrap();
        storage
            .save_active(&nb)
            .expect("no recovery table may be written");
        storage.save_active(&nb).expect("and again");
        drop(storage);

        assert_eq!(recovery_dumps(db.path()), before, "rows, rowids and all");
        let (loaded, loaded_recovery) = Storage::open(db.path()).unwrap().load_document().unwrap();
        assert_eq!(loaded.snapshot_nodes(), nb.snapshot_nodes());
        assert_eq!(loaded_recovery.trash(), recovery.trash());
        assert_eq!(loaded_recovery.checkpoints(), recovery.checkpoints());
    }

    #[test]
    fn the_write_guards_do_detect_a_full_save() {
        // Control for the test above: if the guards were ineffective it
        // would prove nothing. A full save rewrites recovery tables, so with
        // the guards installed it must fail, and roll back completely.
        let db = TempDb::new();
        let (nb, recovery) = populated_v2_file(&db);
        install_write_guards(db.path(), &RECOVERY_TABLES);
        let before = std::fs::read(db.path()).unwrap();

        let mut storage = Storage::open(db.path()).unwrap();
        let err = storage.save_document(&nb, &recovery).unwrap_err();
        assert!(err.to_string().contains("guarded table written"), "{err}");
        drop(storage);
        assert_eq!(std::fs::read(db.path()).unwrap(), before);
    }

    #[test]
    fn a_failed_active_save_rolls_back_and_leaves_the_previous_state_valid() {
        let db = TempDb::new();
        let (nb, recovery) = populated_v2_file(&db);
        // The nodes table is rewritten first; the metadata write then fails.
        install_write_guards(db.path(), &["notebook_meta"]);
        let before = std::fs::read(db.path()).unwrap();

        let mut changed = nb.clone();
        changed.create_root("never saved").unwrap();
        let mut storage = Storage::open(db.path()).unwrap();
        assert!(storage.save_active(&changed).is_err());
        drop(storage);

        assert_eq!(std::fs::read(db.path()).unwrap(), before);
        let (loaded, loaded_recovery) = Storage::open(db.path()).unwrap().load_document().unwrap();
        assert_eq!(loaded.snapshot_nodes(), nb.snapshot_nodes());
        assert_eq!(loaded_recovery.trash(), recovery.trash());
    }

    #[test]
    fn active_save_refuses_a_version_1_file_and_leaves_it_untouched() {
        let db = TempDb::new();
        write_v1_file(db.path());
        let before = std::fs::read(db.path()).unwrap();
        let mut storage = Storage::open(db.path()).unwrap();
        assert!(!storage.is_current_schema());
        let (nb, _) = storage.load_document().unwrap();
        assert!(matches!(
            storage.save_active(&nb),
            Err(StorageError::UnsupportedSchemaVersion(1))
        ));
        drop(storage);
        assert_eq!(std::fs::read(db.path()).unwrap(), before);
        assert_eq!(file_version(db.path()), 1);
    }
}
