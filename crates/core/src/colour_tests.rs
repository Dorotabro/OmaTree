//! T1: a note's colour tag. Persistence, Trash, checkpoints, migration, refusal of bad data and
//! atomicity, all on disposable notebooks.

use crate::attachment_tests::{header_version, raw, tables, Dir};
use crate::document::Document;
use crate::notebook::{Colour, NodeId};
use rusqlite::Connection;
use std::path::Path;

fn colour_of(doc: &Document, id: NodeId) -> Option<Colour> {
    doc.notebook().get(id).unwrap().colour()
}

fn column_names(path: &Path, table: &str) -> Vec<String> {
    let c = raw(path);
    let mut q = c.prepare(&format!("PRAGMA table_info({table})")).unwrap();
    q.query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

/// Parent "P" with child "C" and a second root "Q".
fn tree(path: &Path) -> (Document, NodeId, NodeId, NodeId) {
    let mut d = Document::open(path).unwrap();
    let p = d.create_root("P").unwrap();
    let c = d.create_child(p, "C").unwrap();
    let q = d.create_root("Q").unwrap();
    d.set_body(c, "body of c\n").unwrap();
    d.save().unwrap();
    (d, p, c, q)
}

#[test]
fn a_new_note_has_no_colour_and_a_notebook_without_tags_stays_at_version_3() {
    let d = Dir::new();
    let path = d.join("n.omatree");
    let (doc, p, c, q) = tree(&path);
    for id in [p, c, q] {
        assert_eq!(colour_of(&doc, id), None);
    }
    assert_eq!(header_version(&path), 3, "no tag, no schema change");
    assert!(!column_names(&path, "nodes").contains(&"colour".to_owned()));
}

#[test]
fn every_colour_can_be_assigned_replaced_and_cleared() {
    let d = Dir::new();
    let (mut doc, p, ..) = tree(&d.join("n.omatree"));
    for colour in Colour::ALL {
        assert!(doc.set_colour(p, Some(colour)).unwrap(), "{colour:?}");
        assert_eq!(colour_of(&doc, p), Some(colour));
        assert!(
            !doc.set_colour(p, Some(colour)).unwrap(),
            "same colour: no change"
        );
    }
    assert!(doc.set_colour(p, Some(Colour::Red)).unwrap(), "replace");
    assert_eq!(colour_of(&doc, p), Some(Colour::Red));
    assert!(doc.set_colour(p, None).unwrap(), "clear");
    assert_eq!(colour_of(&doc, p), None);
    assert!(
        !doc.set_colour(p, None).unwrap(),
        "clearing nothing changes nothing"
    );
    assert!(doc.set_colour(NodeId::from_raw(999), None).is_err());
}

#[test]
fn a_tag_edit_is_a_modification_like_a_rename_and_a_no_op_is_not() {
    let d = Dir::new();
    let (mut doc, p, ..) = tree(&d.join("n.omatree"));
    assert!(!doc.is_dirty());
    doc.set_colour(p, None).unwrap();
    assert!(!doc.is_dirty(), "nothing changed");
    doc.set_colour(p, Some(Colour::Blue)).unwrap();
    assert!(doc.is_dirty(), "autosave and the close guard see it");
    doc.save().unwrap();
    assert!(!doc.is_dirty());
}

#[test]
fn tags_survive_save_reopen_and_the_file_becomes_version_5() {
    let d = Dir::new();
    let path = d.join("n.omatree");
    let (mut doc, p, c, q) = tree(&path);
    doc.set_colour(p, Some(Colour::Orange)).unwrap();
    doc.set_colour(c, Some(Colour::Purple)).unwrap();
    doc.save().unwrap();
    assert_eq!(header_version(&path), 5);
    for table in ["nodes", "trash_nodes", "checkpoint_nodes"] {
        assert!(
            column_names(&path, table).contains(&"colour".to_owned()),
            "{table}"
        );
    }
    let re = Document::open_existing(&path).unwrap();
    assert_eq!(colour_of(&re, p), Some(Colour::Orange));
    assert_eq!(
        colour_of(&re, c),
        Some(Colour::Purple),
        "parent and child are independent"
    );
    assert_eq!(colour_of(&re, q), None);
    let stored: Vec<(String, Option<String>)> = raw(&path)
        .prepare("SELECT title, colour FROM nodes ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        stored,
        [
            ("P".to_owned(), Some("orange".to_owned())),
            ("C".to_owned(), Some("purple".to_owned())),
            ("Q".to_owned(), None)
        ],
        "the identity is stored, never a theme's RGB"
    );
}

#[test]
fn clearing_every_tag_keeps_the_file_readable_and_loses_nothing_else() {
    let d = Dir::new();
    let path = d.join("n.omatree");
    let (mut doc, p, c, _) = tree(&path);
    doc.set_colour(p, Some(Colour::Green)).unwrap();
    doc.save().unwrap();
    doc.set_colour(p, None).unwrap();
    doc.save().unwrap();
    assert_eq!(header_version(&path), 5, "a file never goes back down");
    let re = Document::open_existing(&path).unwrap();
    assert_eq!(colour_of(&re, p), None);
    assert_eq!(re.notebook().get(c).unwrap().body(), "body of c\n");
}

#[test]
fn save_as_carries_tags_to_the_new_file() {
    let d = Dir::new();
    let (mut doc, p, c, _) = tree(&d.join("a.omatree"));
    doc.set_colour(p, Some(Colour::Yellow)).unwrap();
    doc.set_colour(c, Some(Colour::Blue)).unwrap();
    let copy = d.join("copy.omatree");
    doc.save_as(&copy, false).unwrap();
    assert_eq!(header_version(&copy), 5);
    let re = Document::open_existing(&copy).unwrap();
    assert_eq!(colour_of(&re, p), Some(Colour::Yellow));
    assert_eq!(colour_of(&re, c), Some(Colour::Blue));
}

#[test]
fn rename_move_and_reorder_keep_the_tag() {
    let d = Dir::new();
    let (mut doc, p, c, q) = tree(&d.join("n.omatree"));
    doc.set_colour(c, Some(Colour::Red)).unwrap();
    doc.rename(c, "C renamed").unwrap();
    assert_eq!(colour_of(&doc, c), Some(Colour::Red));
    doc.move_node(c, Some(q), 0).unwrap(); // to another parent
    assert_eq!(colour_of(&doc, c), Some(Colour::Red));
    doc.move_node(q, None, 0).unwrap(); // reorder the roots
    assert_eq!(colour_of(&doc, c), Some(Colour::Red));
    doc.save().unwrap();
    let re = Document::open_existing(&d.join("n.omatree")).unwrap();
    assert_eq!(colour_of(&re, c), Some(Colour::Red));
    assert_eq!(colour_of(&re, p), None);
}

#[test]
fn trash_keeps_the_tags_of_the_whole_subtree_and_restore_brings_them_back() {
    let d = Dir::new();
    let path = d.join("n.omatree");
    let (mut doc, p, c, q) = tree(&path);
    doc.set_colour(p, Some(Colour::Green)).unwrap();
    doc.set_colour(c, Some(Colour::Purple)).unwrap();
    doc.delete(p).unwrap();
    assert!(doc.notebook().get(p).is_none());
    doc.save().unwrap();
    // Through the file and back.
    let mut re = Document::open_existing(&path).unwrap();
    let entry = &re.recovery().trash()[0];
    let held: Vec<_> = entry
        .nodes()
        .iter()
        .map(|n| (n.title().to_owned(), n.colour()))
        .collect();
    assert_eq!(
        held,
        [
            ("P".to_owned(), Some(Colour::Green)),
            ("C".to_owned(), Some(Colour::Purple))
        ]
    );
    re.restore_trash(0).unwrap();
    assert_eq!(colour_of(&re, p), Some(Colour::Green));
    assert_eq!(colour_of(&re, c), Some(Colour::Purple));
    assert_eq!(colour_of(&re, q), None);
}

#[test]
fn a_tag_set_after_deleting_does_not_touch_what_trash_holds() {
    let d = Dir::new();
    let (mut doc, p, c, q) = tree(&d.join("n.omatree"));
    doc.set_colour(c, Some(Colour::Red)).unwrap();
    doc.delete(p).unwrap();
    doc.set_colour(q, Some(Colour::Blue)).unwrap();
    doc.restore_trash(0).unwrap();
    assert_eq!(colour_of(&doc, c), Some(Colour::Red));
    assert_eq!(colour_of(&doc, q), Some(Colour::Blue));
}

#[test]
fn checkpoints_hold_tags_as_they_were_and_restoring_one_restores_them() {
    let d = Dir::new();
    let path = d.join("n.omatree");
    let (mut doc, p, c, _) = tree(&path);
    doc.set_colour(p, Some(Colour::Orange)).unwrap();
    doc.create_checkpoint("with orange");
    doc.set_colour(p, Some(Colour::Blue)).unwrap(); // later edits are not in the checkpoint
    doc.set_colour(c, Some(Colour::Green)).unwrap();
    doc.save().unwrap();
    let mut re = Document::open_existing(&path).unwrap();
    let at = re.recovery().checkpoints().len() - 1;
    let held = re.recovery().checkpoints()[at]
        .nodes()
        .iter()
        .find(|n| n.id() == p)
        .unwrap()
        .colour();
    assert_eq!(held, Some(Colour::Orange));
    re.restore_checkpoint(at).unwrap();
    assert_eq!(colour_of(&re, p), Some(Colour::Orange));
    assert_eq!(colour_of(&re, c), None, "the checkpoint had no tag on C");
}

#[test]
fn a_checkpoint_taken_before_tags_existed_restores_to_no_tags() {
    let d = Dir::new();
    let (mut doc, p, ..) = tree(&d.join("n.omatree"));
    doc.create_checkpoint("plain");
    doc.set_colour(p, Some(Colour::Red)).unwrap();
    let at = doc.recovery().checkpoints().len() - 1;
    doc.restore_checkpoint(at).unwrap();
    assert_eq!(colour_of(&doc, p), None);
}

#[test]
fn tags_do_not_change_the_markdown_source_or_title() {
    let d = Dir::new();
    let (mut doc, _, c, _) = tree(&d.join("n.omatree"));
    let (body, title) = {
        let n = doc.notebook().get(c).unwrap();
        (n.body().to_owned(), n.title().to_owned())
    };
    doc.set_colour(c, Some(Colour::Purple)).unwrap();
    let n = doc.notebook().get(c).unwrap();
    assert_eq!((n.body(), n.title()), (body.as_str(), title.as_str()));
}

// ---- migration, compatibility, bad data ----------------------------------------------------

#[test]
fn a_legacy_notebook_is_migrated_atomically_with_its_content_and_only_when_a_tag_is_written() {
    let d = Dir::new();
    let path = d.join("n.omatree");
    let (mut doc, p, c, q) = tree(&path);
    doc.delete(q).unwrap();
    doc.save().unwrap();
    assert_eq!(header_version(&path), 3);

    let mut doc = Document::open_existing(&path).unwrap();
    doc.set_body(c, "edited\n").unwrap();
    doc.save().unwrap();
    assert_eq!(
        header_version(&path),
        3,
        "an ordinary save does not migrate"
    );

    doc.set_colour(p, Some(Colour::Blue)).unwrap();
    doc.save().unwrap();
    assert_eq!(header_version(&path), 5);
    let re = Document::open_existing(&path).unwrap();
    assert_eq!(colour_of(&re, p), Some(Colour::Blue));
    assert_eq!(re.notebook().get(c).unwrap().body(), "edited\n");
    assert_eq!(
        re.recovery().trash().len(),
        1,
        "Trash came through the migration"
    );
    let integrity: String = raw(&path)
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    assert_eq!(integrity, "ok");
}

#[test]
fn a_version_4_notebook_with_an_attachment_keeps_it_through_the_migration() {
    let d = Dir::new();
    let path = d.join("n.omatree");
    let mut doc = Document::open(&path).unwrap();
    let a = doc.create_root("A").unwrap();
    let png = crate::attachments::fixtures::png_1x1();
    let reference = doc.put_attachment(&png).unwrap();
    doc.set_body(a, &format!("![x]({reference})\n")).unwrap();
    doc.save().unwrap();
    assert_eq!(header_version(&path), 4);
    doc.set_colour(a, Some(Colour::Red)).unwrap();
    doc.save().unwrap();
    assert_eq!(header_version(&path), 5);
    assert!(tables(&path).contains(&"attachments".to_owned()));
    let re = Document::open_existing(&path).unwrap();
    assert_eq!(re.attachment(&reference).unwrap(), png);
    assert_eq!(colour_of(&re, a), Some(Colour::Red));
}

#[test]
fn a_stored_colour_that_is_not_one_of_the_six_is_refused_not_dropped() {
    let d = Dir::new();
    let path = d.join("n.omatree");
    let (mut doc, p, ..) = tree(&path);
    doc.set_colour(p, Some(Colour::Red)).unwrap();
    doc.save().unwrap();
    drop(doc);
    // The CHECK constraint stops it from being written through SQLite at all ...
    let c = raw(&path);
    assert!(c
        .execute("UPDATE nodes SET colour = 'pink' WHERE title = 'P'", [])
        .is_err());
    // ... so build a file whose constraint is absent (someone else's tool) to test the reader.
    c.execute_batch(
        "PRAGMA writable_schema = ON;
         UPDATE sqlite_master SET sql = replace(sql, 'CHECK (colour IS NULL OR colour IN (''red'',''orange'',''yellow'',''green'',''blue'',''purple''))', '') WHERE name = 'nodes';
         PRAGMA writable_schema = OFF;",
    )
    .unwrap();
    drop(c);
    let c = Connection::open(&path).unwrap();
    c.execute("UPDATE nodes SET colour = 'pink' WHERE title = 'P'", [])
        .unwrap();
    drop(c);
    let before = std::fs::read(&path).unwrap();
    let err = Document::open_existing(&path)
        .err()
        .expect("refused")
        .to_string();
    assert!(
        err.to_lowercase().contains("colour") || err.to_lowercase().contains("invalid"),
        "{err}"
    );
    assert_eq!(
        std::fs::read(&path).unwrap(),
        before,
        "refusing changed nothing"
    );
    // Case and padding are not forgiven either: the identifiers are exact.
    assert_eq!(Colour::parse("Red"), None);
    assert_eq!(Colour::parse(" red"), None);
    assert_eq!(Colour::parse(""), None);
    for c in Colour::ALL {
        assert_eq!(Colour::parse(c.as_str()), Some(c));
    }
}

#[test]
fn a_build_that_does_not_know_a_newer_schema_refuses_it_untouched() {
    // This build's own check of the rule an older build (before T1, maximum version 4) applies to a
    // version 5 file: anything above the newest known version is refused with the file unchanged.
    // (tools/pre-e9-compat.sh runs the real older core against a version 5 file.)
    let d = Dir::new();
    let path = d.join("n.omatree");
    let (mut doc, p, ..) = tree(&path);
    doc.set_colour(p, Some(Colour::Red)).unwrap();
    doc.save().unwrap();
    drop(doc);
    raw(&path).pragma_update(None, "user_version", 6).unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(Document::open_existing(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn a_failed_write_changes_neither_the_tags_nor_the_schema_nor_the_content() {
    let d = Dir::new();
    let path = d.join("n.omatree");
    let (doc, p, c, _) = tree(&path);
    assert_eq!(header_version(&path), 3);
    // Another connection makes the next insert fail inside the migrating transaction.
    raw(&path)
        .execute_batch(
            "CREATE TRIGGER refuse BEFORE INSERT ON checkpoint_nodes
             BEGIN SELECT RAISE(ABORT, 'refused for the test'); END;",
        )
        .unwrap();
    // (The trigger is a commit by another connection; resynchronise by reopening the notebook.)
    drop(doc);
    let mut doc = Document::open_existing(&path).unwrap();
    doc.set_colour(p, Some(Colour::Green)).unwrap();
    doc.delete(c).unwrap(); // takes a checkpoint, so the save writes checkpoint rows
    assert!(doc.save().is_err());
    assert!(doc.is_dirty(), "still unsaved, still in memory");
    assert_eq!(colour_of(&doc, p), Some(Colour::Green));
    drop(doc);
    assert_eq!(
        header_version(&path),
        3,
        "the migration rolled back with the rest"
    );
    assert!(!column_names(&path, "nodes").contains(&"colour".to_owned()));
    raw(&path).execute_batch("DROP TRIGGER refuse").unwrap();
    let re = Document::open_existing(&path).unwrap();
    assert_eq!(colour_of(&re, p), None);
    assert_eq!(re.notebook().get(c).unwrap().body(), "body of c\n");
}
