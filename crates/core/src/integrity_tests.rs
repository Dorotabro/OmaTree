//! Attachment storage integrity under corruption, interruption and resource pressure (ticket
//! E9c). Disposable files only; every assertion that matters reads the SQLite file itself.

use crate::attachment_tests::{header_version, logical, raw, stored, tables, Dir};
use crate::attachments::fixtures::{jpeg, png};
use crate::attachments::AttachmentError;
use crate::document::{Document, DocumentError};
use crate::notebook::NodeId;
use std::path::Path;

pub(crate) fn id_of(reference: &str) -> String {
    reference["assets/".len()..][..32].to_owned()
}

/// A saved v4 notebook with one PNG note body referencing one stored image.
pub(crate) fn v4(path: &Path) -> (String, String, Vec<u8>) {
    let mut doc = Document::open(path).unwrap();
    let bytes = png(5, 4);
    let reference = doc.put_attachment(&bytes).unwrap();
    let n = doc.create_root("Příliš 🦀").unwrap();
    doc.set_body(n, &format!("![x]({reference})\n")).unwrap();
    doc.save().unwrap();
    assert_eq!(header_version(path), 4);
    let id = id_of(&reference);
    (reference, id, bytes)
}

/// A saved text-only v3 notebook; returns its two node ids (a root and its child).
pub(crate) fn v3(path: &Path) -> Vec<NodeId> {
    let mut doc = Document::open(path).unwrap();
    let a = doc.create_root("Kořen").unwrap();
    let b = doc.create_child(a, "Dítě").unwrap();
    doc.set_body(b, "text 🦀\n").unwrap();
    doc.set_expanded(a, true);
    doc.save().unwrap();
    assert_eq!(header_version(path), 3);
    vec![a, b]
}

fn message(e: &DocumentError) -> String {
    format!("{e:?}")
}

fn open_err(path: &Path) -> String {
    match Document::open(path) {
        Ok(_) => panic!("the notebook should have been refused"),
        Err(e) => message(&e),
    }
}

// --- 2. Malformed attachment data ---------------------------------------------------------------

/// Replaces the attachments table with `ddl` (rows copied by `rows`) in a v4 file.
fn swap_table(path: &Path, ddl: &str, rows: &str) {
    let c = raw(path);
    c.execute_batch(&format!(
        "PRAGMA ignore_check_constraints = ON;
         ALTER TABLE attachments RENAME TO old_attachments;
         {ddl};
         {rows};
         DROP TABLE old_attachments;"
    ))
    .unwrap();
}

#[test]
fn a_version_4_file_without_an_attachments_table_is_refused_and_untouched() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    v3(&p);
    raw(&p).pragma_update(None, "user_version", 4).unwrap();
    let before = std::fs::read(&p).unwrap();
    assert!(open_err(&p).contains("attachments table is missing"));
    assert_eq!(std::fs::read(&p).unwrap(), before);
}

#[test]
fn an_attachments_table_with_the_wrong_shape_is_refused_and_untouched() {
    let shapes = [
        // Missing column.
        "CREATE TABLE attachments (id TEXT PRIMARY KEY, media_type TEXT NOT NULL, created_at INTEGER NOT NULL, byte_len INTEGER NOT NULL)",
        // Wrong column type.
        "CREATE TABLE attachments (id TEXT PRIMARY KEY, media_type TEXT NOT NULL, created_at INTEGER NOT NULL, byte_len TEXT NOT NULL, data BLOB NOT NULL)",
        // No primary key.
        "CREATE TABLE attachments (id TEXT, media_type TEXT NOT NULL, created_at INTEGER NOT NULL, byte_len INTEGER NOT NULL, data BLOB NOT NULL)",
        // Nullable data.
        "CREATE TABLE attachments (id TEXT PRIMARY KEY, media_type TEXT NOT NULL, created_at INTEGER NOT NULL, byte_len INTEGER NOT NULL, data BLOB)",
        // Columns in another order.
        "CREATE TABLE attachments (media_type TEXT NOT NULL, id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, byte_len INTEGER NOT NULL, data BLOB NOT NULL)",
        // An extra column.
        "CREATE TABLE attachments (id TEXT PRIMARY KEY, media_type TEXT NOT NULL, created_at INTEGER NOT NULL, byte_len INTEGER NOT NULL, data BLOB NOT NULL, extra TEXT)",
    ];
    for ddl in shapes {
        let d = Dir::new();
        let p = d.join("n.omatree");
        v4(&p);
        swap_table(&p, ddl, "SELECT 1");
        let before = std::fs::read(&p).unwrap();
        assert!(
            open_err(&p).contains("expected columns or primary key"),
            "{ddl}"
        );
        assert_eq!(std::fs::read(&p).unwrap(), before, "{ddl}");
    }
}

#[test]
fn a_correct_table_without_check_constraints_is_still_validated_row_by_row() {
    // A table the program itself never writes (no CHECKs) is accepted only because every row is
    // checked on its own; the next tests give it bad rows.
    let d = Dir::new();
    let p = d.join("n.omatree");
    let (reference, _, bytes) = v4(&p);
    swap_table(
        &p,
        "CREATE TABLE attachments (id TEXT PRIMARY KEY, media_type TEXT NOT NULL, created_at INTEGER NOT NULL, byte_len INTEGER NOT NULL, data BLOB NOT NULL) WITHOUT ROWID",
        "INSERT INTO attachments SELECT * FROM old_attachments",
    );
    let doc = Document::open(&p).unwrap();
    assert_eq!(doc.attachment(&reference).unwrap(), bytes);
    assert!(doc.verify_integrity().unwrap().is_healthy());
}

/// Every way a row can be wrong without the table being: nothing may treat it as "unreferenced"
/// and delete it, Save As must not carry it into a new "healthy" notebook, and the explicit
/// check must name it.
#[test]
fn a_damaged_row_is_reported_never_pruned_and_never_copied_as_healthy() {
    let damages = [
        "byte_len = byte_len + 1",
        "byte_len = 0",
        "byte_len = -5",
        "media_type = 'image/gif'",
        "media_type = ''",
        "id = upper(id)",
        "id = substr(id, 1, 31)",
        "id = 'zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz'",
        "data = CAST('text instead of a blob' AS TEXT), byte_len = 23",
        "byte_len = '5'",
    ];
    for set in damages {
        let d = Dir::new();
        let p = d.join("n.omatree");
        v4(&p);
        // Also give the notebook an orphan so a prune would otherwise have something to do.
        {
            let doc = Document::open(&p).unwrap();
            doc.put_attachment(&jpeg()).unwrap();
        }
        let mut doc = Document::open(&p).unwrap();
        doc.put_attachment(&jpeg()).unwrap();
        doc.save().unwrap();
        drop(doc);
        let c = raw(&p);
        c.execute_batch("PRAGMA ignore_check_constraints = ON")
            .unwrap();
        c.execute(
            &format!(
                "UPDATE attachments SET {set} WHERE id = (SELECT id FROM attachments WHERE media_type = 'image/png')"
            ),
            [],
        )
        .unwrap();
        drop(c);
        let before = std::fs::read(&p).unwrap();
        let rows_before = stored_lossy(&p);

        let mut doc = match Document::open(&p) {
            Ok(doc) => doc,
            Err(e) => panic!("{set}: the notebook itself must stay openable: {e:?}"),
        };
        // The check names it.
        let report = doc.verify_integrity().unwrap();
        assert!(!report.is_healthy(), "{set}");
        assert!(!report.attachments.is_empty(), "{set}: {report:?}");
        // A prune is refused, not computed from partial knowledge.
        assert!(doc.preview_attachment_prune().is_err(), "{set}");
        // Save As to a new file fails and leaves nothing behind.
        let target = d.join("copy.omatree");
        assert!(doc.save_as(&target, false).is_err(), "{set}");
        assert!(!target.exists(), "{set}: a half-made copy was left");
        // Nothing was written anywhere.
        assert_eq!(std::fs::read(&p).unwrap(), before, "{set}");
        assert_eq!(stored_lossy(&p), rows_before, "{set}");
    }
}

/// `stored`, but tolerant of the odd column types the damages create.
fn stored_lossy(path: &Path) -> Vec<String> {
    let c = raw(path);
    let mut q = c
        .prepare("SELECT quote(id), quote(media_type), quote(byte_len), length(data) FROM attachments ORDER BY rowid")
        .unwrap_or_else(|_| {
            c.prepare("SELECT quote(id), quote(media_type), quote(byte_len), length(data) FROM attachments ORDER BY id")
                .unwrap()
        });
    let rows = q
        .query_map([], |r| {
            Ok(format!(
                "{}|{}|{}|{:?}",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<i64>>(3)?
            ))
        })
        .unwrap();
    rows.map(Result::unwrap).collect()
}

#[test]
fn truncated_and_malformed_images_are_detected_on_read_and_in_the_check() {
    let whole = png(6, 5);
    let jpg = jpeg();
    let cases: Vec<(&str, Vec<u8>, &str)> = vec![
        (
            "png cut in the middle",
            whole[..whole.len() / 2].to_vec(),
            "image/png",
        ),
        (
            "png without IEND",
            whole[..whole.len() - 12].to_vec(),
            "image/png",
        ),
        ("png signature only", whole[..8].to_vec(), "image/png"),
        (
            "jpeg without EOI",
            jpg[..jpg.len() - 2].to_vec(),
            "image/jpeg",
        ),
        ("jpeg SOI only", jpg[..3].to_vec(), "image/jpeg"),
        ("png bytes recorded as jpeg", whole.clone(), "image/jpeg"),
        ("jpeg bytes recorded as png", jpg.clone(), "image/png"),
        (
            "plain text recorded as png",
            b"not an image at all".to_vec(),
            "image/png",
        ),
    ];
    for (what, bytes, mime) in cases {
        let d = Dir::new();
        let p = d.join("n.omatree");
        let (reference, id, _) = v4(&p);
        {
            let c = raw(&p);
            c.execute(
                "UPDATE attachments SET data = ?1, byte_len = ?2, media_type = ?3",
                rusqlite::params![bytes, bytes.len() as i64, mime],
            )
            .unwrap();
        }
        let before = std::fs::read(&p).unwrap();
        let mut doc = Document::open(&p).unwrap();
        // The extension in the reference may disagree with the damaged record; use the right
        // spelling for whatever the row now claims.
        let ext = if mime == "image/png" { "png" } else { "jpg" };
        let claimed = format!("assets/{id}.{ext}");
        let _ = reference;
        match doc.attachment(&claimed) {
            Err(AttachmentError::Corrupt(_)) => {}
            other => panic!("{what}: expected Corrupt, got {other:?}"),
        }
        assert!(
            doc.attachment_info(&claimed).is_ok(),
            "{what}: info reads no bytes and sees a consistent row"
        );
        let report = doc.verify_integrity().unwrap();
        assert!(!report.attachments.is_empty(), "{what}");
        // Save As refuses to make a healthy-looking copy.
        let target = d.join("copy.omatree");
        assert!(doc.save_as(&target, false).is_err(), "{what}");
        assert!(!target.exists(), "{what}");
        // A damaged image that a note still references is never offered for pruning.
        if let Ok(preview) = doc.preview_attachment_prune() {
            assert!(
                preview.report.eligible.iter().all(|e| e.id.as_str() != id),
                "{what}"
            );
        }
        assert_eq!(std::fs::read(&p).unwrap(), before, "{what}");
    }
}

#[test]
fn an_image_that_is_structurally_valid_but_cannot_decode_is_a_display_concern() {
    // The decision (documented in the migration plan): core checks structure, which is cheap and
    // done once on the way in and again on every read; decoding pixels belongs to the E10
    // display boundary, which already has to cope with any image. Both sides of the contract:
    let mut garbled = png(8, 8);
    // Corrupt the compressed IDAT payload but keep every chunk header, length and the IEND.
    let idat = garbled.windows(4).position(|w| w == b"IDAT").unwrap();
    for b in &mut garbled[idat + 8..idat + 20] {
        *b ^= 0xA5;
    }
    let d = Dir::new();
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    // Core accepts it (structure is intact) ...
    let r = doc.put_attachment(&garbled).unwrap();
    doc.save().unwrap();
    // ... stores and returns it byte for byte ...
    let doc = Document::open(&p).unwrap();
    assert_eq!(doc.attachment(&r).unwrap(), garbled);
    assert!(doc.verify_integrity().unwrap().is_healthy());
    // ... and rejects what is not even structurally an image.
    assert!(doc.put_attachment(&garbled[..garbled.len() - 3]).is_err());
}

#[test]
fn a_reference_in_a_malformed_form_is_not_a_reference_and_resolves_to_nothing() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let (_, id, bytes) = v4(&p);
    let doc = Document::open(&p).unwrap();
    for bad in [
        format!("assets/{id}.PNG"),
        format!("assets/{id}.jpg"), // right id, wrong extension
        format!("assets/{id}.jpeg"),
        format!("Assets/{id}.png"),
        format!("assets/{}.png", id.to_uppercase()),
        format!("./assets/{id}.png"),
        format!("assets/../assets/{id}.png"),
        "file:///etc/passwd".to_string(),
        format!("https://example.com/{id}.png"),
        format!("assets/{id}.png?x=1"),
        format!("/home/user/{id}.png"),
        "assets/".to_owned(),
    ] {
        assert!(doc.attachment(&bad).is_err(), "{bad}");
        assert!(doc.attachment_info(&bad).is_err(), "{bad}");
    }
    assert_eq!(doc.attachment(&format!("assets/{id}.png")).unwrap(), bytes);
}

#[test]
fn unknown_future_versions_are_refused_without_touching_the_file() {
    for v in [5, 6, 99, 1_000_000] {
        let d = Dir::new();
        let p = d.join("n.omatree");
        v4(&p);
        raw(&p).pragma_update(None, "user_version", v).unwrap();
        let before = std::fs::read(&p).unwrap();
        assert!(open_err(&p).contains(&format!("UnsupportedSchemaVersion({v})")));
        assert_eq!(std::fs::read(&p).unwrap(), before);
    }
}

#[test]
fn a_notebook_whose_database_pages_are_damaged_is_reported_and_the_file_preserved() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    for i in 0..200 {
        let n = doc.create_root(&format!("Poznámka {i}")).unwrap();
        doc.set_body(n, &"žluťoučký kůň ".repeat(200)).unwrap();
    }
    doc.put_attachment(&png(40, 40)).unwrap();
    doc.save().unwrap();
    drop(doc);
    // Wreck the root page of the `nodes` table.
    let (root, page_size): (i64, i64) = {
        let c = raw(&p);
        (
            c.query_row(
                "SELECT rootpage FROM sqlite_master WHERE name = 'nodes'",
                [],
                |r| r.get(0),
            )
            .unwrap(),
            c.pragma_query_value(None, "page_size", |r| r.get(0))
                .unwrap(),
        )
    };
    let mut bytes = std::fs::read(&p).unwrap();
    let at = ((root - 1) * page_size) as usize;
    for b in &mut bytes[at..at + 16] {
        *b = 0xFF;
    }
    std::fs::write(&p, &bytes).unwrap();
    let before = std::fs::read(&p).unwrap();
    // Either opening refuses, or the explicit check reports; neither may succeed quietly.
    match Document::open(&p) {
        Err(e) => eprintln!("damaged page: open refused: {e:?}"),
        Ok(doc) => {
            let outcome = doc.verify_integrity();
            eprintln!("damaged page: open ok, check = {outcome:?}");
            assert!(
                outcome.is_err() || !outcome.unwrap().is_healthy(),
                "a wrecked page must never verify as healthy"
            );
        }
    }
    assert_eq!(std::fs::read(&p).unwrap(), before, "the file is preserved");
}

#[test]
fn a_healthy_notebook_verifies_clean_and_a_text_only_one_has_no_attachment_findings() {
    let d = Dir::new();
    let p4 = d.join("a.omatree");
    v4(&p4);
    let doc = Document::open(&p4).unwrap();
    let r = doc.verify_integrity().unwrap();
    assert!(r.is_healthy(), "{r:?}");
    assert_eq!(r.checked_attachments, 1);
    let p3 = d.join("b.omatree");
    v3(&p3);
    let doc = Document::open(&p3).unwrap();
    let r = doc.verify_integrity().unwrap();
    assert!(r.is_healthy());
    assert_eq!(r.checked_attachments, 0);
    assert_eq!(header_version(&p3), 3, "checking does not upgrade a file");
    let _ = (tables(&p3), logical(&p3), stored(&p4));
}

// --- 3. The first attachment migration under failure --------------------------------------------

use rusqlite::hooks::{AuthAction, AuthContext, Authorization, TransactionOperation};
use rusqlite::Connection;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

/// The points of a first-attachment save at which a failure (or a stop) is injected. Each is the
/// moment SQLite prepares (authorizer) or finishes (commit hook) the named step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stage {
    /// Before the attachments table exists.
    CreateTable,
    /// The table exists, no attachment row yet.
    InsertAttachments,
    /// Every attachment row is inserted; no note written yet.
    AfterAttachments,
    /// During the note writes (the old rows are deleted, none re-inserted).
    InsertNodes,
    /// During the expansion-state writes (notes are all written).
    InsertExpanded,
    /// Everything written, COMMIT not yet issued.
    BeforeCommit,
    /// Inside COMMIT (SQLite's commit hook).
    DuringCommit,
}

impl Stage {
    pub(crate) const ALL: [Stage; 7] = [
        Stage::CreateTable,
        Stage::InsertAttachments,
        Stage::AfterAttachments,
        Stage::InsertNodes,
        Stage::InsertExpanded,
        Stage::BeforeCommit,
        Stage::DuringCommit,
    ];

    fn name(self) -> &'static str {
        match self {
            Stage::CreateTable => "CreateTable",
            Stage::InsertAttachments => "InsertAttachments",
            Stage::AfterAttachments => "AfterAttachments",
            Stage::InsertNodes => "InsertNodes",
            Stage::InsertExpanded => "InsertExpanded",
            Stage::BeforeCommit => "BeforeCommit",
            Stage::DuringCommit => "DuringCommit",
        }
    }

    fn from_name(name: &str) -> Stage {
        *Stage::ALL.iter().find(|s| s.name() == name).unwrap()
    }

    fn hits(self, action: &AuthAction<'_>) -> bool {
        match (self, action) {
            (Stage::CreateTable, AuthAction::CreateTable { table_name }) => {
                *table_name == "attachments"
            }
            (Stage::InsertAttachments, AuthAction::Insert { table_name }) => {
                *table_name == "attachments"
            }
            (Stage::AfterAttachments, AuthAction::Delete { table_name }) => {
                *table_name == "expanded_nodes"
            }
            (Stage::InsertNodes, AuthAction::Insert { table_name }) => *table_name == "nodes",
            (Stage::InsertExpanded, AuthAction::Insert { table_name }) => {
                *table_name == "expanded_nodes"
            }
            (Stage::BeforeCommit, AuthAction::Transaction { operation }) => {
                // rusqlite names no variant for COMMIT; it arrives as `Unknown`.
                matches!(operation, TransactionOperation::Unknown)
            }
            _ => false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// The step fails with a SQLite error.
    Fail,
    /// The process prints a marker and waits there to be killed.
    Pause,
}

pub(crate) const SYNC: &str = "E9C-SYNC";

fn pause() -> ! {
    println!("{SYNC}");
    // Bounded, so an orphaned child can never outlive its test for long.
    std::thread::sleep(Duration::from_secs(120));
    std::process::exit(3)
}

pub(crate) fn arm(conn: &Connection, stage: Stage, mode: Mode) {
    if stage == Stage::DuringCommit {
        conn.commit_hook(Some(move || match mode {
            Mode::Fail => true,
            Mode::Pause => pause(),
        }))
        .unwrap();
        return;
    }
    conn.authorizer(Some(move |ctx: AuthContext<'_>| {
        if stage.hits(&ctx.action) {
            match mode {
                Mode::Fail => Authorization::Deny,
                Mode::Pause => pause(),
            }
        } else {
            Authorization::Allow
        }
    }))
    .unwrap();
}

pub(crate) fn disarm(conn: &Connection) {
    conn.authorizer(None::<fn(AuthContext<'_>) -> Authorization>)
        .unwrap();
    conn.commit_hook(None::<fn() -> bool>).unwrap();
}

/// What is in a notebook file, read with plain SQLite: version, tables, every content table,
/// every attachment (id, type, size, a hash of the bytes), the result of `integrity_check`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct State {
    version: u32,
    tables: Vec<String>,
    logical: Vec<Vec<String>>,
    attachments: Vec<(String, String, i64, Vec<u8>)>,
    integrity: String,
}

pub(crate) fn state(path: &Path) -> State {
    // Two runs of the same scenario differ by the clock (Trash, checkpoint and attachment
    // timestamps), so everything is read from a copy with those columns zeroed. The integrity
    // check runs on the real file.
    let integrity: String = raw(path)
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    let mut masked = path.as_os_str().to_owned();
    masked.push(".masked");
    let masked = PathBuf::from(masked);
    std::fs::copy(path, &masked).unwrap();
    let has_attachments = tables(&masked).iter().any(|t| t == "attachments");
    {
        let c = raw(&masked);
        c.execute_batch(
            "UPDATE checkpoints SET created_at = 0; UPDATE trash_entries SET deleted_at = 0;",
        )
        .ok();
        if has_attachments {
            c.execute_batch("UPDATE attachments SET created_at = 0")
                .ok();
        }
    }
    let out = State {
        version: header_version(&masked),
        tables: tables(&masked),
        logical: logical(&masked),
        attachments: if has_attachments {
            stored(&masked)
        } else {
            Vec::new()
        },
        integrity,
    };
    let _ = std::fs::remove_file(&masked);
    out
}

/// The first-attachment scenario: two images (fixed ids, so two runs are comparable), a new note
/// that uses them, expanded nodes, and (when `full`) a deleted note so the Trash and checkpoint
/// tables are rewritten too and the full-save path is the one taken.
pub(crate) fn scenario(doc: &mut Document, ids: &[NodeId], full: bool) -> (String, String) {
    let r1 = doc
        .put_attachment_with(&png(5, 4), &mut || Ok([1; 16]))
        .unwrap();
    let r2 = doc
        .put_attachment_with(&jpeg(), &mut || Ok([2; 16]))
        .unwrap();
    let n = doc.create_root("Nový 🦀").unwrap();
    doc.set_body(n, &format!("![a]({r1})\n![b]({r2})\n"))
        .unwrap();
    doc.set_expanded(n, true);
    if full {
        doc.delete(ids[1]).unwrap();
    }
    (r1, r2)
}

fn fixture_v3(dir: &Dir, name: &str) -> (std::path::PathBuf, Vec<NodeId>) {
    let p = dir.join(name);
    let ids = v3(&p);
    (p, ids)
}

/// The state a successful scenario save must leave.
fn expected_after(dir: &Dir, full: bool) -> State {
    let (p, ids) = fixture_v3(dir, "expected.omatree");
    let mut doc = Document::open(&p).unwrap();
    scenario(&mut doc, &ids, full);
    doc.save().unwrap();
    drop(doc);
    state(&p)
}

#[test]
fn a_failure_at_any_stage_of_the_first_attachment_save_changes_nothing_and_loses_nothing() {
    for full in [false, true] {
        let d = Dir::new();
        let after = expected_after(&d, full);
        assert_eq!(after.version, 4);
        for stage in Stage::ALL {
            let (p, ids) = fixture_v3(&d, &format!("{}-{full}.omatree", stage.name()));
            let before = state(&p);
            let mut doc = Document::open(&p).unwrap();
            let (r1, _) = scenario(&mut doc, &ids, full);
            let pending = doc.pending_attachments();
            assert_eq!(pending.0, 2);
            arm(
                doc.storage_for_test().unwrap().connection(),
                stage,
                Mode::Fail,
            );
            let outcome = doc.save();
            assert!(
                outcome.is_err(),
                "{stage:?}/{full}: the injected failure must surface"
            );
            // The file is exactly the valid v3 notebook it was: nothing partial.
            assert_eq!(state(&p), before, "{stage:?}/{full}");
            assert_eq!(header_version(&p), 3);
            // The process still holds everything.
            assert_eq!(doc.pending_attachments(), pending, "{stage:?}/{full}");
            assert!(doc.is_dirty(), "{stage:?}/{full}");
            assert_eq!(doc.attachment(&r1).unwrap(), png(5, 4));
            // Retrying once the fault is gone commits the complete state, with no false
            // "changed by someone else" conflict from the failed attempt.
            disarm(doc.storage_for_test().unwrap().connection());
            doc.save()
                .unwrap_or_else(|e| panic!("{stage:?}/{full}: retry failed: {e:?}"));
            assert_eq!(state(&p), after, "{stage:?}/{full}");
            assert_eq!(doc.pending_attachments().0, 0);
        }
    }
}

/// Runs the scenario in a disposable child process that stops at `stage` and is killed there.
fn kill_at(path: &Path, stage: Option<Stage>, full: bool) {
    struct Guard(Child);
    impl Drop for Guard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut child = Guard(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "integrity_tests::child_entry",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("E9C_PATH", path)
            .env("E9C_STAGE", stage.map_or("AfterCommit", Stage::name))
            .env("E9C_FULL", if full { "1" } else { "0" })
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let out = child.0.stdout.take().unwrap();
    let mut synced = false;
    for line in BufReader::new(out).lines() {
        // libtest prints "test name ... " first, on the same line.
        if line.unwrap().contains(SYNC) {
            synced = true;
            break;
        }
    }
    assert!(synced, "the child ended without reaching {stage:?}");
    // Killed with SIGKILL while stopped at the synchronisation point: no clean-up code runs.
    child.0.kill().unwrap();
    child.0.wait().unwrap();
}

/// The child half of `kill_at`. A normal run of the suite skips it (no environment).
#[test]
fn child_entry() {
    let (Ok(path), Ok(stage), Ok(full)) = (
        std::env::var("E9C_PATH"),
        std::env::var("E9C_STAGE"),
        std::env::var("E9C_FULL"),
    ) else {
        return;
    };
    let path = std::path::PathBuf::from(path);
    let full = full == "1";
    // The ids of the fixture's nodes: a root and its child, ids 0 and 1.
    let ids = vec![NodeId::from_raw(0), NodeId::from_raw(1)];
    let mut doc = Document::open(&path).unwrap();
    scenario(&mut doc, &ids, full);
    if stage != "AfterCommit" {
        arm(
            doc.storage_for_test().unwrap().connection(),
            Stage::from_name(&stage),
            Mode::Pause,
        );
    }
    doc.save().unwrap();
    // Everything is committed and the call has returned.
    pause()
}

#[test]
fn a_process_killed_at_any_stage_leaves_the_old_notebook_or_the_whole_new_one() {
    for full in [false, true] {
        let d = Dir::new();
        let after = expected_after(&d, full);
        let mut stages: Vec<Option<Stage>> = Stage::ALL.iter().copied().map(Some).collect();
        stages.push(None); // after the commit
        for stage in stages {
            let (p, _) = fixture_v3(&d, &format!("{stage:?}-{full}.omatree"));
            let before = state(&p);
            kill_at(&p, stage, full);
            // Reopening runs SQLite's own recovery of a hot journal. The result is one of the
            // two valid states, never a mixture.
            let reopened = Document::open(&p);
            assert!(
                reopened.is_ok(),
                "{stage:?}/{full}: the notebook must reopen"
            );
            drop(reopened);
            let now = state(&p);
            assert_eq!(now.integrity, "ok", "{stage:?}/{full}");
            if stage.is_none() {
                assert_eq!(
                    now, after,
                    "{stage:?}/{full}: a commit must survive the kill"
                );
            } else {
                assert_eq!(
                    now, before,
                    "{stage:?}/{full}: nothing may be half-committed"
                );
            }
        }
    }
}

// --- 4. Disk-full and resource exhaustion -------------------------------------------------------
//
// Real ENOSPC: the parent re-runs `enospc_child` inside `unshare -Urm` (an unprivileged user and
// mount namespace) with a small tmpfs mounted over the notebook directory, so the filesystem the
// notebook lives on is genuinely full, and nothing outside the throw-away namespace is touched.
// Where `unshare` is not permitted the test says so and returns; that scenario is then unverified.

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

/// Fills the filesystem holding `dir` until a write fails, then frees `leave` bytes again.
fn fill(dir: &Path, leave: u64) -> PathBuf {
    let p = dir.join("filler");
    let mut f = File::create(&p).unwrap();
    let chunk = vec![0u8; 4096];
    while f.write_all(&chunk).is_ok() {}
    let len = f.metadata().unwrap().len();
    f.set_len(len.saturating_sub(leave)).unwrap();
    p
}

/// A large, structurally valid PNG of about 1 MB.
fn big_png(n: u32) -> Vec<u8> {
    png(500, 500 + n)
}

/// Checks the file after an operation that may have failed for lack of space: it opens, its
/// integrity check is clean, and it is exactly one of the two states offered.
fn assert_one_of(p: &Path, a: &State, b: &State, what: &str) {
    Document::open(p).unwrap_or_else(|e| panic!("{what}: notebook unreadable: {e:?}"));
    let now = state(p);
    assert_eq!(now.integrity, "ok", "{what}");
    assert!(
        &now == a || &now == b,
        "{what}: neither the old nor the new state"
    );
}

/// The child half. Prints one line per case and panics on the first violation.
#[test]
fn enospc_child() {
    let Ok(dir) = std::env::var("E9C_ENOSPC_DIR") else {
        return;
    };
    let dir = PathBuf::from(dir);
    let mut outcomes = Vec::new();
    for leave in [0u64, 4096, 64 * 1024, 300_000, 900_000, 1_400_000] {
        let reset = || {
            for e in std::fs::read_dir(&dir).unwrap() {
                let _ = std::fs::remove_file(e.unwrap().path());
            }
        };
        let note = |what: &str, failed: bool, outcomes: &mut Vec<String>| {
            outcomes.push(format!(
                "{what} with {leave} bytes free: {}",
                if failed {
                    "failed cleanly"
                } else {
                    "committed"
                }
            ));
        };

        // First attachment migration (v3 -> v4).
        reset();
        let p = dir.join("m.omatree");
        v3(&p);
        let before = state(&p);
        let mut doc = Document::open(&p).unwrap();
        let r = doc.put_attachment(&big_png(0)).unwrap();
        let n = doc.create_root("Nový").unwrap();
        doc.set_body(n, &format!("![x]({r})\n")).unwrap();
        let filler = fill(&dir, leave);
        let res = doc.save();
        std::fs::remove_file(&filler).unwrap();
        if res.is_err() {
            assert_eq!(state(&p), before, "migration {leave}: partial state");
            assert_eq!(
                doc.pending_attachments().0,
                1,
                "migration {leave}: pending lost"
            );
            assert!(doc.is_dirty());
            doc.save().unwrap();
        }
        note("migration", res.is_err(), &mut outcomes);
        drop(doc);
        let st = state(&p);
        assert_eq!(
            (st.version, st.attachments.len(), st.integrity.as_str()),
            (4, 1, "ok")
        );
        assert_eq!(st.attachments[0].3, big_png(0), "exact bytes");

        // An ordinary save adding a second attachment to a v4 notebook.
        reset();
        let p = dir.join("o.omatree");
        v4(&p);
        let before = state(&p);
        let mut doc = Document::open(&p).unwrap();
        let r = doc.put_attachment(&big_png(1)).unwrap();
        let n = doc.create_root("Další").unwrap();
        doc.set_body(n, &format!("![x]({r})\n")).unwrap();
        let filler = fill(&dir, leave);
        let res = doc.save();
        std::fs::remove_file(&filler).unwrap();
        if res.is_err() {
            assert_eq!(state(&p), before, "ordinary {leave}");
            assert_eq!(doc.pending_attachments().0, 1);
            doc.save().unwrap();
        }
        note("ordinary save", res.is_err(), &mut outcomes);
        drop(doc);
        assert_eq!(state(&p).attachments.len(), 2);

        // Save As: a new target, and an existing one being replaced.
        reset();
        let src = Dir::new();
        let sp = src.join("s.omatree");
        let (r, _, bytes) = v4(&sp);
        let mut doc = Document::open(&sp).unwrap();
        doc.put_attachment(&big_png(2)).unwrap();
        let target = dir.join("t.omatree");
        let filler = fill(&dir, leave);
        let res = doc.save_as(&target, false);
        std::fs::remove_file(&filler).unwrap();
        if res.is_err() {
            assert!(
                !target.exists(),
                "save as {leave}: a broken target was left"
            );
            assert!(doc.is_dirty() && doc.pending_attachments().0 == 1);
            assert!(doc.is_current_path(&sp));
            doc.save_as(&target, false).unwrap();
        }
        note("save as (new)", res.is_err(), &mut outcomes);
        assert_eq!(doc.attachment(&r).unwrap(), bytes);
        drop(doc);
        assert_eq!(state(&target).attachments.len(), 2);
        // Replacing an existing notebook.
        let existing = dir.join("e.omatree");
        v3(&existing);
        let before = state(&existing);
        let mut doc = Document::open(&sp).unwrap();
        let filler = fill(&dir, leave);
        let res = doc.save_as(&existing, true);
        std::fs::remove_file(&filler).unwrap();
        if res.is_err() {
            assert_eq!(
                state(&existing),
                before,
                "replace {leave}: the old target must stay whole"
            );
        }
        note("save as (replace)", res.is_err(), &mut outcomes);
        assert_one_of(&existing, &before, &state(&existing), "replace");

        // Pruning, then (separately) space reclamation.
        reset();
        let p = dir.join("p.omatree");
        let mut doc = Document::open(&p).unwrap();
        let keep = doc.put_attachment(&big_png(3)).unwrap();
        let n = doc.create_root("Drží").unwrap();
        doc.set_body(n, &format!("![x]({keep})\n")).unwrap();
        for i in 4..7 {
            doc.put_attachment(&big_png(i)).unwrap();
        }
        doc.save().unwrap();
        let before = state(&p);
        let preview = doc.preview_attachment_prune().unwrap();
        assert_eq!(preview.report.eligible.len(), 3);
        let filler = fill(&dir, leave);
        let res = doc.prune_attachments(&preview.token);
        std::fs::remove_file(&filler).unwrap();
        let pruned = match res {
            Err(_) => {
                assert_eq!(state(&p), before, "prune {leave}: partial deletion");
                // The notebook is as inspectable and recoverable as before.
                let again = doc.preview_attachment_prune().unwrap();
                assert_eq!(again.report.eligible.len(), 3);
                doc.prune_attachments(&again.token).unwrap();
                false
            }
            Ok(r) => {
                assert_eq!(r.removed.count, 3);
                true
            }
        };
        outcomes.push(format!(
            "prune with {leave} bytes free: {}",
            if pruned {
                "committed"
            } else {
                "failed cleanly, then succeeded"
            }
        ));
        assert_eq!(state(&p).attachments.len(), 1);
        assert_eq!(doc.attachment(&keep).unwrap(), big_png(3));

        let reclaim = doc.preview_reclaim().unwrap();
        let after_prune = state(&p);
        let filler = fill(&dir, leave);
        let res = doc.reclaim_space(&reclaim.token);
        std::fs::remove_file(&filler).unwrap();
        if let Err(e) = &res {
            {
                assert_eq!(
                    state(&p),
                    after_prune,
                    "vacuum {leave}: the notebook changed"
                );
                // A failed VACUUM must neither lose the baseline nor hide a write.
                let n2 = doc.create_root("Po neúspěchu").unwrap();
                doc.set_body(n2, "ok\n").unwrap();
                doc.save()
                    .unwrap_or_else(|e2| panic!("vacuum {leave}: {e:?} then save: {e2:?}"));
                let again = doc.preview_reclaim().unwrap();
                doc.reclaim_space(&again.token).unwrap();
            }
        }
        outcomes.push(format!(
            "vacuum with {leave} bytes free: {}",
            if res.is_err() {
                "failed cleanly, then succeeded"
            } else {
                "committed"
            }
        ));
        assert_eq!(doc.attachment(&keep).unwrap(), big_png(3));
        drop(doc);
        let st = state(&p);
        assert_eq!((st.attachments.len(), st.integrity.as_str()), (1, "ok"));
    }
    println!("ENOSPC-RESULT\n{}\nENOSPC-END", outcomes.join("\n"));
}

#[test]
fn a_really_full_filesystem_never_leaves_a_partial_or_unreadable_notebook() {
    let d = Dir::new();
    let mount = d.join("small");
    std::fs::create_dir(&mount).unwrap();
    let probe = Command::new("unshare").args(["-Urm", "true"]).status();
    if !probe.is_ok_and(|s| s.success()) {
        eprintln!("SKIPPED: unprivileged mount namespaces are unavailable; real ENOSPC is UNVERIFIED here");
        return;
    }
    let exe = std::env::current_exe().unwrap();
    let out = Command::new("unshare")
        .args(["-Urm", "sh", "-c"])
        .arg(r#"mount -t tmpfs -o size=8m tmpfs "$E9C_ENOSPC_DIR" && exec "$0" --exact integrity_tests::enospc_child --nocapture --test-threads=1"#)
        .arg(&exe)
        .env("E9C_ENOSPC_DIR", &mount)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "child failed:\n{text}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report = text
        .split("ENOSPC-RESULT")
        .nth(1)
        .and_then(|t| t.split("ENOSPC-END").next())
        .unwrap_or("(no result)");
    eprintln!("real ENOSPC results:{report}");
    assert!(
        report.contains("failed cleanly"),
        "no case actually ran out of space:{report}"
    );
}

// --- 5. VACUUM and the external-modification baseline -------------------------------------------

use crate::document::ReclaimError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Journal {
    Delete,
    Wal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fault {
    /// VACUUM runs unhindered.
    None,
    /// Another connection holds the write lock, so VACUUM cannot start (SQLITE_BUSY).
    Busy,
    /// VACUUM is made to fail part-way, after it started rewriting.
    Aborts,
}

/// Another connection commits at the moment the first internal statement of VACUUM is prepared
/// (the closest a test can get to "during" a statement that SQLite runs as one), and then
/// optionally makes VACUUM fail.
fn competitor_during_vacuum(doc: &Document, path: &Path, then_fail: bool) {
    let path = path.to_path_buf();
    let mut done = false;
    doc.storage_for_test()
        .unwrap()
        .connection()
        .authorizer(Some(move |ctx: AuthContext<'_>| {
            if !done && matches!(ctx.action, AuthAction::Attach { .. }) {
                done = true;
                let c = raw(&path);
                c.busy_timeout(Duration::ZERO).unwrap();
                c.execute_batch(
                    "CREATE TABLE IF NOT EXISTS competitor (x TEXT);
                     INSERT INTO competitor VALUES ('written during vacuum');",
                )
                .expect("the competing commit must be able to land while VACUUM prepares");
                return Authorization::Allow;
            }
            if then_fail && done && matches!(ctx.action, AuthAction::CreateTable { .. }) {
                return Authorization::Deny;
            }
            Authorization::Allow
        }))
        .unwrap();
}

/// A saved notebook with several free pages, in the given journal mode.
fn with_free_pages(path: &Path, journal: Journal) -> Document {
    if journal == Journal::Wal {
        // Journal mode WAL is a property of the file; set it before the Document opens it.
        let mut doc = Document::open(path).unwrap();
        doc.create_root("x").unwrap();
        doc.save().unwrap();
        drop(doc);
        let c = raw(path);
        let mode: String = c
            .pragma_update_and_check(None, "journal_mode", "WAL", |r| r.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
    }
    let mut doc = Document::open(path).unwrap();
    let keep = doc.put_attachment(&big_png(10)).unwrap();
    let n = doc.create_root("Drží").unwrap();
    doc.set_body(n, &format!("![k]({keep})\n")).unwrap();
    for i in 11..14 {
        doc.put_attachment(&big_png(i)).unwrap();
    }
    doc.save().unwrap();
    let p = doc.preview_attachment_prune().unwrap();
    doc.prune_attachments(&p.token).unwrap();
    assert!(doc.preview_reclaim().unwrap().free_bytes > 0);
    doc
}

fn competitor_rows(path: &Path) -> i64 {
    raw(path)
        .query_row("SELECT count(*) FROM competitor", [], |r| r.get(0))
        .unwrap_or(0)
}

/// What one combination did: (reclaim succeeded, the next ordinary save succeeded).
fn vacuum_case(journal: Journal, fault: Fault, external: Option<bool>) -> (bool, bool) {
    // `external`: None = nobody else writes; Some(false) = a competing commit lands before the
    // reclamation starts; Some(true) = one lands while VACUUM is being prepared.
    let d = Dir::new();
    let p = d.join("v.omatree");
    let mut doc = with_free_pages(&p, journal);
    let token = doc.preview_reclaim().unwrap().token;
    let mut blocker: Option<Connection> = None;
    if external == Some(false) {
        let c = raw(&p);
        c.execute_batch(
            "CREATE TABLE competitor (x TEXT); INSERT INTO competitor VALUES ('before');",
        )
        .unwrap();
    }
    // The token is derived from the file, which the competitor changed: refused as stale or as a
    // conflict, before anything is rewritten.
    let result;
    match (fault, external) {
        (Fault::Busy, _) => {
            let c = raw(&p);
            c.execute_batch("BEGIN IMMEDIATE").unwrap();
            blocker = Some(c);
            result = doc.reclaim_space_waiting(&token, Duration::ZERO);
        }
        (Fault::Aborts, Some(true)) => {
            competitor_during_vacuum(&doc, &p, true);
            result = doc.reclaim_space_waiting(&token, Duration::ZERO);
        }
        (Fault::Aborts, _) => {
            competitor_during_vacuum_none(&doc, true);
            result = doc.reclaim_space_waiting(&token, Duration::ZERO);
        }
        (Fault::None, Some(true)) => {
            competitor_during_vacuum(&doc, &p, false);
            result = doc.reclaim_space_waiting(&token, Duration::ZERO);
        }
        (Fault::None, _) => result = doc.reclaim_space_waiting(&token, Duration::ZERO),
    }
    if let Some(c) = blocker.take() {
        c.execute_batch("ROLLBACK").unwrap();
    }
    disarm(doc.storage_for_test().unwrap().connection());
    let ok = result.is_ok();
    if external == Some(false) {
        assert!(
            matches!(result, Err(ReclaimError::Stale) | Err(ReclaimError::Storage(_))),
            "{journal:?}/{fault:?}: reclamation after an external write must be refused, got {result:?}"
        );
    }
    // The file is valid, and a competing commit, if any, is still there.
    assert_eq!(
        state(&p).integrity,
        "ok",
        "{journal:?}/{fault:?}/{external:?}"
    );
    if external.is_some() {
        assert_eq!(
            competitor_rows(&p),
            1,
            "{journal:?}/{fault:?}/{external:?}: a write was lost"
        );
    }
    // The next ordinary save.
    let n = doc.create_root("Po").unwrap();
    doc.set_body(n, "text\n").unwrap();
    let saved = doc.save();
    if external.is_some() {
        // Someone else wrote: the save must be refused, never silently overwrite.
        assert!(
            matches!(&saved, Err(e) if e.is_conflict()),
            "{journal:?}/{fault:?}/{external:?}: save after an external commit: {saved:?}"
        );
        assert!(doc.is_dirty(), "the refused edit is still pending");
        assert_eq!(competitor_rows(&p), 1);
    }
    let (observed, now) = doc.storage_for_test().unwrap().baseline();
    eprintln!(
        "vacuum {journal:?}/{fault:?}/external={external:?}: baseline {observed} vs data_version {now} after the attempt; reclaim {} then ordinary save {}",
        if ok { "succeeded" } else { "failed" },
        if saved.is_ok() { "succeeded" } else { "was refused" }
    );
    (ok, saved.is_ok())
}

/// Fails VACUUM part-way without any competitor.
fn competitor_during_vacuum_none(doc: &Document, _then_fail: bool) {
    let mut seen = false;
    doc.storage_for_test()
        .unwrap()
        .connection()
        .authorizer(Some(move |ctx: AuthContext<'_>| {
            if matches!(ctx.action, AuthAction::Attach { .. }) {
                seen = true;
            }
            if seen && matches!(ctx.action, AuthAction::CreateTable { .. }) {
                return Authorization::Deny;
            }
            Authorization::Allow
        }))
        .unwrap();
}

#[test]
fn rollback_journal_mode_vacuum_matrix() {
    // Nobody else writes: success and every kind of failure leave the next save working.
    for fault in [Fault::None, Fault::Busy, Fault::Aborts] {
        let (reclaimed, saved) = vacuum_case(Journal::Delete, fault, None);
        assert_eq!(reclaimed, fault == Fault::None, "{fault:?}");
        assert!(
            saved,
            "{fault:?}: a failed VACUUM must not cause a false conflict"
        );
    }
    // Somebody else committed before the reclamation, or while it was starting: always a conflict.
    for fault in [Fault::None, Fault::Busy, Fault::Aborts] {
        vacuum_case(Journal::Delete, fault, Some(false));
        if fault != Fault::Busy {
            vacuum_case(Journal::Delete, fault, Some(true));
        }
    }
}

#[test]
fn wal_mode_vacuum_matrix() {
    // In WAL mode the header counter proves nothing, so a *failed* VACUUM may leave the
    // baseline refused (a false conflict, the safe side); a successful one and a busy refusal
    // must not. A competing commit is never accepted either way.
    let (reclaimed, saved) = vacuum_case(Journal::Wal, Fault::None, None);
    assert!(
        reclaimed && saved,
        "a successful VACUUM leaves the baseline valid"
    );
    let (reclaimed, saved) = vacuum_case(Journal::Wal, Fault::Busy, None);
    assert!(!reclaimed && saved, "a busy refusal changes nothing");
    let (reclaimed, _saved) = vacuum_case(Journal::Wal, Fault::Aborts, None);
    assert!(!reclaimed);
    for fault in [Fault::None, Fault::Busy, Fault::Aborts] {
        vacuum_case(Journal::Wal, fault, Some(false));
        if fault != Fault::Busy {
            vacuum_case(Journal::Wal, fault, Some(true));
        }
    }
}

fn dv(c: &Connection) -> i64 {
    c.pragma_query_value(None, "data_version", |r| r.get(0))
        .unwrap()
}

#[cfg(unix)]
/// The raw SQLite behaviour behind the baseline logic, on a plain connection with no OmaTree
/// code in between: how `PRAGMA data_version` moves across successful and failed `VACUUM`s,
/// per journal mode. Printed as evidence; asserted only where the baseline logic relies on it.
#[test]
fn data_version_across_successful_and_failed_vacuum_as_sqlite_reports_it() {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    for journal in [Journal::Delete, Journal::Wal] {
        let mut seen = Vec::new();
        for how in ["success", "authorizer abort", "busy", "read-only directory"] {
            let d = Dir::new();
            let p = d.join("v.omatree");
            let c = raw(&p);
            if journal == Journal::Wal {
                let _: String = c
                    .pragma_update_and_check(None, "journal_mode", "WAL", |r| r.get(0))
                    .unwrap();
            }
            c.execute_batch(
                "CREATE TABLE t (b BLOB); INSERT INTO t VALUES (zeroblob(200000));
                 INSERT INTO t VALUES (zeroblob(200000)); DELETE FROM t;",
            )
            .unwrap();
            let before = dv(&c);
            let result = match how {
                "success" => c.execute_batch("VACUUM"),
                "authorizer abort" => {
                    let mut attached = false;
                    c.authorizer(Some(move |ctx: AuthContext<'_>| {
                        attached |= matches!(ctx.action, AuthAction::Attach { .. });
                        if attached && matches!(ctx.action, AuthAction::CreateTable { .. }) {
                            Authorization::Deny
                        } else {
                            Authorization::Allow
                        }
                    }))
                    .unwrap();
                    c.execute_batch("VACUUM")
                }
                "busy" => {
                    let other = raw(&p);
                    other.execute_batch("BEGIN IMMEDIATE").unwrap();
                    c.busy_timeout(Duration::ZERO).unwrap();
                    c.execute_batch("VACUUM")
                }
                _ => {
                    std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o555))
                        .unwrap();
                    c.execute_batch("VACUUM")
                }
            };
            let after = dv(&c);
            seen.push(format!(
                "{how}: {} data_version {before} -> {after}",
                if result.is_ok() {
                    "VACUUM ok,"
                } else {
                    "VACUUM failed,"
                }
            ));
            if how == "success" {
                assert_eq!(
                    before, after,
                    "{journal:?}: a successful VACUUM does not move the baseline"
                );
                assert!(result.is_ok());
            }
            if how == "busy" {
                assert!(result.is_err());
                assert_eq!(
                    before, after,
                    "{journal:?}: a refused VACUUM does not move it"
                );
            }
        }
        eprintln!("raw SQLite, {journal:?} journal:\n  {}", seen.join("\n  "));
    }
}

// --- 6. Pruning: interruption and authorization -------------------------------------------------

use crate::document::PruneError;

/// A saved notebook: one note using `keep`, and three orphans (`o1`..`o3`) of different sizes.
struct Pn {
    keep: String,
    orphans: Vec<String>,
    keep_note: NodeId,
}

fn prune_fixture(path: &Path) -> (Document, Pn) {
    let mut doc = Document::open(path).unwrap();
    let keep = doc.put_attachment(&png(9, 9)).unwrap();
    let n = doc.create_root("Drží").unwrap();
    doc.set_body(n, &format!("![k]({keep})\n")).unwrap();
    let orphans = [png(20, 20), png(30, 30), png(40, 40)]
        .iter()
        .map(|b| id_of(&doc.put_attachment(b).unwrap()))
        .collect();
    doc.save().unwrap();
    let pn = Pn {
        keep: id_of(&keep),
        orphans,
        keep_note: n,
    };
    (doc, pn)
}

fn rows_of(path: &Path) -> Vec<String> {
    stored(path).into_iter().map(|r| r.0).collect()
}

/// The point of a prune where a failure or a stop is injected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PruneStage {
    BeforeFirstDelete,
    BetweenDeletes,
    BeforeCommit,
    DuringCommit,
}

impl PruneStage {
    const ALL: [PruneStage; 4] = [
        PruneStage::BeforeFirstDelete,
        PruneStage::BetweenDeletes,
        PruneStage::BeforeCommit,
        PruneStage::DuringCommit,
    ];

    fn name(self) -> &'static str {
        match self {
            PruneStage::BeforeFirstDelete => "BeforeFirstDelete",
            PruneStage::BetweenDeletes => "BetweenDeletes",
            PruneStage::BeforeCommit => "BeforeCommit",
            PruneStage::DuringCommit => "DuringCommit",
        }
    }

    fn from_name(n: &str) -> PruneStage {
        *PruneStage::ALL.iter().find(|s| s.name() == n).unwrap()
    }
}

pub(crate) fn arm_prune(conn: &Connection, stage: PruneStage, mode: Mode) {
    if stage == PruneStage::DuringCommit {
        conn.commit_hook(Some(move || match mode {
            Mode::Fail => true,
            Mode::Pause => pause(),
        }))
        .unwrap();
        return;
    }
    let mut deletes = 0;
    conn.authorizer(Some(move |ctx: AuthContext<'_>| {
        let hit = match (stage, &ctx.action) {
            (
                PruneStage::BeforeFirstDelete | PruneStage::BetweenDeletes,
                AuthAction::Delete { table_name },
            ) if *table_name == "attachments" => {
                deletes += 1;
                deletes
                    == if stage == PruneStage::BeforeFirstDelete {
                        1
                    } else {
                        2
                    }
            }
            (PruneStage::BeforeCommit, AuthAction::Transaction { operation }) => {
                matches!(operation, TransactionOperation::Unknown)
            }
            _ => false,
        };
        match (hit, mode) {
            (false, _) => Authorization::Allow,
            (true, Mode::Fail) => Authorization::Deny,
            (true, Mode::Pause) => pause(),
        }
    }))
    .unwrap();
}

#[test]
fn a_failure_during_a_prune_commits_nothing_and_the_prune_can_be_redone() {
    for stage in PruneStage::ALL {
        let d = Dir::new();
        let p = d.join("n.omatree");
        let (mut doc, pn) = prune_fixture(&p);
        let before = state(&p);
        let preview = doc.preview_attachment_prune().unwrap();
        assert_eq!(preview.report.eligible.len(), 3);
        arm_prune(
            doc.storage_for_test().unwrap().connection(),
            stage,
            Mode::Fail,
        );
        let res = doc.prune_attachments(&preview.token);
        assert!(res.is_err(), "{stage:?}: the failure must surface");
        assert_eq!(state(&p), before, "{stage:?}: not one row may be gone");
        assert!(!doc.is_dirty());
        // The notebook is still inspectable and the prune can be done again from a new preview.
        disarm(doc.storage_for_test().unwrap().connection());
        let again = doc.preview_attachment_prune().unwrap();
        assert_eq!(again.report.eligible.len(), 3, "{stage:?}");
        let done = doc.prune_attachments(&again.token).unwrap();
        assert_eq!(done.removed.count, 3);
        assert_eq!(rows_of(&p), vec![pn.keep.clone()], "{stage:?}");
        assert_eq!(state(&p).integrity, "ok");
    }
}

fn kill_prune_at(path: &Path, stage: Option<PruneStage>) {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "integrity_tests::child_prune_entry",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("E9C_PATH", path)
        .env("E9C_STAGE", stage.map_or("AfterCommit", PruneStage::name))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut synced = false;
    for line in BufReader::new(child.stdout.take().unwrap()).lines() {
        if line.unwrap().contains(SYNC) {
            synced = true;
            break;
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    assert!(synced, "the child ended without reaching {stage:?}");
}

#[test]
fn child_prune_entry() {
    let (Ok(path), Ok(stage)) = (std::env::var("E9C_PATH"), std::env::var("E9C_STAGE")) else {
        return;
    };
    // Only the child of a prune kill sets E9C_STAGE to a prune stage or AfterCommit; the first
    // attachment child uses the same variables, so this entry must ignore its names.
    if stage != "AfterCommit" && !PruneStage::ALL.iter().any(|s| s.name() == stage) {
        return;
    }
    if std::env::var("E9C_FULL").is_ok() {
        return;
    }
    let mut doc = Document::open(Path::new(&path)).unwrap();
    let preview = doc.preview_attachment_prune().unwrap();
    if stage != "AfterCommit" {
        arm_prune(
            doc.storage_for_test().unwrap().connection(),
            PruneStage::from_name(&stage),
            Mode::Pause,
        );
    }
    doc.prune_attachments(&preview.token).unwrap();
    pause()
}

#[test]
fn a_process_killed_during_a_prune_leaves_every_row_or_none_of_the_deletions() {
    let mut stages: Vec<Option<PruneStage>> = PruneStage::ALL.iter().copied().map(Some).collect();
    stages.push(None);
    for stage in stages {
        let d = Dir::new();
        let p = d.join("n.omatree");
        let (doc, pn) = prune_fixture(&p);
        drop(doc);
        let before = state(&p);
        kill_prune_at(&p, stage);
        assert!(Document::open(&p).is_ok(), "{stage:?}: must reopen");
        let now = state(&p);
        assert_eq!(now.integrity, "ok");
        if stage.is_none() {
            assert_eq!(
                rows_of(&p),
                vec![pn.keep.clone()],
                "a committed prune survives the kill"
            );
            // Only attachment rows differ: notes, Trash and checkpoints are exactly as before.
            assert_eq!(now.logical, before.logical);
        } else {
            assert_eq!(now, before, "{stage:?}: a partial deletion survived a kill");
        }
    }
}

/// Every change that can alter what a prune would delete invalidates the preview, and the
/// refused prune deletes nothing.
#[test]
fn every_change_after_a_preview_invalidates_it_and_deletes_nothing() {
    type Change = Box<dyn Fn(&mut Document, &Pn, &Path)>;
    let changes: Vec<(&str, Change)> = vec![
        (
            "an attachment is added and saved",
            Box::new(|d, _, _| {
                d.put_attachment(&png(11, 11)).unwrap();
                d.save().unwrap();
            }),
        ),
        (
            "an attachment is added and left pending",
            Box::new(|d, _, _| {
                d.put_attachment(&png(12, 12)).unwrap();
            }),
        ),
        (
            "a note starts referencing an orphan",
            Box::new(|d, pn, _| {
                let r = format!("assets/{}.png", pn.orphans[0]);
                d.set_body(pn.keep_note, &format!("![o]({r})\n")).unwrap();
            }),
        ),
        (
            "a note referencing an orphan is saved and moved to Trash",
            Box::new(|d, pn, _| {
                let r = format!("assets/{}.png", pn.orphans[0]);
                let n = d.create_root("Nový").unwrap();
                d.set_body(n, &format!("![o]({r})\n")).unwrap();
                d.save().unwrap();
                d.delete(n).unwrap();
                d.save().unwrap();
            }),
        ),
        (
            "a checkpoint is created",
            Box::new(|d, _, _| {
                d.create_checkpoint("po náhledu");
            }),
        ),
        (
            "the checkpoint list is trimmed",
            Box::new(|d, _, _| {
                for i in 0..(crate::recovery::MAX_CHECKPOINTS + 3) {
                    d.create_checkpoint(&format!("c{i}"));
                }
            }),
        ),
        (
            "another connection inserts an attachment row",
            Box::new(|_, _, p| {
                let id = "ab".repeat(16);
                raw(p)
                    .execute(
                        "INSERT INTO attachments VALUES (?1, 'image/png', 0, 3, x'010203')",
                        [id],
                    )
                    .unwrap();
            }),
        ),
        (
            "another connection deletes an attachment row",
            Box::new(|_, pn, p| {
                raw(p)
                    .execute("DELETE FROM attachments WHERE id = ?1", [&pn.orphans[2]])
                    .unwrap();
            }),
        ),
        (
            "another connection rewrites an attachment's bytes",
            Box::new(|_, pn, p| {
                raw(p)
                    .execute(
                        "UPDATE attachments SET data = zeroblob(10), byte_len = 10 WHERE id = ?1",
                        [&pn.orphans[1]],
                    )
                    .unwrap();
            }),
        ),
        (
            "another connection adds a reference",
            Box::new(|_, pn, p| {
                let c = raw(p);
                c.execute(
                    "UPDATE nodes SET body = body || ?1",
                    [format!("assets/{}.png", pn.orphans[0])],
                )
                .unwrap();
            }),
        ),
    ];
    for (what, change) in changes {
        let d = Dir::new();
        let p = d.join("n.omatree");
        let (mut doc, pn) = prune_fixture(&p);
        let preview = doc.preview_attachment_prune().unwrap();
        change(&mut doc, &pn, &p);
        let rows_before = rows_of(&p);
        let res = doc.prune_attachments(&preview.token);
        assert!(
            matches!(res, Err(PruneError::Stale | PruneError::NeedsSave))
                || matches!(&res, Err(e) if e.is_conflict()),
            "{what}: the old preview must be refused, got {res:?}"
        );
        assert_eq!(rows_of(&p), rows_before, "{what}: a row was deleted anyway");
        // Whatever was added, the referenced attachment is still there.
        assert!(rows_of(&p).contains(&pn.keep), "{what}");
    }
}

#[test]
fn a_preview_cannot_be_reused_after_save_as_or_on_a_copy_and_cannot_be_replayed() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let (mut doc, pn) = prune_fixture(&p);
    let preview = doc.preview_attachment_prune().unwrap();
    // Replaying on a byte-for-byte copy opened as its own document.
    let copy = d.join("copy.omatree");
    std::fs::copy(&p, &copy).unwrap();
    let mut other = Document::open(&copy).unwrap();
    assert!(matches!(
        other.prune_attachments(&preview.token),
        Err(PruneError::Stale)
    ));
    assert_eq!(rows_of(&copy).len(), 4);
    // After Save As, the document's file is another one.
    let moved = d.join("moved.omatree");
    doc.save_as(&moved, false).unwrap();
    assert!(matches!(
        doc.prune_attachments(&preview.token),
        Err(PruneError::Stale)
    ));
    assert_eq!(rows_of(&moved).len(), 4);
    assert_eq!(
        rows_of(&p).len(),
        4,
        "the original file was not touched either"
    );
    // A genuine token works once and only once.
    let fresh = doc.preview_attachment_prune().unwrap();
    let done = doc.prune_attachments(&fresh.token).unwrap();
    assert_eq!(done.removed.count, 3);
    assert_eq!(rows_of(&moved), vec![pn.keep.clone()]);
    assert!(matches!(
        doc.prune_attachments(&fresh.token),
        Err(PruneError::Stale)
    ));
    assert_eq!(rows_of(&moved), vec![pn.keep.clone()]);
    // The copy and the original are unaffected by what happened to `moved`.
    assert_eq!(rows_of(&p).len(), 4);
}

#[test]
fn a_refused_prune_with_unsaved_changes_saves_nothing() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let (mut doc, pn) = prune_fixture(&p);
    let n = doc.create_root("Neuloženo").unwrap();
    doc.set_body(n, "text\n").unwrap();
    // Unsaved changes: refused with NeedsSave, and the changes stay unsaved.
    let preview = doc.preview_attachment_prune().unwrap();
    let before = state(&p);
    let res = doc.prune_attachments(&preview.token);
    assert!(matches!(res, Err(PruneError::NeedsSave)), "{res:?}");
    assert!(doc.is_dirty());
    assert_eq!(state(&p), before, "nothing was saved, pruned or vacuumed");
    let _ = pn;
}

// --- 4 (continued). Read-only destinations, permissions, lock contention ------------------------

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(unix)]
fn chmod(path: &Path, mode: u32) {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

#[test]
fn a_read_only_directory_fails_every_write_cleanly_and_keeps_the_pending_state() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let ids = v3(&p);
    let before = state(&p);
    let mut doc = Document::open(&p).unwrap();
    scenario(&mut doc, &ids, true);
    let pending = doc.pending_attachments();
    // The permission changes while the notebook is open: the rollback journal cannot be made.
    chmod(d.path(), 0o555);
    let res = doc.save();
    assert!(res.is_err(), "saving into a read-only directory must fail");
    assert_eq!(doc.pending_attachments(), pending);
    assert!(doc.is_dirty());
    chmod(d.path(), 0o755);
    assert_eq!(state(&p), before, "nothing was committed");
    doc.save().unwrap();
    assert_eq!(state(&p).attachments.len(), 2);
    drop(doc);

    // Save As into a directory that has become read-only: nothing is left behind.
    let mut doc = Document::open(&p).unwrap();
    doc.put_attachment(&png(3, 3)).unwrap();
    let ro = d.join("ro");
    std::fs::create_dir(&ro).unwrap();
    chmod(&ro, 0o555);
    let target = ro.join("copy.omatree");
    assert!(doc.save_as(&target, false).is_err());
    assert!(!target.exists());
    assert!(doc.is_dirty() && doc.pending_attachments().0 == 1);
    assert!(doc.is_current_path(&p));
    chmod(&ro, 0o755);
    doc.save_as(&target, false).unwrap();
    assert_eq!(state(&target).attachments.len(), 3);

    // Pruning and reclaiming in a read-only directory change nothing either.
    let q = d.join("q.omatree");
    let (mut doc, _) = prune_fixture(&q);
    let preview = doc.preview_attachment_prune().unwrap();
    let before = state(&q);
    chmod(d.path(), 0o555);
    let res = doc.prune_attachments(&preview.token);
    chmod(d.path(), 0o755);
    assert!(res.is_err());
    assert_eq!(state(&q), before);
    let again = doc.preview_attachment_prune().unwrap();
    doc.prune_attachments(&again.token).unwrap();
}

#[test]
fn a_read_only_target_file_is_reported_as_what_it_is() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    v4(&p);
    let target = d.join("t.omatree");
    v3(&target);
    chmod(&target, 0o444);
    let before = std::fs::read(&target).unwrap();
    let mut doc = Document::open(&p).unwrap();
    let err = doc.save_as(&target, true).unwrap_err();
    eprintln!(
        "replacing a read-only notebook: {err:?} / {}",
        err.save_message()
    );
    assert_eq!(std::fs::read(&target).unwrap(), before);
    assert!(doc.is_current_path(&p));
    chmod(&target, 0o644);
}

#[test]
fn lock_contention_fails_a_save_cleanly_and_a_retry_succeeds_without_a_false_conflict() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let ids = v3(&p);
    let before = state(&p);
    let mut doc = Document::open(&p).unwrap();
    scenario(&mut doc, &ids, false);
    doc.storage_for_test()
        .unwrap()
        .connection()
        .busy_timeout(Duration::ZERO)
        .unwrap();
    let other = raw(&p);
    other.execute_batch("BEGIN EXCLUSIVE").unwrap();
    let res = doc.save();
    assert!(res.is_err(), "a locked file cannot be saved");
    assert!(
        !matches!(&res, Err(e) if e.is_conflict()),
        "a lock is not an external write"
    );
    assert_eq!(doc.pending_attachments().0, 2);
    other.execute_batch("ROLLBACK").unwrap();
    assert_eq!(state(&p), before);
    doc.save().unwrap();
    assert_eq!(state(&p).attachments.len(), 2);

    // Save As into a locked target fails and leaves that notebook as it was.
    let target = d.join("t.omatree");
    v3(&target);
    let target_before = state(&target);
    let lock = raw(&target);
    lock.execute_batch("BEGIN EXCLUSIVE").unwrap();
    doc.put_attachment(&png(4, 4)).unwrap();
    let res = doc.save_as(&target, true);
    assert!(res.is_err());
    lock.execute_batch("ROLLBACK").unwrap();
    assert_eq!(state(&target), target_before);
    assert!(doc.is_current_path(&p) && doc.pending_attachments().0 == 1);
}

#[test]
fn an_attachments_table_that_vanishes_under_an_open_notebook_stops_save_as() {
    // Found by review: `has_stored` used to turn the error into `false`, and Save As then wrote a
    // "successful" notebook without the images its notes still reference.
    let d = Dir::new();
    let p = d.join("n.omatree");
    let (reference, _, _) = v4(&p);
    let mut doc = Document::open(&p).unwrap();
    raw(&p).execute_batch("DROP TABLE attachments").unwrap();
    let target = d.join("copy.omatree");
    let res = doc.save_as(&target, false);
    assert!(
        res.is_err(),
        "Save As must not report success without the images"
    );
    assert!(!target.exists());
    assert!(doc.attachment(&reference).is_err());
}

// --- 8. Identity and reference safety -----------------------------------------------------------

use std::collections::BTreeMap;

#[test]
fn two_notebooks_with_the_same_node_ids_share_no_attachment_state() {
    let d = Dir::new();
    let (p1, p2) = (d.join("one.omatree"), d.join("two.omatree"));
    let mut a = Document::open(&p1).unwrap();
    let mut b = Document::open(&p2).unwrap();
    let (na, nb) = (
        a.create_root("Kořen").unwrap(),
        b.create_root("Kořen").unwrap(),
    );
    assert_eq!(na, nb, "both notebooks use the same first node id");
    let same_bytes = png(6, 6);
    let ra = a.put_attachment(&same_bytes).unwrap();
    let rb = b.put_attachment(&same_bytes).unwrap();
    assert_ne!(ra, rb, "identical bytes in two notebooks get unrelated ids");
    a.set_body(na, &format!("![a]({ra})\n")).unwrap();
    b.set_body(nb, &format!("![b]({rb})\n")).unwrap();
    a.save().unwrap();
    b.save().unwrap();
    // A reference copied from one notebook into the other resolves to nothing, not to the other
    // notebook's image.
    assert!(matches!(
        b.attachment(&ra),
        Err(AttachmentError::NotFound(_))
    ));
    assert!(matches!(
        a.attachment(&rb),
        Err(AttachmentError::NotFound(_))
    ));
    b.set_body(nb, &format!("![a]({ra})\n![b]({rb})\n"))
        .unwrap();
    b.save().unwrap();
    let report = b.preview_attachment_prune().unwrap().report;
    assert_eq!(
        report.missing_references, 1,
        "the foreign reference stays visibly missing"
    );
    assert_eq!(report.eligible.len(), 0);
    assert_eq!(rows_of(&p2).len(), 1);
    // Dropping the foreign reference leaves nothing dangling, and `a`'s image never entered `b`.
    assert!(!rows_of(&p2).contains(&id_of(&ra)));
}

#[test]
fn attachment_ids_are_never_reused_within_a_notebook() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    let mut seen = std::collections::BTreeSet::new();
    let tiny = jpeg();
    for round in 0..3 {
        for _ in 0..700 {
            let r = doc.put_attachment(&tiny).unwrap();
            assert!(seen.insert(id_of(&r)), "id reused in round {round}");
        }
        doc.save().unwrap();
        drop(doc);
        doc = Document::open(&p).unwrap();
    }
    assert_eq!(rows_of(&p).len(), 2100);
    // A drawn id that is already stored is skipped, never overwritten.
    let stored_id = rows_of(&p)[0].clone();
    let bytes: Vec<u8> = (0..16)
        .map(|i| u8::from_str_radix(&stored_id[i * 2..i * 2 + 2], 16).unwrap())
        .collect();
    let mut first = true;
    let r = doc
        .put_attachment_with(&png(2, 2), &mut || {
            if std::mem::take(&mut first) {
                Ok(bytes.clone().try_into().unwrap())
            } else {
                crate::attachments::random_bytes()
            }
        })
        .unwrap();
    assert_ne!(id_of(&r), stored_id);
}

#[test]
fn save_as_keeps_every_id_and_every_byte() {
    let d = Dir::new();
    let (p, q) = (d.join("n.omatree"), d.join("copy.omatree"));
    let mut doc = Document::open(&p).unwrap();
    for i in 0..5u32 {
        doc.put_attachment(&png(3 + i, 3)).unwrap();
    }
    doc.put_attachment(&jpeg()).unwrap();
    doc.save().unwrap();
    doc.put_attachment(&png(30, 3)).unwrap(); // pending at Save As
    doc.save_as(&q, false).unwrap();
    let copy = stored(&q);
    assert_eq!(copy.len(), 7);
    let original = stored(&p);
    for row in &original {
        assert!(copy.contains(row), "{} changed in the copy", row.0);
    }
}

#[test]
fn nothing_outside_the_notebook_is_ever_read_to_resolve_a_reference() {
    // A decoy file where a naive implementation might look: next to the notebook, in an
    // `assets` folder, named exactly like a reference for an id that is not stored.
    let d = Dir::new();
    let p = d.join("n.omatree");
    v4(&p);
    let id = "cd".repeat(16);
    std::fs::create_dir(d.join("assets")).unwrap();
    std::fs::write(d.join("assets").join(format!("{id}.png")), png(2, 2)).unwrap();
    let doc = Document::open(&p).unwrap();
    for r in [
        format!("assets/{id}.png"),
        format!("{}/assets/{id}.png", d.path().display()),
        format!("file://{}/assets/{id}.png", d.path().display()),
    ] {
        assert!(doc.attachment(&r).is_err(), "{r}");
        assert!(doc.attachment_info(&r).is_err(), "{r}");
    }
    assert!(matches!(
        doc.attachment(&format!("assets/{id}.png")),
        Err(AttachmentError::NotFound(_))
    ));
}

// --- 9. The whole recovery lifecycle ------------------------------------------------------------

/// Every expected image is in the file, byte for byte, and the file is healthy.
fn assert_all_retrievable(path: &Path, expected: &BTreeMap<String, Vec<u8>>, when: &str) {
    let rows: BTreeMap<String, Vec<u8>> = stored(path).into_iter().map(|r| (r.0, r.3)).collect();
    for (id, bytes) in expected {
        assert_eq!(
            rows.get(id),
            Some(bytes),
            "{when}: attachment {id} lost or changed"
        );
    }
    assert_eq!(state(path).integrity, "ok", "{when}");
}

#[test]
fn the_full_recovery_lifecycle_never_loses_a_recoverable_image() {
    let d = Dir::new();
    let p = d.join("life.omatree");
    let mut doc = Document::open(&p).unwrap();
    let mut expected = BTreeMap::new(); // recoverable images only
    let put = |doc: &Document, bytes: Vec<u8>| -> (String, String) {
        let r = doc.put_attachment(&bytes).unwrap();
        (id_of(&r), r)
    };
    let (a, ra) = put(&doc, png(3, 3));
    let (b, rb) = put(&doc, png(4, 3));
    let (t, rt) = put(&doc, png(5, 3));
    let (c, rc) = put(&doc, jpeg());
    let (o, _) = put(&doc, png(7, 3)); // never referenced
    for (id, bytes) in [
        (&a, png(3, 3)),
        (&b, png(4, 3)),
        (&t, png(5, 3)),
        (&c, jpeg()),
    ] {
        expected.insert(id.clone(), bytes);
    }

    let first = doc.create_root("Příliš žluťoučký kůň 🦀").unwrap();
    doc.set_body(first, &format!("![a]({ra}) ![b]({rb})\n"))
        .unwrap();
    let mut parent = first;
    let mut chain = Vec::new();
    for depth in 1..=8 {
        parent = doc
            .create_child(parent, &format!("Úroveň {depth} 🌲"))
            .unwrap();
        chain.push(parent);
    }
    doc.set_body(*chain.last().unwrap(), &format!("hluboko ![t]({rt})\n"))
        .unwrap();
    let watch = doc.create_root("Kontrola").unwrap();
    doc.set_body(watch, &format!("![c]({rc})\n")).unwrap();
    doc.set_expanded(first, true);
    // 1. Save and reopen.
    doc.save().unwrap();
    drop(doc);
    assert_all_retrievable(&p, &expected, "1 save and reopen");
    let mut doc = Document::open(&p).unwrap();
    // A checkpoint while `watch` still shows `c`, then the note drops it: `c` is now recoverable
    // only through that checkpoint.
    doc.create_checkpoint("early");
    doc.set_body(watch, "nic\n").unwrap();
    doc.save().unwrap();
    // 2. Delete a subtree, 3. save and reopen.
    doc.delete(first).unwrap();
    doc.save().unwrap();
    drop(doc);
    assert_all_retrievable(&p, &expected, "3 reopen after delete");
    let mut doc = Document::open(&p).unwrap();
    assert_eq!(doc.recovery().trash().len(), 1);
    // 4. Restore Trash.
    doc.restore_trash(0).unwrap();
    doc.save().unwrap();
    assert_all_retrievable(&p, &expected, "4 restore trash");
    // 5. Restore the early checkpoint (which has `c` in `watch`), then drop it again.
    let early = doc
        .recovery()
        .checkpoints()
        .iter()
        .position(|cp| cp.reason() == "early")
        .unwrap();
    doc.restore_checkpoint(early).unwrap();
    doc.set_body(watch, "znovu nic\n").unwrap();
    doc.save().unwrap();
    assert_all_retrievable(&p, &expected, "5 restore checkpoint");
    // Put the subtree back in Trash so Trash and checkpoints both hold images again.
    doc.delete(first).unwrap();
    doc.save().unwrap();
    // 6. Save As, with one more image still pending.
    let (pend, _) = put(&doc, png(8, 3));
    let q = d.join("life-copy.omatree");
    doc.save_as(&q, false).unwrap();
    assert_all_retrievable(&q, &expected, "6 save as");
    assert!(rows_of(&q).contains(&pend) && rows_of(&q).contains(&o));
    // 7. Preview: exactly the two never-referenced images are eligible.
    let before_prune = state(&q);
    let preview = doc.preview_attachment_prune().unwrap();
    let mut eligible: Vec<String> = preview
        .report
        .eligible
        .iter()
        .map(|e| e.id.to_string())
        .collect();
    eligible.sort();
    let mut want = vec![o.clone(), pend.clone()];
    want.sort();
    assert_eq!(eligible, want, "7 preview");
    assert_eq!(state(&q), before_prune, "a preview writes nothing");
    // 8. Prune them.
    let done = doc.prune_attachments(&preview.token).unwrap();
    assert_eq!(done.removed.count, 2);
    let after_prune = state(&q);
    assert_eq!(
        after_prune.logical, before_prune.logical,
        "notes, Trash, checkpoints, expansion untouched"
    );
    assert_eq!(rows_of(&q).len(), expected.len());
    assert_all_retrievable(&q, &expected, "8 prune");
    // 9. Restore a checkpoint that refers to an older image, 10. and read its bytes back.
    let cp = doc
        .recovery()
        .checkpoints()
        .iter()
        .position(|cp| cp.reason() == "early")
        .unwrap();
    doc.restore_checkpoint(cp).unwrap();
    let body = doc.notebook().get(watch).unwrap().body().to_owned();
    assert!(
        body.contains(&rc),
        "the restored note shows the older image again"
    );
    assert_eq!(doc.attachment(&rc).unwrap(), jpeg(), "10 original bytes");
    doc.save().unwrap();
    // 11. Reclaim, separately.
    let r = doc.preview_reclaim().unwrap();
    if !r.nothing_to_reclaim {
        let out = doc.reclaim_space(&r.token).unwrap();
        assert!(out.size_after <= out.size_before);
    }
    assert_all_retrievable(&q, &expected, "11 reclaim");
    // 12. Close and reopen once more; every table is there and the file is healthy.
    drop(doc);
    let doc = Document::open(&q).unwrap();
    assert_eq!(header_version(&q), 4);
    let report = doc.verify_integrity().unwrap();
    assert!(report.is_healthy(), "{report:?}");
    for id in expected.keys() {
        let ext = if *id == c { "jpg" } else { "png" };
        assert_eq!(
            doc.attachment(&format!("assets/{id}.{ext}")).unwrap(),
            expected[id],
            "12 reopen"
        );
    }
    let names = tables(&q);
    for t in [
        "attachments",
        "nodes",
        "trash_entries",
        "trash_nodes",
        "checkpoints",
        "checkpoint_nodes",
        "expanded_nodes",
        "notebook_meta",
    ] {
        assert!(names.iter().any(|n| n == t), "{t}");
    }
}

// --- 7. Large attachments and memory ------------------------------------------------------------
//
// `cargo test -p omatree-core --release -- --ignored large_attachment_measurements --nocapture`
// builds a notebook with attachments up to the 64 MiB limit and measures each operation in its
// own child process, so that each peak resident set size (`VmHWM`) belongs to that operation
// alone. Files go under `target/e9c-large` (on disk, not RAM-backed `/tmp`).

use crate::attachments::fixtures::png_of_len;
use crate::attachments::MAX_ATTACHMENT_BYTES;

const MIB: usize = 1024 * 1024;

fn large_sizes() -> [usize; 4] {
    [MAX_ATTACHMENT_BYTES as usize, 48 * MIB, 32 * MIB, 16 * MIB]
}

#[cfg(not(target_os = "linux"))]
fn peak_kib() -> u64 {
    0 // `/proc` is Linux-only
}

#[cfg(target_os = "linux")]
fn peak_kib() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("VmHWM:"))
        .and_then(|v| v.trim().trim_end_matches("kB").trim().parse().ok())
        .unwrap_or(0)
}

#[cfg(not(target_os = "linux"))]
fn rss_kib() -> i64 {
    0 // `/proc` is Linux-only
}

#[cfg(target_os = "linux")]
fn rss_kib() -> i64 {
    std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("VmRSS:"))
        .and_then(|v| v.trim().trim_end_matches("kB").trim().parse().ok())
        .unwrap_or(0)
}

fn report(phase: &str, started: std::time::Instant, extra: &str) {
    println!(
        "PHASE {phase} {:.0} ms, peak resident {} MiB {extra}",
        started.elapsed().as_secs_f64() * 1000.0,
        peak_kib() / 1024
    );
}

#[test]
fn large_phase_child() {
    let (Ok(dir), Ok(phase)) = (
        std::env::var("E9C_LARGE_DIR"),
        std::env::var("E9C_LARGE_PHASE"),
    ) else {
        return;
    };
    let dir = PathBuf::from(dir);
    let p = dir.join("large.omatree");
    let t = std::time::Instant::now();
    match phase.as_str() {
        "build" => {
            let mut doc = Document::open(&p).unwrap();
            let mut refs = Vec::new();
            for size in large_sizes() {
                // The fixture itself is built (and its memory returned) before measuring.
                let bytes = png_of_len(size);
                let rss = rss_kib();
                let started = std::time::Instant::now();
                refs.push(doc.put_attachment(&bytes).unwrap());
                let grew = (rss_kib() - rss) / 1024;
                report(
                    &format!("insert {} MiB (pending)", size / MIB),
                    started,
                    &format!("(resident grew by {grew} MiB: the one stored copy; the peak figure includes the test's own fixture)"),
                );
            }
            let n = doc.create_root("Velké obrázky 🖼").unwrap();
            // The 64 MiB image and the 16 MiB one stay referenced; the 48 and 32 MiB are orphans.
            doc.set_body(n, &format!("![a]({})\n![d]({})\n", refs[0], refs[3]))
                .unwrap();
            let started = std::time::Instant::now();
            doc.save().unwrap();
            report(
                "first save (v3 -> v4, 4 attachments, 160 MiB)",
                started,
                &format!(
                    "file {} MiB",
                    std::fs::metadata(&p).unwrap().len() / MIB as u64
                ),
            );
        }
        "reopen" => {
            let doc = Document::open(&p).unwrap();
            report("open (notes only)", t, "");
            let id = doc.stored_attachment_ids().unwrap();
            assert_eq!(id.len(), 4);
            let ids = doc.stored_attachment_ids().unwrap();
            let rss = rss_kib();
            let started = std::time::Instant::now();
            let mut sizes = Vec::new();
            for id in &ids {
                // One image at a time: the previous one is dropped before the next is read.
                let len = doc.attachment(&format!("assets/{id}.png")).unwrap().len();
                sizes.push(len);
            }
            report(
                "retrieve each of the 4 images once",
                started,
                &format!("(resident after: +{} MiB)", (rss_kib() - rss) / 1024),
            );
            // Exactness, outside the measurement.
            for id in &ids {
                let bytes = doc.attachment(&format!("assets/{id}.png")).unwrap();
                assert!(large_sizes().contains(&bytes.len()));
                assert_eq!(bytes, png_of_len(bytes.len()), "exact bytes");
            }
            assert_eq!(sizes.len(), 4);
        }
        "saveas" => {
            let mut doc = Document::open(&p).unwrap();
            let q = dir.join("copy.omatree");
            let _ = std::fs::remove_file(&q);
            let started = std::time::Instant::now();
            doc.save_as(&q, false).unwrap();
            report("save as (4 attachments, 160 MiB)", started, "");
            assert_eq!(
                std::fs::metadata(&p).unwrap().len(),
                std::fs::metadata(&q).unwrap().len()
            );
            let (a, b) = (stored(&p), stored(&q));
            assert_eq!(a, b, "byte-identical rows");
        }
        "preview" => {
            let doc = Document::open(&p).unwrap();
            let started = std::time::Instant::now();
            let pv = doc.preview_attachment_prune().unwrap();
            assert_eq!(pv.report.eligible.len(), 2);
            let rv = doc.preview_reclaim().unwrap();
            let _ = rv;
            report("prune preview + reclaim preview", started, "(no BLOB read)");
        }
        "prune" => {
            let mut doc = Document::open(&p).unwrap();
            let pv = doc.preview_attachment_prune().unwrap();
            let started = std::time::Instant::now();
            let done = doc.prune_attachments(&pv.token).unwrap();
            assert_eq!(done.removed.count, 2);
            report(
                "transactional prune of 80 MiB",
                started,
                &format!(
                    "file still {} MiB",
                    std::fs::metadata(&p).unwrap().len() / MIB as u64
                ),
            );
        }
        "reclaim" => {
            let mut doc = Document::open(&p).unwrap();
            let rv = doc.preview_reclaim().unwrap();
            let started = std::time::Instant::now();
            let out = doc.reclaim_space(&rv.token).unwrap();
            report(
                "VACUUM",
                started,
                &format!(
                    "file {} -> {} MiB",
                    out.size_before / MIB as u64,
                    out.size_after / MIB as u64
                ),
            );
        }
        other => panic!("unknown phase {other}"),
    }
}

#[test]
#[ignore = "builds a 160 MiB notebook; run separately, ideally with --release"]
fn large_attachment_measurements() {
    let dir = std::env::var("E9C_LARGE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/e9c-large")
        });
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let exe = std::env::current_exe().unwrap();
    for phase in ["build", "reopen", "saveas", "preview", "prune", "reclaim"] {
        let out = Command::new(&exe)
            .args([
                "--exact",
                "integrity_tests::large_phase_child",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("E9C_LARGE_DIR", &dir)
            .env("E9C_LARGE_PHASE", phase)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success(),
            "{phase}:\n{text}\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        for line in text.lines().filter_map(|l| l.split("PHASE ").nth(1)) {
            eprintln!("{line}");
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// --- 7 (continued). Memory exhaustion ----------------------------------------------------------
//
// The parent re-runs `oom_child` under `ulimit -v`, so that allocations really fail. An
// allocation that cannot be made must surface as an error and leave the notebook and the
// process usable; it must never abort.

#[test]
fn oom_child() {
    let Ok(path) = std::env::var("E9C_OOM_PATH") else {
        return;
    };
    let path = PathBuf::from(path);
    let phase = std::env::var("E9C_OOM_PHASE").unwrap_or_default();
    let before = state(&path);
    if phase == "put" {
        return oom_put(&path);
    }
    // Reading: hold each 64 MiB image that is read until memory runs out.
    let doc = Document::open(&path).unwrap();
    let ids = doc.stored_attachment_ids().unwrap();
    let mut held = Vec::new();
    let mut failed_reading = false;
    for round in 0..40 {
        let id = &ids[round % ids.len()];
        match doc.attachment(&format!("assets/{id}.png")) {
            Ok(bytes) => held.push(bytes),
            Err(e) => {
                println!("OOM-READ-REFUSED after {} images: {e}", held.len());
                failed_reading = true;
                break;
            }
        }
    }
    assert!(
        failed_reading,
        "memory never ran out; the limit is too high for this test"
    );
    drop(held);
    drop(doc);
    assert_eq!(state(&path), before, "reading changed the file");

    println!("OOM-OK");
}

/// Putting: pending images accumulate until memory runs out (under a limit tighter than the
/// 128 MiB budget needs, or the budget would stop it first).
fn oom_put(path: &Path) {
    let doc = Document::open(path).unwrap();
    let fixture = png_of_len(MAX_ATTACHMENT_BYTES as usize);
    let mut put = 0;
    let mut failed_putting = false;
    for _ in 0..40 {
        match doc.put_attachment(&fixture) {
            Ok(_) => put += 1,
            Err(AttachmentError::Storage(why)) => {
                println!("OOM-PUT-REFUSED after {put} images: {why}");
                failed_putting = true;
                break;
            }
            Err(other) => panic!("unexpected error {other:?}"),
        }
    }
    assert!(failed_putting, "memory never ran out ({put} images held)");
    assert_eq!(
        doc.pending_attachments().0,
        put,
        "every image reported as held is held"
    );
    println!("OOM-OK");
}

#[test]
fn memory_exhaustion_is_an_error_never_an_abort() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    for i in 0..2 {
        // Two big images in the notebook (distinct, so two rows).
        let mut bytes = png_of_len(MAX_ATTACHMENT_BYTES as usize / 2);
        bytes[40] = i as u8; // inside the tEXt padding; the CRC is not checked structurally
        doc.put_attachment(&bytes).unwrap();
    }
    doc.save().unwrap();
    drop(doc);
    // The virtual-memory limit is relative to what the test binary needs just to start.
    for (phase, limit, want) in [
        ("read", "650000", "OOM-READ-REFUSED"),
        ("put", "E9C_PUT_LIMIT", "OOM-PUT-REFUSED"),
    ] {
        let limit = if phase == "put" { PUT_VM_LIMIT } else { limit };
        let out = Command::new("sh")
            .arg("-c")
            .arg(r#"ulimit -v "$E9C_VLIMIT" && exec "$0" --exact integrity_tests::oom_child --nocapture --test-threads=1"#)
            .arg(std::env::current_exe().unwrap())
            .env("E9C_OOM_PATH", &p)
            .env("E9C_OOM_PHASE", phase)
            .env("E9C_VLIMIT", limit)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.status.success(),
            "{phase}: child did not finish cleanly (abort?):\n{text}\n{err}"
        );
        assert!(
            text.contains("OOM-OK") && text.contains(want),
            "{phase}: {text}"
        );
        for line in text.lines().filter(|l| l.contains("OOM-")) {
            eprintln!("{line}");
        }
    }
}

/// Tighter than the read phase: room for the fixture and about two images, which is less than
/// the budget allows.
const PUT_VM_LIMIT: &str = "400000";

#[test]
fn after_a_failed_vacuum_save_as_and_repeated_attempts_work_and_old_tokens_die() {
    let d = Dir::new();
    let p = d.join("v.omatree");
    let mut doc = with_free_pages(&p, Journal::Delete);
    let token = doc.preview_reclaim().unwrap().token;
    // A failure that makes SQLite move `data_version` (no journal can be created): the baseline
    // is re-accepted only because the file's change counter proves nobody wrote.
    chmod(d.path(), 0o555);
    let res = doc.reclaim_space_waiting(&token, Duration::ZERO);
    chmod(d.path(), 0o755);
    assert!(res.is_err());
    // Repeated attempts with the same, still valid authorization: busy twice, then it runs.
    for _ in 0..2 {
        let other = raw(&p);
        other.execute_batch("BEGIN IMMEDIATE").unwrap();
        assert!(matches!(
            doc.reclaim_space_waiting(&token, Duration::ZERO),
            Err(ReclaimError::Busy)
        ));
        other.execute_batch("ROLLBACK").unwrap();
    }
    // Save As straight after failures: copies everything, leaves the old file alone.
    let copy = d.join("copy.omatree");
    let source_before = state(&p);
    doc.save_as(&copy, false).unwrap();
    assert_eq!(state(&p), source_before);
    assert_eq!(state(&copy).attachments, source_before.attachments);
    // The document now lives at `copy`: the old token belongs to the old file.
    assert!(matches!(
        doc.reclaim_space(&token),
        Err(ReclaimError::Stale)
    ));
    // A Save As copy carries only the live rows into a fresh file: nothing left to reclaim.
    assert!(doc.preview_reclaim().unwrap().nothing_to_reclaim);
    // A token is spent by the physical change it authorised.
    let mut spent = with_free_pages(&d.join("s.omatree"), Journal::Delete);
    let fresh = spent.preview_reclaim().unwrap();
    let done = spent.reclaim_space(&fresh.token).unwrap();
    assert!(done.ran && done.size_after < done.size_before);
    assert!(matches!(
        spent.reclaim_space(&fresh.token),
        Err(ReclaimError::Stale)
    ));
    // And a physical change by someone else (here a row deleted and committed elsewhere) kills a
    // token even though the page statistics might not move.
    let mut doc = with_free_pages(&d.join("w.omatree"), Journal::Delete);
    let t = doc.preview_reclaim().unwrap().token;
    raw(&d.join("w.omatree"))
        .execute_batch("CREATE TABLE elsewhere (x); INSERT INTO elsewhere VALUES (1)")
        .unwrap();
    assert!(doc.reclaim_space(&t).is_err());
    assert_eq!(state(&d.join("w.omatree")).integrity, "ok");
}

#[test]
fn save_as_onto_another_name_of_the_same_file_is_safe() {
    // A hard link is the same file under a second name: `is_current_path` (canonicalise) cannot
    // tell, so the target is "an existing notebook being replaced" while it is also the source.
    // It must either succeed with every image intact or fail without damaging the file.
    for size_mib in [0usize, 6] {
        let d = Dir::new();
        let p = d.join("a.omatree");
        let mut doc = Document::open(&p).unwrap();
        let n = doc.create_root("Kořen").unwrap();
        let small = doc.put_attachment(&png(5, 5)).unwrap();
        doc.set_body(n, &format!("![x]({small})\n")).unwrap();
        if size_mib > 0 {
            doc.put_attachment(&png_of_len(size_mib * MIB)).unwrap();
        }
        doc.save().unwrap();
        let before = state(&p);
        let link = d.join("b.omatree");
        std::fs::hard_link(&p, &link).unwrap();
        let started = std::time::Instant::now();
        let res = doc.save_as(&link, true);
        eprintln!(
            "save as onto a hard link of itself ({size_mib} MiB image): {:?} after {} ms",
            res.as_ref().map(|_| "ok").map_err(|e| format!("{e:?}")),
            started.elapsed().as_millis()
        );
        let after = state(&p);
        assert_eq!(after.integrity, "ok");
        assert_eq!(
            after.attachments.len(),
            before.attachments.len(),
            "an image was lost"
        );
        assert_eq!(after.attachments, before.attachments, "an image changed");
        assert_eq!(after.logical, before.logical);
        assert_eq!(state(&link), after, "both names are one file");
    }
}
