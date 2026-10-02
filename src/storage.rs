//! SQLite persistence for notebooks. One notebook is one database file.

use std::fmt;
use std::path::Path;

use rusqlite::{params, Connection, OpenFlags};

use crate::notebook::{Node, NodeId, Notebook, NotebookError};

const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = "
    CREATE TABLE nodes (
        id        INTEGER PRIMARY KEY,
        parent_id INTEGER REFERENCES nodes(id) ON DELETE CASCADE,
        position  INTEGER NOT NULL CHECK (position >= 0),
        title     TEXT NOT NULL,
        body      TEXT NOT NULL
    );
    CREATE INDEX nodes_parent ON nodes(parent_id, position);
";

#[derive(Debug)]
pub enum StorageError {
    Sqlite(rusqlite::Error),
    UnsupportedSchemaVersion(i64),
    InvalidData(String),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageError::Sqlite(e) => write!(f, "database error: {e}"),
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
}

impl Storage {
    /// Creates the schema in a new (or empty) database file.
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
        tx.execute_batch(SCHEMA)?;
        tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        tx.commit()?;
        Ok(Storage { conn })
    }

    /// Opens an existing notebook; never creates a file.
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.pragma_update(None, "foreign_keys", true)?;

        let version = user_version(&conn)?;
        if version != SCHEMA_VERSION {
            return Err(StorageError::UnsupportedSchemaVersion(version));
        }
        Ok(Storage { conn })
    }

    /// Replaces the stored notebook with `notebook` in a single transaction.
    /// On any failure the previous contents remain.
    pub fn save(&mut self, notebook: &Notebook) -> Result<(), StorageError> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM nodes", [])?;
        {
            let mut insert = tx.prepare(
                "INSERT INTO nodes (id, parent_id, position, title, body)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            // Parents come before children, as the foreign key requires.
            for node in notebook.in_tree_order() {
                insert.execute(params![
                    id_to_sql(node.id())?,
                    node.parent_id().map(id_to_sql).transpose()?,
                    i64::try_from(node.position())
                        .map_err(|_| StorageError::InvalidData("position too large".into()))?,
                    node.title(),
                    node.body(),
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Loads and validates the stored notebook.
    pub fn load(&self) -> Result<Notebook, StorageError> {
        let mut query = self
            .conn
            .prepare("SELECT id, parent_id, position, title, body FROM nodes")?;
        let rows = query.query_map([], |row| {
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
        Ok(Notebook::from_nodes(nodes)?)
    }
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
        assert_eq!(user_version(&storage.conn).unwrap(), 1);
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
        nb.create_root("A");
        nb.create_root("B");
        let loaded = round_trip(&nb);
        assert_eq!(titles(loaded.roots()), ["A", "B"]);
    }

    #[test]
    fn round_trips_nested_children() {
        let mut nb = Notebook::new();
        let root = nb.create_root("Projects");
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
        let first = nb.create_root("first");
        let second = nb.create_root("second");
        nb.create_root("third");
        for t in ["x", "y", "z"] {
            nb.create_child(second, t).unwrap();
        }
        nb.create_child(first, "only").unwrap();
        // Deleting a middle sibling must not disturb the saved order.
        let gone = nb.create_root("gone");
        nb.delete(gone).unwrap();

        let loaded = round_trip(&nb);
        assert_eq!(titles(loaded.roots()), ["first", "second", "third"]);
        assert_eq!(titles(loaded.children(second).unwrap()), ["x", "y", "z"]);
    }

    #[test]
    fn preserves_titles_and_bodies() {
        let mut nb = Notebook::new();
        let id = nb.create_root("Tïtle \u{1F333}");
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
        let a = nb.create_root("a");
        let b = nb.create_child(a, "b").unwrap();
        let c = nb.create_root("c");
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
        let a = nb.create_root("a");
        let b = nb.create_root("b");
        let mut loaded = round_trip(&nb);
        let c = loaded.create_root("c");
        assert!(c > a && c > b);
        assert_eq!(loaded.get(a).unwrap().title(), "a");
    }

    #[test]
    fn reopens_existing_valid_database() {
        let db = TempDb::new();
        let mut nb = Notebook::new();
        let root = nb.create_root("root");
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
        good.create_root("kept");
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
}
