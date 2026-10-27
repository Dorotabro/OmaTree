//! Reference analysis, explicit pruning and space reclamation (ticket E9b). Everything runs on
//! disposable files and the assertions read the SQLite file itself (rows, exact BLOB bytes,
//! header version, tables), not only what the API says.

use crate::attachment_tests::{header_version, logical, raw, stored, tables, Dir};
use crate::attachments::fixtures::png;
use crate::document::{Document, PruneError, PruneReport, ReclaimError};
use crate::storage::{install_write_guards, remove_write_guards, StorageError};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

fn id_of(reference: &str) -> String {
    reference["assets/".len()..][..32].to_owned()
}

fn rows(path: &Path) -> BTreeMap<String, Vec<u8>> {
    stored(path)
        .into_iter()
        .map(|(id, _, _, data)| (id, data))
        .collect()
}

struct Fx {
    /// id -> original bytes, for every attachment put.
    bytes: BTreeMap<String, Vec<u8>>,
    a: String,
    g: String,
    t_only: String,
    t_cp: String,
    cp_only: String,
    orphan: String,
    pasted: String,
}

impl Fx {
    fn orphans(&self) -> Vec<String> {
        let mut v = vec![self.orphan.clone(), self.pasted.clone()];
        v.sort();
        v
    }

    fn retained(&self) -> Vec<String> {
        let mut v = vec![
            self.a.clone(),
            self.g.clone(),
            self.t_only.clone(),
            self.t_cp.clone(),
            self.cp_only.clone(),
        ];
        v.sort();
        v
    }

    fn size(&self, id: &str) -> u64 {
        self.bytes[id].len() as u64
    }
}

/// A saved notebook with overlapping references:
/// - `a`: an active note (and, as a consequence of how checkpoints are taken, checkpoints);
/// - `g`: two active notes, and checkpoints;
/// - `t_only`: Trash only (its checkpoints are removed with SQL);
/// - `t_cp`: Trash and a checkpoint;
/// - `cp_only`: a retained checkpoint only (the note dropped it afterwards);
/// - `orphan`: put and never referenced;
/// - `pasted`: pasted into a body, saved, then removed from it again.
///
/// Unicode in titles and bodies, and bodies that merely look like references, are included.
fn fixture(path: &Path) -> Fx {
    let mut bytes = BTreeMap::new();
    let mut doc = Document::open(path).unwrap();
    let mut put = |doc: &Document, w: u32, h: u32| {
        let b = png(w, h);
        let r = doc.put_attachment(&b).unwrap();
        let id = id_of(&r);
        bytes.insert(id.clone(), b);
        (id, r)
    };
    let (a, ra) = put(&doc, 3, 3);
    let (g, rg) = put(&doc, 4, 3);
    let (t_only, rt_only) = put(&doc, 5, 3);
    let (t_cp, rt_cp) = put(&doc, 6, 3);
    let (cp_only, rcp) = put(&doc, 7, 3);
    let (orphan, _) = put(&doc, 8, 3);
    let (pasted, rpasted) = put(&doc, 9, 3);

    let r1 = doc.create_root("Příliš žluťoučký 🦀").unwrap();
    let r2 = doc.create_root("Druhá").unwrap();
    let r3 = doc.create_root("Do koše").unwrap();
    let r4 = doc.create_root("Kontrolní bod").unwrap();
    let r5 = doc.create_root("Jen koš").unwrap();
    let r6 = doc.create_root("Vložený").unwrap();
    doc.set_body(r1, &format!("ahoj ![a]({ra}) a ![g]({rg}) žlutý kůň\n"))
        .unwrap();
    doc.set_body(r2, &format!("jiný odkaz ![g]({rg})\n"))
        .unwrap();
    doc.set_body(r3, &format!("![t]({rt_cp})\n")).unwrap();
    doc.set_body(r4, &format!("![c]({rcp})\n")).unwrap();
    doc.set_body(r5, &format!("![t]({rt_only})\n")).unwrap();
    doc.save().unwrap();
    doc.delete(r5).unwrap();
    doc.delete(r3).unwrap();
    doc.create_checkpoint("keep");
    doc.set_body(r4, "nic\n").unwrap();
    doc.save().unwrap();
    doc.set_body(r6, &format!("![p]({rpasted})\n")).unwrap();
    doc.save().unwrap();
    doc.set_body(r6, "").unwrap();
    doc.save().unwrap();
    assert!(!doc.is_dirty());
    drop(doc);

    // `t_only` must be referenced by Trash alone: drop the checkpoints that hold it.
    let c = raw(path);
    c.execute(
        &format!(
            "DELETE FROM checkpoints WHERE id IN
             (SELECT checkpoint_id FROM checkpoint_nodes WHERE body LIKE '%{t_only}%')"
        ),
        [],
    )
    .unwrap();
    Fx {
        bytes,
        a,
        g,
        t_only,
        t_cp,
        cp_only,
        orphan,
        pasted,
    }
}

fn eligible_ids(r: &PruneReport) -> Vec<String> {
    r.eligible.iter().map(|e| e.id.to_string()).collect()
}

fn preview(doc: &Document) -> crate::document::PrunePreview {
    doc.preview_attachment_prune().unwrap()
}

// --- Reference analysis ------------------------------------------------------------------------

#[test]
fn every_kind_of_reference_is_found_and_the_categories_overlap_honestly() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let fx = fixture(&p);
    let doc = Document::open_existing(&p).unwrap();
    let r = preview(&doc).report;

    assert_eq!(r.stored.count, 7);
    assert_eq!(
        r.stored.bytes,
        fx.bytes.values().map(|b| b.len() as u64).sum::<u64>()
    );
    // Active: a and g, each counted once although g is mentioned by two notes.
    assert_eq!(r.in_active.count, 2);
    assert_eq!(r.in_active.bytes, fx.size(&fx.a) + fx.size(&fx.g));
    assert_eq!(r.in_trash.count, 2);
    assert_eq!(r.in_trash.bytes, fx.size(&fx.t_only) + fx.size(&fx.t_cp));
    // Checkpoints hold the active images too (a checkpoint is a copy of the notebook).
    assert_eq!(r.in_checkpoints.count, 4);
    assert!(
        r.in_active.count + r.in_trash.count + r.in_checkpoints.count > r.retained.count,
        "the categories overlap and must never be added up"
    );
    assert_eq!(r.retained.count, 5);
    assert_eq!(eligible_ids(&r), fx.orphans());
    assert_eq!(
        r.eligible_total.bytes,
        fx.size(&fx.orphan) + fx.size(&fx.pasted)
    );
    assert_eq!(r.retained.count + r.eligible_total.count, r.stored.count);
    assert_eq!(r.retained.bytes + r.eligible_total.bytes, r.stored.bytes);
    assert_eq!(r.missing_references, 0);
    assert_eq!(r.pending.count, 0);
    assert!(!r.requires_save);
    assert_eq!(
        r.file_size_bytes,
        Some(std::fs::metadata(&p).unwrap().len())
    );
}

#[test]
fn each_recovery_only_reference_protects_by_itself() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let fx = fixture(&p);
    let doc = Document::open_existing(&p).unwrap();
    let r = preview(&doc).report;
    let ids = eligible_ids(&r);
    for protected in [&fx.t_only, &fx.t_cp, &fx.cp_only] {
        assert!(!ids.contains(protected), "{protected} is still recoverable");
    }
    // `t_only` is in Trash and in no checkpoint; `cp_only` in checkpoints and neither of the others.
    let c = raw(&p);
    let n = |sql: &str| -> i64 { c.query_row(sql, [], |r| r.get(0)).unwrap() };
    assert_eq!(
        n(&format!(
            "SELECT count(*) FROM checkpoint_nodes WHERE body LIKE '%{}%'",
            fx.t_only
        )),
        0
    );
    assert_eq!(
        n(&format!(
            "SELECT count(*) FROM nodes WHERE body LIKE '%{}%'",
            fx.cp_only
        )),
        0
    );
    assert_eq!(
        n(&format!(
            "SELECT count(*) FROM trash_nodes WHERE body LIKE '%{}%'",
            fx.cp_only
        )),
        0
    );
}

#[test]
fn references_in_code_blocks_comments_and_odd_text_still_protect() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    let r: Vec<String> = (1..=4)
        .map(|n| doc.put_attachment(&png(n, 2)).unwrap())
        .collect();
    let n = doc.create_root("n").unwrap();
    doc.set_body(
        n,
        &format!(
            "```\n{}\n```\n<!-- {} -->\nžluťoučký🦀{}🦀 `{}` ",
            r[0], r[1], r[2], r[3]
        ),
    )
    .unwrap();
    doc.save().unwrap();
    let rep = preview(&doc).report;
    assert_eq!(rep.stored.count, 4);
    assert!(rep.eligible.is_empty(), "{:?}", rep.eligible);
}

#[test]
fn missing_invalid_and_partial_references_delete_nothing_else() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    let kept = doc.put_attachment(&png(2, 2)).unwrap();
    let orphan = doc.put_attachment(&png(3, 2)).unwrap();
    let n = doc.create_root("n").unwrap();
    let kid = id_of(&kept);
    doc.set_body(
        n,
        &format!(
            "{kept}\nassets/ffffffffffffffffffffffffffffffff.png\n\
             assets/{}.png\nassets/{}.PNG\nassets/{kid}0.png\nassets/{}.jpeg\n\
             ../assets/{}/x.png\nhttps://example.com/assets/{}.png.\n",
            &kid[..31],
            kid.to_uppercase(),
            &kid[..31],
            id_of(&orphan),
            &id_of(&orphan)[..31],
        ),
    )
    .unwrap();
    doc.save().unwrap();
    let r = preview(&doc).report;
    // A reference to something not stored changes nothing but the count of missing ones. The
    // malformed lookalikes of `kept` and `orphan` never protect them, nor resolve to them.
    assert_eq!(r.missing_references, 1);
    assert_eq!(eligible_ids(&r), vec![id_of(&orphan)]);
    // The one well formed reference that does exist in https text protects its own id only.
    assert_eq!(r.retained.count, 1);
}

#[test]
fn a_pending_attachment_is_reported_never_prunable_and_blocks_pruning() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let fx = fixture(&p);
    let mut doc = Document::open_existing(&p).unwrap();
    let pending = png(11, 3);
    let r = doc.put_attachment(&pending).unwrap();
    let n = doc.notebook().roots()[0].id();
    doc.set_body(n, &format!("![x]({r})")).unwrap();
    let pv = preview(&doc);
    assert_eq!(pv.report.pending.count, 1);
    assert_eq!(pv.report.pending.bytes, pending.len() as u64);
    assert_eq!(pv.report.stored.count, 7, "pending is not stored");
    assert!(pv.report.requires_save);
    assert_eq!(pv.report.missing_references, 0, "the pending id is known");
    assert!(!eligible_ids(&pv.report).contains(&id_of(&r)));
    let before = rows(&p);
    assert!(matches!(
        doc.prune_attachments(&pv.token),
        Err(PruneError::NeedsSave)
    ));
    assert_eq!(rows(&p), before);
    assert_eq!(doc.pending_attachments().0, 1, "still pending");
    assert!(doc.is_dirty());
    assert_eq!(doc.attachment(&r).unwrap(), pending);
    let _ = fx;
}

#[test]
fn unsaved_edits_that_add_or_drop_references_are_both_accounted_for() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let fx = fixture(&p);
    let mut doc = Document::open_existing(&p).unwrap();
    // Unsaved: the orphan gains a reference; `a` loses its only active one.
    let r1 = doc.notebook().roots()[0].id();
    let orphan_ref = format!("assets/{}.png", fx.orphan);
    doc.set_body(r1, &format!("![o]({orphan_ref}) ![g](assets/{}.png)", fx.g))
        .unwrap();
    let rep = preview(&doc).report;
    assert!(rep.requires_save);
    // The orphan is now mentioned only by an unsaved body ...
    assert_eq!(rep.only_in_unsaved_changes.count, 1);
    assert_eq!(rep.only_in_unsaved_changes.bytes, fx.size(&fx.orphan));
    // ... and `a` only by what the file has (and by checkpoints, which hold it too).
    assert!(!eligible_ids(&rep).contains(&fx.orphan));
    assert!(!eligible_ids(&rep).contains(&fx.a));
    assert_eq!(eligible_ids(&rep), vec![fx.pasted.clone()]);
    // After saving, the file agrees: `a` is still held by checkpoints, nothing new is eligible.
    doc.save().unwrap();
    let rep = preview(&doc).report;
    assert_eq!(eligible_ids(&rep), vec![fx.pasted.clone()]);
    assert_eq!(rep.only_in_unsaved_changes.count, 0);
    assert_eq!(rep.only_in_saved_file.count, 0);
}

#[test]
fn unsaved_trash_and_checkpoint_changes_never_unprotect_what_the_file_holds() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let fx = fixture(&p);
    let mut doc = Document::open_existing(&p).unwrap();
    // Evict every old checkpoint in memory (the oldest go first, so `cp_only` is dropped).
    for i in 0..crate::recovery::MAX_CHECKPOINTS + 1 {
        doc.create_checkpoint(&format!("c{i}"));
    }
    assert!(!doc
        .recovery()
        .checkpoints()
        .iter()
        .any(|c| c.reason() == "keep"));
    // Also delete the note that holds `a` and `g` (they move to Trash in memory).
    let r1 = doc.notebook().roots()[0].id();
    doc.delete(r1).unwrap();
    let rep = preview(&doc).report;
    assert!(rep.requires_save);
    // The file still holds `cp_only` in its checkpoints: protected until the save lands.
    assert!(!eligible_ids(&rep).contains(&fx.cp_only));
    assert!(rep.only_in_saved_file.count >= 1);
    assert!(rep.in_trash.count >= 4, "a, g moved to Trash in memory");
    doc.save().unwrap();
    let rep = preview(&doc).report;
    // Now that the eviction is durable, the checkpoint-only image has no reference left.
    assert!(eligible_ids(&rep).contains(&fx.cp_only));
    assert_eq!(rep.only_in_saved_file.count, 0);
}

// --- The preview writes nothing -----------------------------------------------------------------

#[test]
fn a_preview_changes_neither_the_file_nor_the_document() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    fixture(&p);
    let mut doc = Document::open_existing(&p).unwrap();
    let before = std::fs::read(&p).unwrap();
    install_write_guards(
        &p,
        &[
            "nodes",
            "trash_nodes",
            "checkpoint_nodes",
            "attachments",
            "notebook_meta",
        ],
    );
    doc.resync_for_test();
    let guarded = std::fs::read(&p).unwrap();
    let (dirty, pending) = (doc.is_dirty(), doc.pending_attachments());
    for _ in 0..3 {
        preview(&doc);
        doc.preview_reclaim().unwrap();
    }
    assert_eq!(std::fs::read(&p).unwrap(), guarded, "byte for byte");
    assert_ne!(
        before, guarded,
        "(the guards themselves are the only difference)"
    );
    assert_eq!(
        (doc.is_dirty(), doc.pending_attachments()),
        (dirty, pending)
    );
    // And it never saved or migrated either.
    assert_eq!(header_version(&p), 4);
}

#[test]
fn an_attachment_free_v3_notebook_is_untouched_by_preview_and_noop_prune() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    {
        let mut doc = Document::open(&p).unwrap();
        let n = doc.create_root("jen text").unwrap();
        doc.set_body(n, "žádný obrázek\n").unwrap();
        doc.delete(n).unwrap();
        doc.save().unwrap();
    }
    assert_eq!(header_version(&p), 3);
    let tables_before = tables(&p);
    assert_eq!(tables_before.len(), 7);
    let bytes = std::fs::read(&p).unwrap();
    let mut doc = Document::open_existing(&p).unwrap();
    let pv = preview(&doc);
    assert_eq!(pv.report.stored.count, 0);
    assert!(pv.report.eligible.is_empty());
    let res = doc.prune_attachments(&pv.token).unwrap();
    assert!(!res.wrote);
    assert_eq!(res.removed.count, 0);
    assert!(res.warning.is_none());
    assert_eq!(std::fs::read(&p).unwrap(), bytes);
    assert_eq!(header_version(&p), 3);
    assert_eq!(tables(&p), tables_before);
}

// --- Authorization -------------------------------------------------------------------------------

#[test]
fn a_preview_expires_when_a_note_is_edited() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let fx = fixture(&p);
    let mut doc = Document::open_existing(&p).unwrap();
    let pv = preview(&doc);
    let r1 = doc.notebook().roots()[0].id();
    doc.set_body(r1, "jiný text bez odkazu\n").unwrap();
    let before = rows(&p);
    // Unsaved: told to save first, not silently saved.
    assert!(matches!(
        doc.prune_attachments(&pv.token),
        Err(PruneError::NeedsSave)
    ));
    doc.save().unwrap();
    // Saved: the same edit now makes the old preview stale.
    assert!(matches!(
        doc.prune_attachments(&pv.token),
        Err(PruneError::Stale)
    ));
    assert_eq!(rows(&p), before);
    // A new preview works and `a` is still held by recovery history.
    let pv2 = preview(&doc);
    assert!(!eligible_ids(&pv2.report).contains(&fx.a));
}

#[test]
fn a_preview_expires_when_notes_are_deleted_or_restored() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    fixture(&p);
    let mut doc = Document::open_existing(&p).unwrap();
    let pv = preview(&doc);
    let r2 = doc.notebook().roots()[1].id();
    doc.delete(r2).unwrap();
    doc.save().unwrap();
    assert!(matches!(
        doc.prune_attachments(&pv.token),
        Err(PruneError::Stale)
    ));
    let pv = preview(&doc);
    doc.restore_trash(doc.recovery().trash().len() - 1).unwrap();
    doc.save().unwrap();
    assert!(matches!(
        doc.prune_attachments(&pv.token),
        Err(PruneError::Stale)
    ));
    assert_eq!(stored(&p).len(), 7, "nothing was deleted");
}

#[test]
fn a_preview_expires_when_checkpoints_change() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    fixture(&p);
    let mut doc = Document::open_existing(&p).unwrap();
    let pv = preview(&doc);
    doc.create_checkpoint("new");
    doc.save().unwrap();
    assert!(matches!(
        doc.prune_attachments(&pv.token),
        Err(PruneError::Stale)
    ));
    let pv = preview(&doc);
    doc.restore_checkpoint(0).unwrap();
    doc.save().unwrap();
    assert!(matches!(
        doc.prune_attachments(&pv.token),
        Err(PruneError::Stale)
    ));
    assert_eq!(stored(&p).len(), 7);
}

#[test]
fn a_preview_expires_when_an_attachment_is_added() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    fixture(&p);
    let mut doc = Document::open_existing(&p).unwrap();
    let pv = preview(&doc);
    let r = doc.put_attachment(&png(12, 3)).unwrap();
    assert!(matches!(
        doc.prune_attachments(&pv.token),
        Err(PruneError::NeedsSave)
    ));
    assert_eq!(doc.pending_attachments().0, 1);
    doc.save().unwrap();
    assert!(matches!(
        doc.prune_attachments(&pv.token),
        Err(PruneError::Stale)
    ));
    // The new, still unreferenced image shows up in the next preview, and in it alone.
    assert!(eligible_ids(&preview(&doc).report).contains(&id_of(&r)));
}

#[test]
fn an_external_write_after_the_preview_aborts_the_prune() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let fx = fixture(&p);
    let mut doc = Document::open_existing(&p).unwrap();
    let pv = preview(&doc);
    // Someone else commits something the preview did not depend on: the connection guard stops it.
    raw(&p).execute_batch("CREATE TABLE elsewhere (x)").unwrap();
    let before = rows(&p);
    match doc.prune_attachments(&pv.token) {
        Err(e) => assert!(e.is_conflict(), "{e}"),
        Ok(_) => panic!("must not delete after an external write"),
    }
    assert_eq!(rows(&p), before);
    assert_eq!(rows(&p).len(), 7);
    // Someone else changes what the preview did depend on (an extra orphan): stale instead.
    doc.resync_for_test();
    let pv = preview(&doc);
    let extra = "abababababababababababababababab";
    raw(&p)
        .execute(
            "INSERT INTO attachments (id, media_type, created_at, byte_len, data)
             VALUES (?1, 'image/png', 0, 3, x'010203')",
            [extra],
        )
        .unwrap();
    doc.resync_for_test();
    assert!(matches!(
        doc.prune_attachments(&pv.token),
        Err(PruneError::Stale)
    ));
    assert!(rows(&p).contains_key(extra));
    for id in fx.orphans() {
        assert!(rows(&p).contains_key(&id));
    }
}

#[test]
fn another_connection_adding_a_reference_before_the_delete_is_never_overridden() {
    // The reference appears between the preview and the prune, in the file, with the write
    // guard unable to tell the difference when we resync. The in-transaction recheck still
    // refuses: the set it derives is not the reviewed one.
    let d = Dir::new();
    let p = d.join("n.omatree");
    let fx = fixture(&p);
    let mut doc = Document::open_existing(&p).unwrap();
    let pv = preview(&doc);
    raw(&p)
        .execute(
            "UPDATE nodes SET body = body || ?1 WHERE id = (SELECT min(id) FROM nodes)",
            [format!("assets/{}.png", fx.orphan)],
        )
        .unwrap();
    doc.resync_for_test();
    // The in-memory notebook does not have the reference, but the file does.
    let r = doc.prune_attachments(&pv.token);
    assert!(matches!(r, Err(PruneError::Stale)), "{r:?}");
    assert!(rows(&p).contains_key(&fx.orphan));
}

// --- Pruning ------------------------------------------------------------------------------------

#[test]
fn only_genuine_orphans_are_deleted_and_everything_else_is_byte_identical() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let fx = fixture(&p);
    let logical_before = logical(&p);
    let mut doc = Document::open_existing(&p).unwrap();
    let pv = preview(&doc);
    let size_before = std::fs::metadata(&p).unwrap().len();
    let res = doc.prune_attachments(&pv.token).unwrap();

    assert!(res.wrote);
    assert_eq!(res.removed.count, 2);
    assert_eq!(res.removed.bytes, fx.size(&fx.orphan) + fx.size(&fx.pasted));
    assert_eq!(res.retained.count, 5);
    assert_eq!(
        res.retained.bytes,
        fx.retained().iter().map(|i| fx.size(i)).sum::<u64>()
    );
    assert!(res.warning.is_some());
    assert_eq!(res.file_size_before, Some(size_before));

    let after = rows(&p);
    assert_eq!(after.keys().cloned().collect::<Vec<_>>(), fx.retained());
    for (id, data) in &after {
        assert_eq!(data, &fx.bytes[id], "exact bytes of {id}");
    }
    assert_eq!(
        logical(&p),
        logical_before,
        "no note, Trash or checkpoint row changed"
    );
    assert_eq!(header_version(&p), 4);
    assert!(!doc.is_dirty());
    // Retained attachments are still served by the document.
    for id in fx.retained() {
        let r = format!("assets/{id}.png");
        assert_eq!(doc.attachment(&r).unwrap(), fx.bytes[&id]);
    }
    // A second preview agrees there is nothing left, and a second prune is a no-op.
    let pv = preview(&doc);
    assert!(pv.report.eligible.is_empty());
    assert!(!doc.prune_attachments(&pv.token).unwrap().wrote);
}

#[test]
fn recovery_history_alone_keeps_attachments_alive_through_a_prune() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let fx = fixture(&p);
    let mut doc = Document::open_existing(&p).unwrap();
    let pv = preview(&doc);
    doc.prune_attachments(&pv.token).unwrap();
    for id in [&fx.t_only, &fx.t_cp, &fx.cp_only] {
        assert!(rows(&p).contains_key(id));
    }
    // Restoring from Trash after the prune finds its images.
    let last = doc.recovery().trash().len() - 1;
    let root = doc.restore_trash(last).unwrap();
    let body = doc.notebook().get(root).unwrap().body().to_owned();
    let reference = body
        .trim()
        .trim_start_matches("![t](")
        .trim_end_matches(')');
    assert!(doc.attachment(reference).is_ok(), "{reference}");
    // So does restoring the checkpoint that holds `cp_only`.
    let idx = doc
        .recovery()
        .checkpoints()
        .iter()
        .position(|c| c.reason() == "keep")
        .unwrap();
    doc.restore_checkpoint(idx).unwrap();
    let all: String = doc
        .notebook()
        .depth_first()
        .iter()
        .map(|n| n.body().to_owned())
        .collect();
    assert!(all.contains(&fx.cp_only));
    assert!(doc
        .attachment(&format!("assets/{}.png", fx.cp_only))
        .is_ok());
    doc.save().unwrap();
    let again = Document::open_existing(&p).unwrap();
    assert!(again
        .attachment(&format!("assets/{}.png", fx.cp_only))
        .is_ok());
}

#[test]
fn deleting_a_note_moves_its_attachments_to_recovery_and_a_prune_keeps_them() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    let img = png(5, 5);
    let r = doc.put_attachment(&img).unwrap();
    let n = doc.create_root("n").unwrap();
    doc.set_body(n, &format!("![x]({r})")).unwrap();
    doc.save().unwrap();
    doc.delete(n).unwrap();
    doc.save().unwrap();
    let pv = preview(&doc);
    assert_eq!(pv.report.in_active.count, 0);
    assert_eq!(pv.report.in_trash.count, 1);
    assert!(pv.report.eligible.is_empty());
    assert!(!doc.prune_attachments(&pv.token).unwrap().wrote);
    assert_eq!(rows(&p)[&id_of(&r)], img);
}

#[test]
fn an_injected_failure_part_way_rolls_back_every_deletion() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let fx = fixture(&p);
    let mut doc = Document::open_existing(&p).unwrap();
    let pv = preview(&doc);
    // The first orphan deletes fine; the second one aborts.
    let orphans = fx.orphans();
    raw(&p)
        .execute_batch(&format!(
            "CREATE TRIGGER boom BEFORE DELETE ON attachments WHEN old.id = '{}'
             BEGIN SELECT RAISE(ABORT, 'injected'); END;",
            orphans[1]
        ))
        .unwrap();
    doc.resync_for_test();
    let before = rows(&p);
    let logical_before = logical(&p);
    let err = doc.prune_attachments(&pv.token).unwrap_err();
    assert!(
        matches!(err, PruneError::Storage(StorageError::Sqlite(_))),
        "{err}"
    );
    assert_eq!(rows(&p), before, "the first deletion was rolled back too");
    assert_eq!(logical(&p), logical_before);
    assert_eq!(header_version(&p), 4);
    // The document is still usable and clean, and a normal save is not refused.
    assert!(!doc.is_dirty());
    let r1 = doc.notebook().roots()[0].id();
    doc.set_body(r1, "po chybě\n").unwrap();
    doc.save().unwrap();
    // Once the obstacle is gone the same review can be repeated and then succeeds.
    remove_write_guards(&p);
    doc.resync_for_test();
    let pv = preview(&doc);
    assert_eq!(doc.prune_attachments(&pv.token).unwrap().removed.count, 2);
}

#[test]
fn a_refused_prune_keeps_pending_attachments_and_dirty_flags() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    fixture(&p);
    let mut doc = Document::open_existing(&p).unwrap();
    let pv = preview(&doc);
    let pending = png(13, 3);
    let r = doc.put_attachment(&pending).unwrap();
    let r1 = doc.notebook().roots()[0].id();
    doc.set_body(r1, "změna\n").unwrap();
    doc.create_checkpoint("ručně");
    for _ in 0..2 {
        assert!(matches!(
            doc.prune_attachments(&pv.token),
            Err(PruneError::NeedsSave)
        ));
    }
    assert!(doc.is_dirty());
    assert_eq!(doc.pending_attachments(), (1, pending.len() as u64));
    assert_eq!(doc.attachment(&r).unwrap(), pending);
    assert_eq!(doc.notebook().get(r1).unwrap().body(), "změna\n");
    // The save is still the caller's, and it lands everything.
    doc.save().unwrap();
    assert!(rows(&p).contains_key(&id_of(&r)));
}

#[test]
fn a_noop_prune_performs_no_write_at_all() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let fx = fixture(&p);
    let mut doc = Document::open_existing(&p).unwrap();
    let pv = preview(&doc);
    doc.prune_attachments(&pv.token).unwrap();
    // No orphan is left. Make any write to any table fail, then ask again.
    install_write_guards(
        &p,
        &[
            "nodes",
            "trash_nodes",
            "checkpoint_nodes",
            "attachments",
            "notebook_meta",
            "trash_entries",
            "checkpoints",
        ],
    );
    doc.resync_for_test();
    let guarded = std::fs::read(&p).unwrap();
    let pv = preview(&doc);
    let res = doc.prune_attachments(&pv.token).unwrap();
    assert!(!res.wrote && res.removed.count == 0);
    assert_eq!(res.retained.count, fx.retained().len());
    assert_eq!(std::fs::read(&p).unwrap(), guarded);
}

#[test]
fn saving_and_save_as_never_delete_an_attachment() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let fx = fixture(&p);
    let mut doc = Document::open_existing(&p).unwrap();
    let r1 = doc.notebook().roots()[0].id();
    doc.set_body(r1, "bez odkazu\n").unwrap();
    doc.save().unwrap(); // active-only save
    doc.create_checkpoint("a");
    doc.save().unwrap(); // full save
    assert_eq!(rows(&p).len(), 7);
    let q = d.join("copy.omatree");
    doc.save_as(&q, false).unwrap();
    assert_eq!(rows(&q).len(), 7);
    for id in fx.orphans() {
        assert_eq!(rows(&q)[&id], fx.bytes[&id]);
    }
    // The sidebar of the old file is untouched too.
    assert_eq!(rows(&p).len(), 7);
}

#[test]
fn save_as_after_pruning_copies_only_what_is_left() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let fx = fixture(&p);
    let mut doc = Document::open_existing(&p).unwrap();
    let pv = preview(&doc);
    doc.prune_attachments(&pv.token).unwrap();
    let q = d.join("copy.omatree");
    doc.save_as(&q, false).unwrap();
    assert_eq!(rows(&q).keys().cloned().collect::<Vec<_>>(), fx.retained());
    for (id, data) in rows(&q) {
        assert_eq!(data, fx.bytes[&id]);
    }
    // The copy is a working notebook: it can be previewed and finds nothing to prune.
    let doc = Document::open_existing(&q).unwrap();
    assert!(preview(&doc).report.eligible.is_empty());
}

#[test]
fn the_notebook_reopens_after_pruning_with_everything_intact() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let fx = fixture(&p);
    let logical_before = logical(&p);
    {
        let mut doc = Document::open_existing(&p).unwrap();
        let pv = preview(&doc);
        doc.prune_attachments(&pv.token).unwrap();
    }
    let doc = Document::open_existing(&p).unwrap();
    assert_eq!(doc.schema_version(), Some(4));
    assert_eq!(logical(&p), logical_before);
    let ids = doc.stored_attachment_ids().unwrap();
    assert_eq!(ids, fx.retained());
    for id in ids {
        assert_eq!(
            doc.attachment(&format!("assets/{id}.png")).unwrap(),
            fx.bytes[&id]
        );
    }
    let qt_style = raw(&p);
    let ok: String = qt_style
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    assert_eq!(ok, "ok");
}

#[test]
fn an_ordinary_save_never_touches_the_attachments_table() {
    // What the Qt build does: it never calls the attachment API, it only saves.
    let d = Dir::new();
    let p = d.join("n.omatree");
    fixture(&p);
    let mut doc = Document::open_existing(&p).unwrap();
    install_write_guards(&p, &["attachments"]);
    doc.resync_for_test();
    let r1 = doc.notebook().roots()[0].id();
    doc.set_body(r1, "Qt save\n").unwrap();
    doc.save().unwrap();
    doc.create_checkpoint("Qt");
    doc.save().unwrap();
    remove_write_guards(&p);
    assert_eq!(rows(&p).len(), 7);
}

// --- Space reclamation --------------------------------------------------------------------------

/// A notebook whose attachments are big enough for the file size to be meaningful.
fn big_orphans(path: &Path) -> Document {
    let mut doc = Document::open(path).unwrap();
    let keep = doc.put_attachment(&png(40, 40)).unwrap();
    for n in 0..6 {
        doc.put_attachment(&png(200 + n, 200)).unwrap();
    }
    let n = doc.create_root("n").unwrap();
    doc.set_body(n, &format!("![k]({keep})")).unwrap();
    doc.save().unwrap();
    doc
}

#[test]
fn pruning_never_shrinks_the_file_and_reclaiming_is_a_separate_authorized_step() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let mut doc = big_orphans(&p);
    let size0 = std::fs::metadata(&p).unwrap().len();
    let pv = preview(&doc);
    assert_eq!(pv.report.eligible.len(), 6);
    let res = doc.prune_attachments(&pv.token).unwrap();
    // Free pages stay inside the file: the size is exactly what it was.
    assert_eq!(std::fs::metadata(&p).unwrap().len(), size0);
    assert_eq!(res.file_size_after, Some(size0));
    assert!(res.warning.unwrap().contains("not smaller"));

    let rp = doc.preview_reclaim().unwrap();
    assert!(!rp.nothing_to_reclaim);
    assert!(
        rp.free_bytes >= res.removed.bytes,
        "the freed BLOB pages are free pages"
    );
    assert_eq!(rp.file_size_bytes, size0);
    assert_eq!(rp.journal_mode, "delete");
    let rp = doc.preview_reclaim().unwrap();
    let before = rows(&p);
    let r = doc.reclaim_space(&rp.token).unwrap();
    assert!(r.ran);
    let actual = std::fs::metadata(&p).unwrap().len();
    assert_eq!(
        r.size_after, actual,
        "the real size on disk, not an estimate"
    );
    assert_eq!(r.size_before, rp.file_size_bytes);
    assert_eq!(
        r.size_difference(),
        rp.file_size_bytes as i64 - actual as i64
    );
    assert!(actual < size0, "{actual} < {size0}");
    assert_eq!(r.free_bytes_after, 0);
    // Contents and schema are preserved exactly.
    assert_eq!(rows(&p), before);
    assert_eq!(header_version(&p), 4);
    assert_eq!(tables(&p).len(), 8);
    let ok: String = raw(&p)
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    assert_eq!(ok, "ok");
}

#[test]
fn a_stale_reclaim_token_is_refused_and_nothing_is_rewritten() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let mut doc = big_orphans(&p);
    let pv = preview(&doc);
    doc.prune_attachments(&pv.token).unwrap();
    let rp = doc.preview_reclaim().unwrap();
    let before = std::fs::read(&p).unwrap();
    // Free more pages: the file's page statistics differ from what was reviewed.
    let n = doc.notebook().roots()[0].id();
    doc.set_body(n, &"z".repeat(3_000_000)).unwrap();
    doc.save().unwrap();
    let after_edit = std::fs::read(&p).unwrap();
    assert_ne!(before, after_edit);
    assert!(matches!(
        doc.reclaim_space(&rp.token),
        Err(ReclaimError::Stale)
    ));
    assert_eq!(std::fs::read(&p).unwrap(), after_edit);
}

#[test]
fn reclaiming_with_no_free_pages_does_nothing() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    doc.create_root("n").unwrap();
    doc.save().unwrap();
    let rp = doc.preview_reclaim().unwrap();
    assert!(rp.nothing_to_reclaim);
    let bytes = std::fs::read(&p).unwrap();
    let r = doc.reclaim_space(&rp.token).unwrap();
    assert!(!r.ran);
    assert_eq!(r.size_difference(), 0);
    assert_eq!(std::fs::read(&p).unwrap(), bytes);
}

#[test]
fn reclaiming_uses_the_documents_own_connection_and_leaves_its_baseline_valid() {
    // If a second connection had written, this document's own next save would be refused as an
    // external modification. Neither prune nor reclaim causes that.
    let d = Dir::new();
    let p = d.join("n.omatree");
    let mut doc = big_orphans(&p);
    let pv = preview(&doc);
    doc.prune_attachments(&pv.token).unwrap();
    let n = doc.notebook().roots()[0].id();
    doc.set_body(n, "po prune\n").unwrap();
    doc.save().unwrap();
    let rp = doc.preview_reclaim().unwrap();
    assert!(doc.reclaim_space(&rp.token).unwrap().ran);
    doc.set_body(n, "po reclaim\n").unwrap();
    doc.save().unwrap();
    doc.create_checkpoint("x");
    doc.save().unwrap();
    assert_eq!(
        Document::open_existing(&p)
            .unwrap()
            .notebook()
            .get(n)
            .unwrap()
            .body(),
        "po reclaim\n"
    );
}

#[test]
fn reclaiming_is_refused_after_an_external_write_and_the_next_save_is_too() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let mut doc = big_orphans(&p);
    let pv = preview(&doc);
    doc.prune_attachments(&pv.token).unwrap();
    let rp = doc.preview_reclaim().unwrap();
    raw(&p).execute_batch("CREATE TABLE elsewhere (x)").unwrap();
    let before = std::fs::read(&p).unwrap();
    match doc.reclaim_space(&rp.token) {
        Err(e) => assert!(e.is_conflict() || matches!(e, ReclaimError::Stale), "{e}"),
        Ok(_) => panic!("must not compact over an external change"),
    }
    assert_eq!(std::fs::read(&p).unwrap(), before);
    // The conflict guard of saving is unchanged: still refused, nothing overwritten.
    let n = doc.notebook().roots()[0].id();
    doc.set_body(n, "moje\n").unwrap();
    assert!(doc.save().unwrap_err().is_conflict());
    assert_eq!(std::fs::read(&p).unwrap(), before);
}

#[test]
fn lock_contention_makes_reclaiming_fail_cleanly_and_a_retry_succeeds() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let mut doc = big_orphans(&p);
    let pv = preview(&doc);
    doc.prune_attachments(&pv.token).unwrap();
    let rp = doc.preview_reclaim().unwrap();
    let before = std::fs::read(&p).unwrap();
    let other = raw(&p);
    other.execute_batch("BEGIN EXCLUSIVE").unwrap();
    let err = doc
        .reclaim_space_waiting(&rp.token, Duration::from_millis(50))
        .unwrap_err();
    assert!(matches!(err, ReclaimError::Busy), "{err}");
    other.execute_batch("ROLLBACK").unwrap();
    drop(other);
    assert_eq!(std::fs::read(&p).unwrap(), before, "nothing changed");
    // Nobody wrote, so the same review is still good.
    let r = doc.reclaim_space(&rp.token).unwrap();
    assert!(r.ran && r.size_after < r.size_before);
}

#[test]
fn a_reclaim_that_cannot_write_leaves_a_valid_readable_notebook() {
    // The directory refuses new files (the rollback journal cannot be created), as a full or
    // read-only disk would.
    use std::os::unix::fs::PermissionsExt;
    let d = Dir::new();
    let p = d.join("n.omatree");
    let mut doc = big_orphans(&p);
    let pv = preview(&doc);
    doc.prune_attachments(&pv.token).unwrap();
    let rp = doc.preview_reclaim().unwrap();
    let before = std::fs::read(&p).unwrap();
    std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
    let result = doc.reclaim_space(&rp.token);
    std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    if result.is_ok() {
        // Running as a user that ignores directory permissions: the injection did not apply.
        eprintln!("directory permissions are not enforced here; failure not injected");
        return;
    }
    assert!(result.is_err());
    assert_eq!(std::fs::read(&p).unwrap(), before);
    let again = Document::open_existing(&p).unwrap();
    assert_eq!(again.stored_attachment_ids().unwrap().len(), 1);
    // And once the obstacle is gone, the notebook still works and can reclaim.
    // The failed attempt must not leave this document believing the file was changed by someone
    // else (SQLite counts a failed VACUUM in data_version), so an ordinary save still works.
    let n = doc.notebook().roots()[0].id();
    doc.set_body(n, "po neúspěchu\n").unwrap();
    doc.save().unwrap();
    let rp = doc.preview_reclaim().unwrap();
    let retry = doc.reclaim_space(&rp.token);
    assert!(retry.is_ok(), "{retry:?}");
}

#[test]
fn a_notebook_in_wal_mode_is_reclaimed_and_checkpointed() {
    let d = Dir::new();
    let p = d.join("n.omatree");
    let mut doc = big_orphans(&p);
    let pv = preview(&doc);
    doc.prune_attachments(&pv.token).unwrap();
    drop(doc);
    // Another tool switched the file to WAL (persistent).
    let mode: String = raw(&p)
        .query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
    let mut doc = Document::open_existing(&p).unwrap();
    let rp = doc.preview_reclaim().unwrap();
    assert_eq!(rp.journal_mode, "wal");
    let r = doc.reclaim_space(&rp.token).unwrap();
    assert!(r.ran);
    assert!(r.size_after < r.size_before, "{r:?}");
    assert_eq!(doc.stored_attachment_ids().unwrap().len(), 1);
}

#[test]
fn an_untitled_notebook_has_nothing_to_prune_or_reclaim() {
    let doc = Document::untitled();
    doc.put_attachment(&png(2, 2)).unwrap();
    assert!(matches!(
        doc.preview_attachment_prune(),
        Err(PruneError::NoFile)
    ));
    assert!(matches!(doc.preview_reclaim(), Err(ReclaimError::NoFile)));
    assert_eq!(doc.pending_attachments().0, 1);
    assert!(doc.is_dirty());
}

#[test]
fn a_prune_preview_cannot_be_replayed_against_another_notebook() {
    let d = Dir::new();
    let (p, q) = (d.join("a.omatree"), d.join("b.omatree"));
    fixture(&p);
    std::fs::copy(&p, &q).unwrap();
    let a = Document::open_existing(&p).unwrap();
    let mut b = Document::open_existing(&q).unwrap();
    let pv = preview(&a);
    // Same content, different file: bound to the path.
    assert!(matches!(
        b.prune_attachments(&pv.token),
        Err(PruneError::Stale)
    ));
    assert_eq!(rows(&q).len(), 7);
}

// --- Scale --------------------------------------------------------------------------------------

/// Thousands of notes, hundreds of attachments, with Trash and checkpoints.
fn large(
    path: &Path,
    notes: usize,
    images: usize,
    kept_every: usize,
    side: u32,
) -> (Vec<String>, Vec<String>) {
    let mut doc = Document::open(path).unwrap();
    let mut referenced = Vec::new();
    let mut orphans = Vec::new();
    let mut refs = Vec::new();
    for i in 0..images {
        let r = doc
            .put_attachment(&png(side + (i % 20) as u32, side))
            .unwrap();
        if i % kept_every == 0 {
            referenced.push(id_of(&r));
            refs.push(r);
        } else {
            orphans.push(id_of(&r));
        }
    }
    let root = doc.create_root("kořen").unwrap();
    for i in 0..notes {
        let n = doc.create_child(root, &format!("Poznámka {i} 🦀")).unwrap();
        let mut body = format!("Text {i} žluťoučký kůň\n").repeat(8);
        if !refs.is_empty() && i % 7 == 0 {
            body.push_str(&format!("![x]({})\n", refs[(i / 7) % refs.len()]));
        }
        doc.set_body(n, &body).unwrap();
    }
    doc.save().unwrap();
    (referenced, orphans)
}

#[test]
fn a_large_notebook_is_analysed_and_pruned_exactly() {
    let d = Dir::new();
    let p = d.join("big.omatree");
    let (referenced, orphans) = large(&p, 3000, 60, 5, 8);
    // Not every kept id is used by a note (only those the modulo reaches): count what is.
    let mut doc = Document::open_existing(&p).unwrap();
    let t0 = std::time::Instant::now();
    let pv = preview(&doc);
    let scan = t0.elapsed();
    let used: Vec<&String> = referenced
        .iter()
        .filter(|id| pv.report.eligible.iter().all(|e| &e.id.to_string() != *id))
        .collect();
    let mut expect_orphans: Vec<String> = orphans.clone();
    expect_orphans.extend(referenced.iter().filter(|id| !used.contains(id)).cloned());
    expect_orphans.sort();
    assert_eq!(eligible_ids(&pv.report), expect_orphans);
    assert!(pv.report.retained.count >= 1);
    let t1 = std::time::Instant::now();
    let res = doc.prune_attachments(&pv.token).unwrap();
    let prune = t1.elapsed();
    assert_eq!(res.removed.count, expect_orphans.len());
    assert_eq!(rows(&p).len(), 60 - expect_orphans.len());
    // Generous bounds (debug build): this guards against a quadratic scan, nothing finer.
    assert!(scan < Duration::from_secs(20), "{scan:?}");
    assert!(prune < Duration::from_secs(20), "{prune:?}");
}

/// `cargo test --release -p omatree-core -- --ignored --nocapture benchmark`
#[test]
#[ignore = "benchmark; prints timings"]
fn benchmark_scan_prune_and_reclaim_on_thousands_of_notes() {
    use std::time::Instant;
    let d = Dir::new();
    let p = d.join("bench.omatree");
    let (_, orphans) = large(&p, 6000, 400, 4, 150);
    let mut doc = Document::open_existing(&p).unwrap();
    for _ in 0..2 {
        doc.create_checkpoint("bench");
    }
    doc.save().unwrap();
    let size = std::fs::metadata(&p).unwrap().len();

    let t = Instant::now();
    let pv = preview(&doc);
    let dry = t.elapsed();
    let t = Instant::now();
    let pv2 = preview(&doc);
    let dry2 = t.elapsed();
    assert_eq!(pv.token, pv2.token);

    let t = Instant::now();
    let res = doc.prune_attachments(&pv2.token).unwrap();
    let prune = t.elapsed();
    let rp = doc.preview_reclaim().unwrap();
    let t = Instant::now();
    let rec = doc.reclaim_space(&rp.token).unwrap();
    let reclaim = t.elapsed();
    println!(
        "notes=6000 attachments=400 (about 90 KB each) file={size}B eligible={} (expected {})\n\
         preview {dry:?} (again {dry2:?})\nprune {prune:?} removed {} rows / {} B, file {} -> {}\n\
         reclaim {reclaim:?}: {} -> {} B (difference {})",
        pv.report.eligible.len(),
        orphans.len(),
        res.removed.count,
        res.removed.bytes,
        res.file_size_before.unwrap_or(0),
        res.file_size_after.unwrap_or(0),
        rec.size_before,
        rec.size_after,
        rec.size_difference()
    );
}
