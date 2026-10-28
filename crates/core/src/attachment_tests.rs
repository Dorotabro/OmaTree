//! Attachment storage, schema version 4 and its migration (ticket E9a). Everything runs on
//! disposable files; the assertions read the SQLite file itself (header version, tables, rows,
//! BLOB bytes), not only what the API says.

use crate::attachments::fixtures::{jpeg, png};
use crate::attachments::{AttachmentError, MediaType, MAX_ATTACHMENT_BYTES};
use crate::document::{Document, DocumentError};
use crate::notebook::NodeId;
use crate::storage::{
    dump_table, install_write_guards, remove_write_guards, Storage, StorageError,
};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

pub(crate) struct Dir(PathBuf);

impl Dir {
    pub(crate) fn new() -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let d = std::env::temp_dir().join(format!(
            "omatree-att-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }

    pub(crate) fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        #[cfg(unix)]
        let _ =
            std::fs::set_permissions(&self.0, std::os::unix::fs::PermissionsExt::from_mode(0o755));
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(crate) fn raw(path: &Path) -> Connection {
    Connection::open(path).unwrap()
}

pub(crate) fn header_version(path: &Path) -> u32 {
    let b = std::fs::read(path).unwrap();
    u32::from_be_bytes([b[60], b[61], b[62], b[63]])
}

pub(crate) fn tables(path: &Path) -> Vec<String> {
    let c = raw(path);
    let mut q = c
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .unwrap();
    let rows = q.query_map([], |r| r.get::<_, String>(0)).unwrap();
    rows.map(Result::unwrap).collect()
}

/// (id, media_type, byte_len, data) of every stored attachment.
pub(crate) fn stored(path: &Path) -> Vec<(String, String, i64, Vec<u8>)> {
    let c = raw(path);
    let mut q = c
        .prepare("SELECT id, media_type, byte_len, data FROM attachments ORDER BY id")
        .unwrap();
    let rows = q
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap();
    rows.map(Result::unwrap).collect()
}

const CONTENT_TABLES: [&str; 7] = [
    "nodes",
    "notebook_meta",
    "trash_entries",
    "trash_nodes",
    "checkpoints",
    "checkpoint_nodes",
    "expanded_nodes",
];

/// Every content table, row by row.
pub(crate) fn logical(path: &Path) -> Vec<Vec<String>> {
    CONTENT_TABLES.iter().map(|t| dump_table(path, t)).collect()
}

fn ref_of(bytes: &[u8], doc: &Document) -> String {
    doc.put_attachment(bytes).unwrap()
}

/// A version 3 notebook: nested notes, expanded nodes, a Trash entry, checkpoints, Czech text and
/// emoji, and bodies that merely look like attachment references.
fn v3_fixture(path: &Path) -> Vec<NodeId> {
    let mut doc = Document::open(path).unwrap();
    let a = doc.create_root("Příliš žluťoučký").unwrap();
    let b = doc.create_child(a, "Kůň 🦀").unwrap();
    let c = doc.create_child(b, "Hluboko").unwrap();
    let d = doc.create_child(c, "Ještě hlouběji").unwrap();
    let e = doc.create_root("Smazat").unwrap();
    let f = doc.create_child(e, "Podřízená").unwrap();
    doc.set_body(a, "ahoj 🦀\n").unwrap();
    doc.set_body(
        d,
        "![x](assets/0123456789abcdef0123456789abcdef.png) not stored\n",
    )
    .unwrap();
    doc.set_body(f, "![y](assets/ffffffffffffffffffffffffffffffff.jpg)\n")
        .unwrap();
    doc.set_expanded(a, true);
    doc.set_expanded(b, true);
    doc.delete(e).unwrap(); // Trash entry and a checkpoint
    doc.create_checkpoint("by hand");
    doc.save().unwrap();
    assert_eq!(header_version(path), 3);
    vec![a, b, c, d]
}

// --- Pending attachments --------------------------------------------------------------------------

#[test]
fn an_untitled_notebook_holds_attachments_in_memory_and_reads_them_back() {
    let doc = Document::untitled();
    let png = png(3, 2);
    let r = doc.put_attachment(&png).unwrap();
    assert!(r.starts_with("assets/") && r.ends_with(".png"));
    assert_eq!(r.len(), "assets/".len() + 32 + ".png".len());
    assert_eq!(doc.attachment(&r).unwrap(), png, "readable before any save");
    let info = doc.attachment_info(&r).unwrap();
    assert_eq!(
        (info.len, info.media_type),
        (png.len() as u64, MediaType::Png)
    );
    assert_eq!(doc.pending_attachments(), (1, png.len() as u64));
    assert!(doc.is_dirty(), "unsaved until a save commits it");
    assert!(matches!(doc.clone_save_check(), Err(DocumentError::NoPath)));
    assert_eq!(doc.schema_version(), None);
}

impl Document {
    fn clone_save_check(&self) -> Result<(), DocumentError> {
        // Saving an untitled notebook in place is not possible; this only reports that.
        Err(DocumentError::NoPath)
    }
}

#[test]
fn png_and_jpeg_survive_save_and_reopen_byte_for_byte_with_the_right_types() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let (png, jpg) = (png(40, 30), jpeg());
    let mut doc = Document::open(&p).unwrap();
    let n = doc.create_root("Note").unwrap();
    let (rp, rj) = (ref_of(&png, &doc), ref_of(&jpg, &doc));
    doc.set_body(n, &format!("![a]({rp}) ![b]({rj})")).unwrap();
    assert_eq!(
        header_version(&p),
        3,
        "staging in memory does not touch the file"
    );
    doc.save().unwrap();
    assert_eq!(header_version(&p), 4);
    assert_eq!(doc.pending_attachments().0, 0);
    let rows = stored(&p);
    assert_eq!(rows.len(), 2);
    for (id, mime, len, data) in &rows {
        assert_eq!(id.len(), 32);
        assert!(id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
        assert_eq!(*len as usize, data.len());
        assert!(
            (mime == "image/png" && *data == png) || (mime == "image/jpeg" && *data == jpg),
            "{mime}"
        );
    }
    let again = Document::open_existing(&p).unwrap();
    assert_eq!(again.attachment(&rp).unwrap(), png);
    assert_eq!(again.attachment(&rj).unwrap(), jpg);
    assert_eq!(
        again.attachment_info(&rp).unwrap().media_type,
        MediaType::Png
    );
    assert_eq!(
        again.attachment_info(&rj).unwrap().media_type,
        MediaType::Jpeg
    );
    assert!(rp.ends_with(".png") && rj.ends_with(".jpg"));
}

#[test]
fn unsupported_and_damaged_images_are_refused_and_stage_nothing() {
    let doc = Document::untitled();
    for bytes in [
        &b""[..],
        b"GIF89a....",
        b"%PDF-1.7",
        &png(2, 2)[..20],
        &jpeg()[..6],
    ] {
        assert!(doc.put_attachment(bytes).is_err(), "{bytes:?}");
    }
    assert_eq!(doc.pending_attachments(), (0, 0));
    assert!(!doc.is_dirty());
    let mut big = vec![0u8; MAX_ATTACHMENT_BYTES as usize + 1];
    big[..8].copy_from_slice(&png(1, 1)[..8]);
    assert_eq!(
        doc.put_attachment(&big).unwrap_err(),
        AttachmentError::TooLarge(MAX_ATTACHMENT_BYTES + 1)
    );
    assert_eq!(doc.pending_attachments(), (0, 0));
}

#[test]
fn references_that_are_not_exactly_ours_are_invalid_and_unknown_ones_are_not_found() {
    let doc = Document::untitled();
    let r = doc.put_attachment(&png(1, 1)).unwrap();
    for bad in [
        "../x.png",
        "/etc/passwd",
        "assets/../x.png",
        "https://example.org/a.png",
        "assets/XYZ.png",
        "assets/0123456789ABCDEF0123456789ABCDEF.png",
        "",
    ] {
        assert!(
            matches!(
                doc.attachment(bad),
                Err(AttachmentError::InvalidReference(_))
            ),
            "{bad}"
        );
        assert!(matches!(
            doc.attachment_info(bad),
            Err(AttachmentError::InvalidReference(_))
        ));
    }
    assert!(matches!(
        doc.attachment("assets/0123456789abcdef0123456789abcdef.png"),
        Err(AttachmentError::NotFound(_))
    ));
    // The right id with the wrong extension is not the attachment.
    let wrong = r.replace(".png", ".jpg");
    assert!(matches!(
        doc.attachment(&wrong),
        Err(AttachmentError::NotFound(_))
    ));
}

#[test]
fn an_id_that_is_already_taken_is_never_reused() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    let mut draws = [[7u8; 16], [7u8; 16], [9u8; 16]].into_iter();
    let first = doc
        .put_attachment_with(&png(1, 1), &mut || Ok(draws.next().unwrap()))
        .unwrap();
    // Draws the pending id again, then a new one.
    let second = doc
        .put_attachment_with(&png(2, 2), &mut || Ok(draws.next().unwrap()))
        .unwrap();
    assert_ne!(first, second);
    assert_eq!(
        doc.attachment(&first).unwrap(),
        png(1, 1),
        "the first one was not replaced"
    );
    doc.save().unwrap();
    // Now the collision is with a stored one.
    let mut again = [[7u8; 16], [1u8; 16]].into_iter();
    let third = doc
        .put_attachment_with(&png(3, 3), &mut || Ok(again.next().unwrap()))
        .unwrap();
    assert!(third.contains("01010101"));
    assert_eq!(doc.attachment(&first).unwrap(), png(1, 1));
    // A source that only ever collides gives up with an error instead of overwriting.
    let r = doc.put_attachment_with(&png(4, 4), &mut || Ok([7u8; 16]));
    assert!(matches!(r, Err(AttachmentError::Storage(_))));
    // The table itself refuses a duplicate id.
    let c = raw(&p);
    let dup = c.execute(
        "INSERT INTO attachments (id, media_type, created_at, byte_len, data) VALUES (?1, 'image/png', 0, 1, x'00')",
        [first.trim_start_matches("assets/").trim_end_matches(".png")],
    );
    assert!(dup.is_err());
}

// --- Schema: lazy version 4 -----------------------------------------------------------------------

#[test]
fn text_only_work_never_upgrades_a_version_3_notebook() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let ids = v3_fixture(&p);
    let bytes = std::fs::read(&p).unwrap();
    // Opening writes nothing.
    let mut doc = Document::open_existing(&p).unwrap();
    assert_eq!(std::fs::read(&p).unwrap(), bytes);
    // Every kind of ordinary work, saved.
    doc.set_body(ids[0], "edited\n").unwrap();
    doc.save().unwrap();
    let n = doc.create_root("New").unwrap();
    doc.rename(n, "Renamed").unwrap();
    doc.move_node(n, Some(ids[1]), 0).unwrap();
    doc.save().unwrap();
    doc.delete(n).unwrap();
    doc.save().unwrap();
    doc.restore_trash(doc.recovery().trash().len() - 1).unwrap();
    doc.restore_checkpoint(0).unwrap();
    doc.save().unwrap();
    assert_eq!(header_version(&p), 3);
    assert!(!tables(&p).contains(&"attachments".to_owned()));
    assert_eq!(doc.schema_version(), Some(3));
}

#[test]
fn the_first_saved_attachment_upgrades_to_version_4_and_keeps_everything_else() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let ids = v3_fixture(&p);
    let before = logical(&p);
    let mut doc = Document::open_existing(&p).unwrap();
    let r = doc.put_attachment(&png(5, 5)).unwrap();
    assert_eq!(header_version(&p), 3, "only staged");
    doc.save().unwrap();
    assert_eq!(header_version(&p), 4);
    assert_eq!(doc.schema_version(), Some(4));
    assert!(tables(&p).contains(&"attachments".to_owned()));
    assert_eq!(
        logical(&p),
        before,
        "notes, Trash, checkpoints, expansion and counter are intact"
    );
    assert_eq!(stored(&p).len(), 1);
    // Reopen and edit normally (the active-only path on a version 4 file).
    let mut again = Document::open_existing(&p).unwrap();
    assert!(again.is_expanded(ids[0]) && again.is_expanded(ids[1]));
    again.set_body(ids[2], &format!("![z]({r})")).unwrap();
    again.save().unwrap();
    assert_eq!(header_version(&p), 4);
    assert_eq!(
        Document::open_existing(&p).unwrap().attachment(&r).unwrap(),
        png(5, 5)
    );
    assert_eq!(again.recovery().trash().len(), 1);
}

#[test]
fn version_1_and_2_notebooks_open_and_go_straight_to_4_with_their_first_attachment() {
    let d = Dir::new();
    let v1 = d.join("v1.omatree");
    {
        let c = raw(&v1);
        c.execute_batch(
            "CREATE TABLE nodes (id INTEGER PRIMARY KEY, parent_id INTEGER REFERENCES nodes(id) ON DELETE CASCADE,
                 position INTEGER NOT NULL CHECK (position >= 0), title TEXT NOT NULL, body TEXT NOT NULL);
             CREATE INDEX nodes_parent ON nodes(parent_id, position);
             INSERT INTO nodes VALUES (1, NULL, 0, 'Old', 'text');
             PRAGMA user_version = 1;",
        )
        .unwrap();
    }
    let mut doc = Document::open_existing(&v1).unwrap();
    assert_eq!(doc.notebook().roots()[0].title(), "Old");
    assert_eq!(doc.schema_version(), Some(1));
    doc.put_attachment(&jpeg()).unwrap();
    doc.save().unwrap();
    assert_eq!(header_version(&v1), 4);
    for t in [
        "notebook_meta",
        "trash_entries",
        "expanded_nodes",
        "attachments",
    ] {
        assert!(tables(&v1).contains(&t.to_owned()), "{t}");
    }
    // A version 2 file (recovery tables, no expanded_nodes).
    let v2 = d.join("v2.omatree");
    {
        let c = raw(&v2);
        c.execute_batch(
            "CREATE TABLE nodes (id INTEGER PRIMARY KEY, parent_id INTEGER REFERENCES nodes(id) ON DELETE CASCADE,
                 position INTEGER NOT NULL CHECK (position >= 0), title TEXT NOT NULL, body TEXT NOT NULL);
             CREATE TABLE notebook_meta (id INTEGER PRIMARY KEY CHECK (id = 1), next_node_id INTEGER NOT NULL);
             CREATE TABLE trash_entries (id INTEGER PRIMARY KEY, deleted_at INTEGER NOT NULL, root_node_id INTEGER NOT NULL);
             CREATE TABLE trash_nodes (entry_id INTEGER NOT NULL REFERENCES trash_entries(id) ON DELETE CASCADE,
                 node_id INTEGER NOT NULL, parent_id INTEGER, position INTEGER NOT NULL, title TEXT NOT NULL, body TEXT NOT NULL,
                 PRIMARY KEY (entry_id, node_id));
             CREATE TABLE checkpoints (id INTEGER PRIMARY KEY, created_at INTEGER NOT NULL, reason TEXT NOT NULL);
             CREATE TABLE checkpoint_nodes (checkpoint_id INTEGER NOT NULL REFERENCES checkpoints(id) ON DELETE CASCADE,
                 node_id INTEGER NOT NULL, parent_id INTEGER, position INTEGER NOT NULL, title TEXT NOT NULL, body TEXT NOT NULL,
                 PRIMARY KEY (checkpoint_id, node_id));
             INSERT INTO nodes VALUES (1, NULL, 0, 'Two', 'x');
             INSERT INTO notebook_meta VALUES (1, 2);
             PRAGMA user_version = 2;",
        )
        .unwrap();
    }
    let mut doc2 = Document::open_existing(&v2).unwrap();
    assert_eq!(doc2.schema_version(), Some(2));
    // A text save migrates to 3 only.
    doc2.set_body(NodeId::from_raw(1), "y").unwrap();
    doc2.save().unwrap();
    assert_eq!(header_version(&v2), 3);
    doc2.put_attachment(&png(1, 1)).unwrap();
    doc2.save().unwrap();
    assert_eq!(header_version(&v2), 4);
}

// --- One connection, one transaction ------------------------------------------------------------------------

fn v4_notebook(d: &Dir) -> (PathBuf, Document, String) {
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    let n = doc.create_root("Note").unwrap();
    let r = doc.put_attachment(&png(4, 4)).unwrap();
    doc.set_body(n, &format!("![a]({r})")).unwrap();
    doc.save().unwrap();
    (p, doc, r)
}

#[test]
fn a_failed_attachment_insert_rolls_back_the_whole_save_and_keeps_the_pending_bytes() {
    let d = Dir::new();
    let (p, mut doc, r) = v4_notebook(&d);
    let before = logical(&p);
    let stored_before = stored(&p);
    install_write_guards(&p, &["attachments"]);
    doc.resync_for_test();
    let n = doc.notebook().roots()[0].id();
    doc.set_body(n, "changed text").unwrap();
    let second = doc.put_attachment(&jpeg()).unwrap();
    let err = doc.save().unwrap_err();
    assert!(err.to_string().contains("guarded table written"), "{err}");
    assert_eq!(
        logical(&p),
        before,
        "the note change was rolled back with it"
    );
    assert_eq!(stored(&p), stored_before);
    assert!(doc.is_dirty());
    assert_eq!(doc.pending_attachments().0, 1);
    assert_eq!(
        doc.attachment(&second).unwrap(),
        jpeg(),
        "still readable from memory"
    );
    // The next save, once the cause is gone, goes through.
    remove_write_guards(&p);
    doc.resync_for_test();
    doc.save().unwrap();
    assert_eq!(stored(&p).len(), 2);
    assert_eq!(
        Document::open_existing(&p)
            .unwrap()
            .attachment(&second)
            .unwrap(),
        jpeg()
    );
    assert_eq!(
        Document::open_existing(&p).unwrap().attachment(&r).unwrap(),
        png(4, 4)
    );
}

#[test]
fn a_failed_node_write_rolls_back_the_attachment_inserts_and_the_upgrade() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    v3_fixture(&p);
    let before = logical(&p);
    install_write_guards(&p, &["nodes"]);
    let mut doc = Document::open_existing(&p).unwrap();
    let r = doc.put_attachment(&png(2, 2)).unwrap();
    let err = doc.save().unwrap_err();
    assert!(err.to_string().contains("guarded table written"));
    assert_eq!(header_version(&p), 3, "the upgrade rolled back");
    assert!(!tables(&p).contains(&"attachments".to_owned()));
    assert_eq!(logical(&p), before);
    assert_eq!(doc.pending_attachments().0, 1);
    assert_eq!(doc.attachment(&r).unwrap(), png(2, 2));
    // Same on a file that is already version 4: the insert that came first is undone.
    let d2 = Dir::new();
    let (p4, mut doc4, _) = v4_notebook(&d2);
    let rows = stored(&p4);
    install_write_guards(&p4, &["nodes"]);
    doc4.resync_for_test();
    doc4.put_attachment(&jpeg()).unwrap();
    assert!(doc4.save().is_err());
    assert_eq!(
        stored(&p4),
        rows,
        "no attachment row survived the failed save"
    );
    assert_eq!(doc4.pending_attachments().0, 1);
}

#[test]
fn a_failed_migration_leaves_the_old_schema_and_the_pending_attachment() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    v3_fixture(&p);
    let before = logical(&p);
    let mut doc = Document::open_existing(&p).unwrap();
    // Something already called "attachments" makes CREATE TABLE fail.
    raw(&p)
        .execute_batch("CREATE VIEW attachments AS SELECT 1 AS x;")
        .unwrap();
    doc.resync_for_test();
    let r = doc.put_attachment(&png(2, 2)).unwrap();
    doc.set_body(NodeId::from_raw(0), "text that must roll back too")
        .unwrap();
    assert!(doc.save().is_err());
    assert_eq!(header_version(&p), 3);
    assert_eq!(logical(&p), before);
    assert_eq!(doc.pending_attachments().0, 1);
    assert!(doc.is_dirty());
    raw(&p).execute_batch("DROP VIEW attachments;").unwrap();
    doc.resync_for_test();
    doc.save().unwrap();
    assert_eq!(header_version(&p), 4);
    assert_eq!(
        Document::open_existing(&p).unwrap().attachment(&r).unwrap(),
        png(2, 2)
    );
}

#[test]
fn an_external_change_is_detected_during_an_attachment_save_and_the_application_does_not_conflict_with_itself(
) {
    let d = Dir::new();
    let p = d.join("n.omatree");
    v3_fixture(&p);
    let mut doc = Document::open_existing(&p).unwrap();
    // Our own writes, several, never conflict.
    for i in 0..3u8 {
        doc.put_attachment(&png(u32::from(i) + 1, 1)).unwrap();
        doc.save().unwrap();
        doc.set_body(NodeId::from_raw(0), &format!("edit {i}"))
            .unwrap();
        doc.save().unwrap();
    }
    assert_eq!(stored(&p).len(), 3);
    // Someone else commits; the next attachment save is refused and nothing is written.
    {
        let mut other = Document::open_existing(&p).unwrap();
        other.set_body(NodeId::from_raw(0), "theirs").unwrap();
        other.save().unwrap();
    }
    let theirs = std::fs::read(&p).unwrap();
    doc.put_attachment(&jpeg()).unwrap();
    match doc.save() {
        Err(DocumentError::Storage(StorageError::ExternallyModified)) => {}
        other => panic!("{other:?}"),
    }
    assert_eq!(std::fs::read(&p).unwrap(), theirs);
    assert_eq!(doc.pending_attachments().0, 1, "kept for Save As");
    // And the same for a version 3 file that would have been upgraded.
    let q = d.join("m.omatree");
    v3_fixture(&q);
    let mut mine = Document::open_existing(&q).unwrap();
    {
        let mut other = Document::open_existing(&q).unwrap();
        other.set_body(NodeId::from_raw(0), "theirs").unwrap();
        other.save().unwrap();
    }
    mine.put_attachment(&png(1, 1)).unwrap();
    assert!(mine.save().is_err());
    assert_eq!(header_version(&q), 3);
}

// --- Save As -------------------------------------------------------------------------------------------------------

#[test]
fn save_as_carries_stored_and_pending_attachments_and_leaves_the_source_alone() {
    let d = Dir::new();
    let (p, mut doc, r1) = v4_notebook(&d);
    let r2 = doc.put_attachment(&jpeg()).unwrap(); // pending
    let source = std::fs::read(&p).unwrap();
    let target = d.join("copy.omatree");
    doc.save_as(&target, false).unwrap();
    assert_eq!(
        std::fs::read(&p).unwrap(),
        source,
        "the old file is untouched"
    );
    assert_eq!(header_version(&target), 4);
    assert_eq!(stored(&target).len(), 2);
    let copy = Document::open_existing(&target).unwrap();
    assert_eq!(copy.attachment(&r1).unwrap(), png(4, 4));
    assert_eq!(copy.attachment(&r2).unwrap(), jpeg());
    assert_eq!(doc.pending_attachments().0, 0);
    // The document now lives in the copy and still reads both.
    assert_eq!(doc.attachment(&r2).unwrap(), jpeg());
    assert!(!doc.is_dirty());
}

#[test]
fn save_as_from_untitled_with_only_pending_images_creates_a_version_4_file() {
    let d = Dir::new();
    let mut doc = Document::untitled();
    let n = doc.create_root("A").unwrap();
    let r = doc.put_attachment(&png(6, 6)).unwrap();
    doc.set_body(n, &format!("![]({r})")).unwrap();
    let target = d.join("fresh.omatree");
    doc.save_as(&target, false).unwrap();
    assert_eq!(header_version(&target), 4);
    assert_eq!(
        Document::open_existing(&target)
            .unwrap()
            .attachment(&r)
            .unwrap(),
        png(6, 6)
    );
    // Without images the new file stays version 3.
    let mut plain = Document::untitled();
    plain.create_root("B").unwrap();
    let t2 = d.join("plain.omatree");
    plain.save_as(&t2, false).unwrap();
    assert_eq!(header_version(&t2), 3);
}

#[test]
fn save_as_keeps_attachments_that_only_trash_or_a_checkpoint_references_and_orphans() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    let keep = doc.create_root("Keep").unwrap();
    let gone = doc.create_root("Gone").unwrap();
    let in_trash = doc.put_attachment(&png(2, 1)).unwrap();
    let orphan = doc.put_attachment(&png(3, 1)).unwrap(); // never referenced
    let in_active = doc.put_attachment(&jpeg()).unwrap();
    doc.set_body(gone, &format!("![]({in_trash})")).unwrap();
    doc.set_body(keep, &format!("![]({in_active})")).unwrap();
    doc.save().unwrap();
    doc.delete(gone).unwrap(); // now referenced only from Trash and checkpoints
    doc.save().unwrap();
    assert_eq!(stored(&p).len(), 3, "an ordinary save pruned nothing");
    let target = d.join("copy.omatree");
    doc.save_as(&target, false).unwrap();
    let copy = Document::open_existing(&target).unwrap();
    for r in [&in_trash, &orphan, &in_active] {
        assert!(copy.attachment(r).is_ok(), "{r}");
    }
    assert_eq!(stored(&target).len(), 3);
}

#[cfg(unix)]
#[test]
fn a_failed_save_as_leaves_source_destination_and_pending_data_safe() {
    let d = Dir::new();
    let (p, mut doc, _) = v4_notebook(&d);
    let pending = doc.put_attachment(&jpeg()).unwrap();
    let source = std::fs::read(&p).unwrap();
    // A new destination that cannot be written: nothing is left behind.
    let ro = d.join("ro");
    std::fs::create_dir(&ro).unwrap();
    std::fs::set_permissions(&ro, std::os::unix::fs::PermissionsExt::from_mode(0o555)).unwrap();
    assert!(doc.save_as(&ro.join("x.omatree"), false).is_err());
    std::fs::set_permissions(&ro, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    assert!(!ro.join("x.omatree").exists());
    // An existing notebook as the destination, failing half way: its old content survives.
    let other = d.join("other.omatree");
    let old_r = {
        let mut o = Document::open(&other).unwrap();
        let n = o.create_root("Theirs").unwrap();
        let r = o.put_attachment(&png(9, 9)).unwrap();
        o.set_body(n, &format!("![]({r})")).unwrap();
        o.save().unwrap();
        r
    };
    install_write_guards(&other, &["nodes"]);
    let theirs = std::fs::read(&other).unwrap();
    assert!(doc.save_as(&other, true).is_err());
    assert_eq!(
        std::fs::read(&other).unwrap(),
        theirs,
        "the destination is exactly as it was"
    );
    remove_write_guards(&other);
    assert_eq!(
        Document::open_existing(&other)
            .unwrap()
            .attachment(&old_r)
            .unwrap(),
        png(9, 9)
    );
    assert_eq!(std::fs::read(&p).unwrap(), source);
    assert_eq!(doc.pending_attachments().0, 1);
    assert_eq!(doc.attachment(&pending).unwrap(), jpeg());
    assert_eq!(doc.schema_version(), Some(4));
    assert!(doc.is_dirty());
}

#[test]
fn replacing_an_existing_notebook_replaces_its_attachments_too() {
    let d = Dir::new();
    let (_p, mut doc, r) = v4_notebook(&d);
    let other = d.join("other.omatree");
    let old = {
        let mut o = Document::open(&other).unwrap();
        o.create_root("Theirs").unwrap();
        let old = o.put_attachment(&png(9, 9)).unwrap();
        o.save().unwrap();
        old
    };
    doc.save_as(&other, true).unwrap();
    let after = Document::open_existing(&other).unwrap();
    assert!(after.attachment(&r).is_ok());
    assert!(
        matches!(after.attachment(&old), Err(AttachmentError::NotFound(_))),
        "the old file's image is not ours"
    );
    assert_eq!(stored(&other).len(), 1);
    // A file that is not a notebook is still never touched.
    let text = d.join("text.omatree");
    std::fs::write(&text, "precious").unwrap();
    assert!(matches!(
        doc.save_as(&text, true),
        Err(DocumentError::NotANotebook(_))
    ));
    assert_eq!(std::fs::read_to_string(&text).unwrap(), "precious");
}

// --- Recovery and no automatic deletion ----------------------------------------------------------------------

#[test]
fn deleting_restoring_and_checkpointing_never_remove_or_rewrite_attachments() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    let a = doc.create_root("A").unwrap();
    let r = doc.put_attachment(&png(7, 7)).unwrap();
    let body = format!("before ![shot]({r}) after\n");
    doc.set_body(a, &body).unwrap();
    doc.save().unwrap();
    let blob = stored(&p);
    doc.delete(a).unwrap();
    doc.save().unwrap();
    assert_eq!(stored(&p), blob);
    assert_eq!(
        doc.recovery().trash()[0].root().body(),
        body,
        "the reference is text, untouched"
    );
    doc.restore_trash(0).unwrap();
    doc.save().unwrap();
    assert_eq!(doc.notebook().get(a).unwrap().body(), body);
    assert_eq!(stored(&p), blob);
    // Checkpoints hold bodies only (no BLOBs): the table has no data column.
    assert!(raw(&p)
        .prepare("SELECT data FROM checkpoint_nodes")
        .is_err());
    doc.set_body(a, "rewritten, no image any more\n").unwrap();
    doc.create_checkpoint("with the image reference gone");
    doc.restore_checkpoint(0).unwrap();
    doc.save().unwrap();
    assert_eq!(
        stored(&p),
        blob,
        "an orphan stays until an explicit compaction exists"
    );
    assert_eq!(
        Document::open_existing(&p).unwrap().attachment(&r).unwrap(),
        png(7, 7)
    );
    // Many saves of every kind.
    for i in 0..5 {
        doc.set_body(a, &format!("v{i}")).unwrap();
        doc.save().unwrap();
        doc.create_checkpoint("x");
        doc.save().unwrap();
    }
    assert_eq!(stored(&p), blob);
}

#[test]
fn a_version_3_notebook_saved_by_this_build_stays_a_plain_version_3_file() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    v3_fixture(&p);
    let mut doc = Document::open_existing(&p).unwrap();
    doc.set_body(NodeId::from_raw(0), "Řeřicha 🦀\n").unwrap();
    doc.save().unwrap();
    assert_eq!(header_version(&p), 3);
    let c = raw(&p);
    let tables: i64 = c
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(tables, 7, "exactly the version 3 tables");
    assert_eq!(
        Document::open_existing(&p)
            .unwrap()
            .notebook()
            .get(NodeId::from_raw(0))
            .unwrap()
            .body(),
        "Řeřicha 🦀\n"
    );
}

#[test]
fn a_notebook_from_a_newer_schema_is_refused_without_a_byte_changing() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    v3_fixture(&p);
    raw(&p).execute_batch("PRAGMA user_version = 5;").unwrap();
    let bytes = std::fs::read(&p).unwrap();
    assert!(matches!(
        Storage::open(&p),
        Err(StorageError::UnsupportedSchemaVersion(5))
    ));
    assert!(Document::open_existing(&p).is_err());
    assert!(matches!(
        Document::open_existing(&p),
        Err(DocumentError::Storage(
            StorageError::UnsupportedSchemaVersion(5)
        ))
    ));
    let mut untitled = Document::untitled();
    untitled.create_root("x").unwrap();
    assert!(
        untitled.save_as(&p, true).is_err(),
        "not overwritten either"
    );
    assert_eq!(std::fs::read(&p).unwrap(), bytes);
}

/// Writes the compatibility fixtures for `tools/pre-e9-compat.sh` into `$OMATREE_COMPAT_DIR`:
/// `v3.omatree` (text only, saved by this build), `v4.omatree` (one stored attachment) and
/// `v4-pruned.omatree` (an orphan pruned and the space reclaimed, E9b).
/// `OMATREE_COMPAT_DIR=/some/dir cargo test -p omatree-core -- --ignored write_compat_fixtures`
#[test]
#[ignore]
fn write_compat_fixtures() {
    let dir =
        PathBuf::from(std::env::var_os("OMATREE_COMPAT_DIR").expect("set OMATREE_COMPAT_DIR"));
    std::fs::create_dir_all(&dir).unwrap();
    for name in ["v3.omatree", "v4.omatree", "v4-pruned.omatree"] {
        let _ = std::fs::remove_file(dir.join(name));
    }
    let v3 = dir.join("v3.omatree");
    v3_fixture(&v3);
    let mut doc = Document::open_existing(&v3).unwrap();
    doc.set_body(NodeId::from_raw(0), "saved again by the E9a build\n")
        .unwrap();
    doc.save().unwrap();
    assert_eq!(header_version(&v3), 3);
    let v4 = dir.join("v4.omatree");
    v3_fixture(&v4);
    let mut doc = Document::open_existing(&v4).unwrap();
    let r = doc.put_attachment(&png(8, 8)).unwrap();
    doc.set_body(NodeId::from_raw(0), &format!("![shot]({r})\n"))
        .unwrap();
    doc.save().unwrap();
    assert_eq!(header_version(&v4), 4);
    let pruned = dir.join("v4-pruned.omatree");
    v3_fixture(&pruned);
    let mut doc = Document::open_existing(&pruned).unwrap();
    let keep = doc.put_attachment(&png(8, 8)).unwrap();
    doc.put_attachment(&png(120, 120)).unwrap(); // never referenced
    doc.set_body(NodeId::from_raw(0), &format!("![shot]({keep})\n"))
        .unwrap();
    doc.save().unwrap();
    let pv = doc.preview_attachment_prune().unwrap();
    assert_eq!(doc.prune_attachments(&pv.token).unwrap().removed.count, 1);
    let rp = doc.preview_reclaim().unwrap();
    assert!(doc.reclaim_space(&rp.token).unwrap().ran);
    assert_eq!(header_version(&pruned), 4);
}

// --- E10: the budget for unsaved images --------------------------------------------------------

#[test]
fn unsaved_images_are_capped_in_total_and_a_refusal_changes_nothing() {
    use crate::attachments::fixtures::png_of_len;
    use crate::attachments::MAX_PENDING_BYTES;
    let d = Dir::new();
    let p = d.join("n.omatree");
    let doc = Document::open(&p).unwrap();
    let max = MAX_ATTACHMENT_BYTES as usize;
    assert_eq!(MAX_PENDING_BYTES, 2 * MAX_ATTACHMENT_BYTES);
    let big = png_of_len(max);
    let a = doc.put_attachment(&big).unwrap();
    let b = doc.put_attachment(&big).unwrap();
    assert_eq!(doc.pending_attachments(), (2, 2 * max as u64));
    // Exactly at the limit: even the smallest image is refused, with what is held reported.
    match doc.put_attachment(&png(1, 1)) {
        Err(AttachmentError::PendingFull(held)) => assert_eq!(held, 2 * max as u64),
        other => panic!("expected PendingFull, got {other:?}"),
    }
    let message = doc.put_attachment(&big).unwrap_err().to_string();
    assert!(
        message.contains("save the notebook first") && message.contains("128 MiB"),
        "{message}"
    );
    // Nothing was discarded or altered; both earlier images are still readable.
    assert_eq!(doc.pending_attachments(), (2, 2 * max as u64));
    assert_eq!(doc.attachment(&a).unwrap(), big);
    assert_eq!(doc.attachment(&b).unwrap(), big);
}

#[cfg(unix)]
#[test]
fn the_budget_is_released_by_a_committed_save_and_kept_by_a_failed_one() {
    use crate::attachments::fixtures::png_of_len;
    let d = Dir::new();
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    let big = png_of_len(MAX_ATTACHMENT_BYTES as usize);
    doc.put_attachment(&big).unwrap();
    let r = doc.put_attachment(&{
        let mut v = big.clone();
        v[40] = 1; // inside the padding: a distinct image of the same size
        v
    });
    assert!(r.is_ok());
    assert!(doc.put_attachment(&png(2, 2)).is_err(), "full");
    // A failed save (a read-only directory) keeps everything pending and still full.
    std::fs::set_permissions(
        d.path(),
        std::os::unix::fs::PermissionsExt::from_mode(0o555),
    )
    .unwrap();
    let res = doc.save();
    std::fs::set_permissions(
        d.path(),
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .unwrap();
    assert!(res.is_err());
    assert_eq!(doc.pending_attachments().0, 2);
    assert!(
        doc.put_attachment(&png(2, 2)).is_err(),
        "still full after the failed save"
    );
    // A committed save releases it.
    doc.save().unwrap();
    assert_eq!(doc.pending_attachments(), (0, 0));
    assert!(doc.put_attachment(&png(2, 2)).is_ok());
}

#[test]
fn many_small_images_up_to_the_limit_then_a_clean_refusal() {
    use crate::attachments::fixtures::png_of_len;
    let d = Dir::new();
    let doc = Document::open(&d.join("n.omatree")).unwrap();
    let chunk = 16 * 1024 * 1024;
    let mut accepted = 0u64;
    for i in 0..10u8 {
        let mut b = png_of_len(chunk);
        b[40] = i;
        match doc.put_attachment(&b) {
            Ok(_) => accepted += chunk as u64,
            Err(AttachmentError::PendingFull(held)) => {
                assert_eq!(held, accepted);
                break;
            }
            Err(e) => panic!("{e:?}"),
        }
    }
    assert_eq!(
        accepted,
        8 * chunk as u64,
        "exactly eight 16 MiB images fit in 128 MiB"
    );
    assert_eq!(doc.pending_attachments().1, accepted);
}
