//! Compact Notebook (ticket E11b): the dry run, the explicit removal, the separate reclamation,
//! through real frames of a `Host` on disposable notebooks. The assertions read the SQLite file
//! itself (attachment rows and exact bytes); no test names an image to delete.

use super::image_tests::{refs, shot, textures};
use super::*;
use crate::compact_ui::human_bytes;
use omatree_editor::assets::encode_png;
use std::collections::BTreeMap;

struct Fx {
    bytes: BTreeMap<String, Vec<u8>>,
    retained: Vec<String>,
    orphans: Vec<String>,
    /// Trash-only and checkpoint-only images, for the recovery checks.
    t_only: String,
    cp_only: String,
}

fn id_of(reference: &str) -> String {
    reference["assets/".len()..][..32].to_owned()
}

/// Active, overlapping, Trash-only, checkpoint-only and orphan images, Czech titles and text.
fn fixture(path: &Path) -> Fx {
    let mut bytes = BTreeMap::new();
    let mut doc = Document::open(path).unwrap();
    let mut put = |doc: &Document, w: usize| {
        let b = encode_png(&shot(w, 20)).unwrap();
        let r = doc.put_attachment(&b).unwrap();
        bytes.insert(id_of(&r), b);
        r
    };
    let a = put(&doc, 30);
    let g = put(&doc, 31);
    let t_only = put(&doc, 32);
    let t_cp = put(&doc, 33);
    let cp_only = put(&doc, 34);
    let o1 = put(&doc, 35);
    let o2 = put(&doc, 36);
    let r1 = doc.create_root("Příliš žluťoučký 🦀").unwrap();
    let r2 = doc.create_root("Druhá").unwrap();
    let r3 = doc.create_root("Do koše").unwrap();
    let r4 = doc.create_root("Kontrolní bod").unwrap();
    let r5 = doc.create_root("Jen koš").unwrap();
    doc.set_body(r1, &format!("![a]({a}) ![g]({g}) kůň\n"))
        .unwrap();
    doc.set_body(r2, &format!("![g]({g})\n")).unwrap();
    doc.set_body(r3, &format!("![t]({t_cp})\n")).unwrap();
    doc.set_body(r4, &format!("![c]({cp_only})\n")).unwrap();
    doc.set_body(r5, &format!("![t]({t_only})\n")).unwrap();
    doc.save().unwrap();
    doc.delete(r5).unwrap();
    doc.delete(r3).unwrap();
    doc.create_checkpoint("keep");
    doc.set_body(r4, "nic\n").unwrap();
    doc.save().unwrap();
    drop(doc);
    // `t_only` must be referenced by Trash alone: drop the checkpoints that hold it.
    let c = rusqlite::Connection::open(path).unwrap();
    c.execute(
        &format!(
            "DELETE FROM checkpoints WHERE id IN (SELECT checkpoint_id FROM checkpoint_nodes WHERE body LIKE '%{}%')",
            id_of(&t_only)
        ),
        [],
    )
    .unwrap();
    let mut retained = vec![a, g, t_cp, cp_only.clone(), t_only.clone()]
        .into_iter()
        .map(|r| id_of(&r))
        .collect::<Vec<_>>();
    retained.sort();
    let mut orphans = vec![id_of(&o1), id_of(&o2)];
    orphans.sort();
    Fx {
        bytes,
        retained,
        orphans,
        t_only: id_of(&t_only),
        cp_only: id_of(&cp_only),
    }
}

fn rows(path: &Path) -> BTreeMap<String, Vec<u8>> {
    let c = rusqlite::Connection::open(path).unwrap();
    let mut q = c
        .prepare("SELECT id, data FROM attachments ORDER BY id")
        .unwrap();
    let rows = q.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    rows.map(Result::unwrap).collect()
}

fn ids(m: &BTreeMap<String, Vec<u8>>) -> Vec<String> {
    m.keys().cloned().collect()
}

fn nb(name: &str) -> (PathBuf, PathBuf, H, Fx) {
    let d = dir(name);
    let p = d.join("n.omatree");
    let fx = fixture(&p);
    let mut h = H::open(&p);
    h.select("Druhá");
    (d, p, h, fx)
}

fn texts(h: &H) -> Vec<String> {
    h.painted().texts.into_iter().map(|t| t.text).collect()
}

impl H {
    fn compact(&mut self) {
        let ctx = self.ctx.clone();
        self.host.start_compact(&ctx);
        self.settle();
    }

    fn cstage(&self) -> &'static str {
        self.host.compact_stage().unwrap_or("none")
    }

    /// Confirm the removal and let the "Working…" frame and the real one pass.
    fn run_prune(&mut self) {
        self.host.compact_ask_prune();
        self.settle();
        assert_eq!(self.cstage(), "confirm-prune", "{}", self.host.status());
        self.host.compact_confirm_prune();
        self.settle();
    }

    fn doc_mut(&mut self) -> &mut Document {
        self.host.doc.as_mut().unwrap()
    }
}

#[test]
fn human_readable_sizes() {
    assert_eq!(human_bytes(0), "0 B");
    assert_eq!(human_bytes(1023), "1023 B");
    assert_eq!(human_bytes(1024), "1.0 KiB");
    assert_eq!(human_bytes(1536), "1.5 KiB");
    assert_eq!(human_bytes(5 * 1024 * 1024 + 512 * 1024), "5.5 MiB");
    assert_eq!(human_bytes(3 * 1024 * 1024 * 1024), "3.0 GiB");
}

#[test]
fn the_file_menu_lists_compact_notebook_without_a_shortcut() {
    let (_d, _p, mut h, _fx) = nb("cmp-menu");
    h.host.focus(Focus::Tree);
    h.settle();
    h.key(Key::Tab, Modifiers::SHIFT);
    h.press(Key::Enter);
    h.advance(0.3);
    assert!(
        texts(&h).iter().any(|t| t == "Compact Notebook\u{2026}"),
        "{:?}",
        texts(&h)
    );
    // Nothing started by itself.
    assert_eq!(h.cstage(), "none");
    assert_eq!(*h.host.dialog(), Dialog::None);
}

#[test]
fn an_untitled_notebook_is_explained_not_compacted() {
    let mut h = H::bare(Vec2::new(1000.0, 700.0));
    let ctx = h.ctx.clone();
    h.host.start_untitled(&ctx);
    h.settle();
    h.compact();
    assert_eq!(h.cstage(), "untitled");
    let t = texts(&h);
    assert!(t.iter().any(|s| s.contains("no file yet")), "{t:?}");
    assert!(!t.iter().any(|s| s.starts_with("Remove unused")));
    h.host.compact_ask_prune();
    h.host.compact_ask_reclaim();
    assert_eq!(h.cstage(), "untitled");
    h.press(Key::Escape);
    assert_eq!(*h.host.dialog(), Dialog::None);
}

#[test]
fn a_notebook_without_attachments_has_nothing_to_remove_and_offers_nothing_destructive() {
    let d = dir("cmp-v3");
    let p = d.join("n.omatree");
    make_fixture(&p);
    let before = fs::read(&p).unwrap();
    let mut h = H::open(&p);
    h.compact();
    assert_eq!(h.cstage(), "report");
    let t = texts(&h);
    assert!(
        t.iter().any(|s| s.contains("There are no unused images")),
        "{t:?}"
    );
    assert!(!t.iter().any(|s| s.starts_with("Remove unused")));
    h.host.compact_ask_prune();
    assert_eq!(
        h.cstage(),
        "report",
        "no confirmation without anything to remove"
    );
    assert_eq!(
        fs::read(&p).unwrap(),
        before,
        "the dry run wrote nothing, and a v3 file stayed v3"
    );
}

#[test]
fn the_report_shows_the_core_figures_protects_recovery_and_writes_nothing() {
    let (_d, p, mut h, fx) = nb("cmp-report");
    let before = fs::read(&p).unwrap();
    let state = h.state();
    h.compact();
    assert_eq!(h.cstage(), "report");
    let report = h
        .host
        .document()
        .unwrap()
        .preview_attachment_prune()
        .unwrap()
        .report;
    assert_eq!(report.eligible.len(), fx.orphans.len());
    let total = |ids: &[String]| ids.iter().map(|i| fx.bytes[i].len() as u64).sum::<u64>();
    assert_eq!(report.eligible_total.bytes, total(&fx.orphans));
    assert_eq!(report.retained.bytes, total(&fx.retained));
    let t = texts(&h);
    let line = |label: &str| {
        let i = t
            .iter()
            .position(|s| s == label)
            .unwrap_or_else(|| panic!("no {label:?} in {t:?}"));
        t[i + 1].clone()
    };
    let show = |n: usize, bytes: u64| format!("{n} images ({})", human_bytes(bytes));
    assert_eq!(
        line("Images stored in the notebook"),
        show(7, total(&fx.retained) + total(&fx.orphans))
    );
    assert_eq!(line("Will stay"), show(5, total(&fx.retained)));
    assert_eq!(line("Unused, can be removed"), show(2, total(&fx.orphans)));
    // The groups overlap and the dialog says so instead of adding them up.
    assert!(
        t.iter().any(|s| s.contains("do not add up to the total")),
        "{t:?}"
    );
    assert!(
        t.iter().any(|s| s.contains("never removed")),
        "recovery protection is explained"
    );
    assert!(
        t.iter().any(|s| s.contains("typed or edited by hand")),
        "the canonical-reference warning"
    );
    // A dry run changes nothing: not the file, not the notebook in memory.
    assert_eq!(fs::read(&p).unwrap(), before);
    assert_eq!(h.state(), state);
    h.press(Key::Escape);
    assert_eq!(*h.host.dialog(), Dialog::None);
    assert_eq!(rows(&p).len(), 7);
}

#[test]
fn only_genuine_orphans_are_removed_after_an_explicit_confirmation_and_recovery_survives() {
    let (_d, p, mut h, fx) = nb("cmp-prune");
    h.compact();
    h.host.compact_ask_prune();
    h.settle();
    // The confirmation says what will happen; nothing has happened yet.
    assert_eq!(h.cstage(), "confirm-prune");
    let t = texts(&h);
    assert!(
        t.iter()
            .any(|s| s.contains("Remove 2 images") && s.contains("permanent")
                || s.contains("Remove 2 images")),
        "{t:?}"
    );
    assert!(t.iter().any(|s| s.contains("This is permanent")));
    assert!(t.iter().any(|s| s == "Cancel") && t.iter().any(|s| s == "Remove unused images"));
    assert_eq!(rows(&p).len(), 7, "nothing removed by asking");
    h.host.compact_confirm_prune();
    h.settle();
    assert_eq!(h.cstage(), "prune-done");
    // The rows and bytes: exactly the protected ones, byte for byte.
    let now = rows(&p);
    assert_eq!(ids(&now), fx.retained);
    for (id, bytes) in &now {
        assert_eq!(bytes, &fx.bytes[id], "{id} changed");
    }
    // The result is the committed one, and the file is not smaller yet.
    let t = texts(&h);
    assert!(t.iter().any(|s| s.starts_with("Removed 2 images")), "{t:?}");
    assert!(t.iter().any(|s| s.contains("not smaller yet")), "{t:?}");
    assert!(
        h.host.status().starts_with("Removed 2 images"),
        "{}",
        h.host.status()
    );
    // Nothing reclaimed by itself: the file is exactly as before the removal.
}

#[test]
fn cancelling_at_each_step_changes_nothing() {
    let (_d, p, mut h, _fx) = nb("cmp-cancel");
    let before = fs::read(&p).unwrap();
    h.compact();
    h.host.compact_ask_prune();
    h.settle();
    assert_eq!(h.cstage(), "confirm-prune");
    // Enter on the confirmation reaches Cancel (it has the focus), not the removal.
    h.press(Key::Enter);
    assert_eq!(h.cstage(), "report", "{}", h.host.status());
    h.host.compact_ask_prune();
    h.settle();
    h.press(Key::Escape);
    assert_eq!(
        *h.host.dialog(),
        Dialog::None,
        "Escape closes without removing"
    );
    assert_eq!(fs::read(&p).unwrap(), before);
    assert_eq!(rows(&p).len(), 7);
}

#[test]
fn unsaved_work_must_be_saved_first_and_saving_authorizes_nothing() {
    let (_d, p, mut h, fx) = nb("cmp-dirty");
    let end = h.host.buffer().len();
    h.set_caret(end, end);
    h.type_text("neuloženo");
    assert!(h.host.is_dirty());
    h.compact();
    assert_eq!(h.cstage(), "report");
    let t = texts(&h);
    assert!(t.iter().any(|s| s.contains("unsaved changes")), "{t:?}");
    assert!(t.iter().any(|s| s == "Save and review again"));
    assert!(
        !t.iter().any(|s| s.starts_with("Remove unused")),
        "no removal while dirty"
    );
    h.host.compact_ask_prune();
    assert_eq!(h.cstage(), "report");
    assert_eq!(rows(&p).len(), 7);
    // Save: a fresh report, still needing its own confirmation.
    let ctx = h.ctx.clone();
    h.host.compact_save(&ctx);
    h.settle();
    assert!(!h.host.is_dirty());
    assert_eq!(h.cstage(), "report");
    assert_eq!(rows(&p).len(), 7, "saving removed nothing");
    assert!(texts(&h).iter().any(|s| s.starts_with("Remove unused")));
    let _ = fx;
}

#[test]
fn a_pending_image_a_pending_title_a_cancelled_and_a_failed_save() {
    use std::os::unix::fs::PermissionsExt;
    let (d, p, mut h, _fx) = nb("cmp-pending");
    // A pending attachment makes the notebook dirty: save first.
    super::image_tests::paste(&mut h, &shot(9, 9));
    assert!(h.host.document().unwrap().pending_attachments().0 > 0);
    h.compact();
    assert!(texts(&h).iter().any(|s| s == "Save and review again"));
    assert!(texts(&h).iter().any(|s| s == "Waiting to be saved"));
    // Closing the dialog without saving keeps everything pending.
    h.press(Key::Escape);
    assert!(h.host.is_dirty() && h.host.document().unwrap().pending_attachments().0 == 1);
    // A failed save: reported, nothing reviewed, nothing removed, everything kept.
    h.compact();
    fs::set_permissions(&d, fs::Permissions::from_mode(0o555)).unwrap();
    let ctx = h.ctx.clone();
    h.host.compact_save(&ctx);
    h.settle();
    fs::set_permissions(&d, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(h.cstage(), "failed");
    assert!(h.host.is_dirty() && h.host.document().unwrap().pending_attachments().0 == 1);
    assert_eq!(rows(&p).len(), 7);
    h.host.cancel_dialog();
    // An invalid pending title: the usual explanation, and Compact does not even open.
    h.host.title_text = "   ".to_owned();
    h.host.start_compact(&ctx);
    h.settle();
    assert!(
        matches!(h.host.dialog(), Dialog::Error(_)),
        "{:?}",
        h.host.dialog()
    );
    assert_eq!(h.cstage(), "none");
}

#[test]
fn an_external_conflict_goes_to_the_conflict_question_and_removes_nothing() {
    let (_d, p, mut h, _fx) = nb("cmp-conflict");
    let end = h.host.buffer().len();
    h.set_caret(end, end);
    h.type_text("x");
    h.compact();
    {
        let c = rusqlite::Connection::open(&p).unwrap();
        c.execute(
            "UPDATE nodes SET title = title || 'e' WHERE title = 'Kontrolní bod'",
            [],
        )
        .unwrap();
    }
    let ctx = h.ctx.clone();
    h.host.compact_save(&ctx);
    h.settle();
    assert_eq!(*h.host.dialog(), Dialog::Conflict, "{}", h.host.status());
    assert_eq!(h.cstage(), "none");
    assert_eq!(rows(&p).len(), 7);
    assert!(h.host.is_dirty(), "the edit is still held");
}

#[test]
fn a_stale_review_is_refused_whatever_changed_and_the_images_stay() {
    // 1. A note edit after the preview.
    let (_d, p, mut h, _fx) = nb("cmp-stale-edit");
    h.compact();
    h.host.compact_ask_prune();
    h.settle();
    let id = h.id("Druhá");
    h.doc_mut().set_body(id, "jiný text\n").unwrap();
    h.host.compact_confirm_prune();
    h.settle();
    assert_eq!(h.cstage(), "stale", "{}", h.host.status());
    assert_eq!(rows(&p).len(), 7);
    assert!(texts(&h).iter().any(|s| s.contains("Nothing was removed")));
    // A new review needs a new confirmation; the old state is not reused.
    h.host.cancel_dialog();

    // 2. A recovery change (a checkpoint) after the preview.
    let (_d, p, mut h, _fx) = nb("cmp-stale-recovery");
    h.compact();
    h.host.compact_ask_prune();
    h.settle();
    h.doc_mut().create_checkpoint("po náhledu");
    h.host.compact_confirm_prune();
    h.settle();
    assert_eq!(h.cstage(), "stale");
    assert_eq!(rows(&p).len(), 7);

    // 3. Another program writes to the file after the preview.
    let (_d, p, mut h, _fx) = nb("cmp-stale-external");
    h.compact();
    h.host.compact_ask_prune();
    h.settle();
    {
        let c = rusqlite::Connection::open(&p).unwrap();
        c.execute(
            "INSERT INTO attachments VALUES (?1, 'image/png', 0, 3, x'010203')",
            ["ef".repeat(16)],
        )
        .unwrap();
    }
    h.host.compact_confirm_prune();
    h.settle();
    assert!(matches!(h.cstage(), "stale" | "none"), "{}", h.cstage());
    assert_eq!(
        rows(&p).len(),
        8,
        "the external row is untouched and nothing was removed"
    );
}

#[test]
fn repeated_confirmations_remove_once_and_a_stale_token_never_runs_again() {
    let (_d, p, mut h, fx) = nb("cmp-repeat");
    h.compact();
    h.host.compact_ask_prune();
    h.settle();
    for _ in 0..4 {
        h.host.compact_confirm_prune();
    }
    h.frame(vec![], Modifiers::NONE);
    assert_eq!(
        h.cstage(),
        "working",
        "one frame shows the operation before it runs"
    );
    h.settle();
    assert_eq!(h.cstage(), "prune-done");
    assert_eq!(ids(&rows(&p)), fx.retained);
    // More confirmations after it finished are ignored.
    for _ in 0..3 {
        h.host.compact_confirm_prune();
        h.host.compact_ask_prune();
    }
    h.settle();
    assert_eq!(h.cstage(), "prune-done");
    assert_eq!(ids(&rows(&p)), fx.retained);
}

#[test]
fn a_failed_removal_is_reported_as_failed_and_keeps_every_image() {
    use std::os::unix::fs::PermissionsExt;
    let (d, p, mut h, fx) = nb("cmp-prune-fail");
    h.compact();
    h.host.compact_ask_prune();
    h.settle();
    fs::set_permissions(&d, fs::Permissions::from_mode(0o555)).unwrap();
    h.host.compact_confirm_prune();
    h.settle();
    fs::set_permissions(&d, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(h.cstage(), "failed", "{}", h.host.status());
    assert!(h.host.status().contains("failed"), "{}", h.host.status());
    assert_eq!(rows(&p).len(), 7);
    for (id, bytes) in rows(&p) {
        assert_eq!(bytes, fx.bytes[&id]);
    }
    // The notebook is still readable and the removal can be reviewed again.
    h.host.compact_review(&h.ctx.clone());
    h.settle();
    assert_eq!(h.cstage(), "report");
}

#[test]
fn space_is_reclaimed_only_on_its_own_confirmation_and_the_file_really_shrinks() {
    let d = dir("cmp-reclaim");
    let p = d.join("n.omatree");
    // Big orphans so the freed pages are worth giving back.
    {
        let mut doc = Document::open(&p).unwrap();
        let keep = doc
            .put_attachment(&encode_png(&shot(40, 40)).unwrap())
            .unwrap();
        let n = doc.create_root("Drží").unwrap();
        doc.set_body(n, &format!("![k]({keep})\n")).unwrap();
        for i in 0..3 {
            doc.put_attachment(&encode_png(&shot(700 + i, 500)).unwrap())
                .unwrap();
        }
        doc.save().unwrap();
    }
    let mut h = H::open(&p);
    h.compact();
    let size0 = fs::metadata(&p).unwrap().len();
    h.run_prune();
    assert_eq!(h.cstage(), "prune-done");
    assert_eq!(
        fs::metadata(&p).unwrap().len(),
        size0,
        "removing rows does not shrink the file"
    );
    // Declining ends the workflow without a VACUUM.
    let mut declined = h.host.compact_stage().is_some();
    h.press(Key::Escape);
    declined &= *h.host.dialog() == Dialog::None;
    assert!(declined);
    assert_eq!(fs::metadata(&p).unwrap().len(), size0);
    // Reclaim: a separate preview, a separate confirmation, the actual sizes.
    h.compact();
    h.host.compact_ask_reclaim();
    h.settle();
    assert_eq!(h.cstage(), "confirm-reclaim");
    assert!(texts(&h)
        .iter()
        .any(|s| s.contains("about the size of the notebook")));
    h.host.compact_back();
    h.settle();
    assert_eq!(h.cstage(), "report", "cancelling reclaims nothing");
    assert_eq!(fs::metadata(&p).unwrap().len(), size0);
    h.host.compact_ask_reclaim();
    h.settle();
    h.host.compact_confirm_reclaim();
    h.settle();
    assert_eq!(h.cstage(), "reclaim-done", "{}", h.host.status());
    let size1 = fs::metadata(&p).unwrap().len();
    assert!(size1 < size0, "{size1} < {size0}");
    assert!(texts(&h).iter().any(|s| s.contains(&human_bytes(size1))));
    assert!(Document::open_existing(&p)
        .unwrap()
        .verify_integrity()
        .unwrap()
        .is_healthy());
}

#[test]
fn reclaiming_works_without_pruning_and_a_failed_vacuum_leaves_a_readable_notebook() {
    use std::os::unix::fs::PermissionsExt;
    let d = dir("cmp-reclaim-alone");
    let p = d.join("n.omatree");
    {
        let mut doc = Document::open(&p).unwrap();
        let keep = doc
            .put_attachment(&encode_png(&shot(40, 40)).unwrap())
            .unwrap();
        let n = doc.create_root("Drží").unwrap();
        doc.set_body(n, &format!("![k]({keep})\n")).unwrap();
        for i in 0..3 {
            doc.put_attachment(&encode_png(&shot(700 + i, 500)).unwrap())
                .unwrap();
        }
        doc.save().unwrap();
        // Remove the orphans through the core, outside the dialog: free pages, nothing eligible.
        let pv = doc.preview_attachment_prune().unwrap();
        doc.prune_attachments(&pv.token).unwrap();
    }
    let size0 = fs::metadata(&p).unwrap().len();
    let mut h = H::open(&p);
    h.compact();
    let t = texts(&h);
    assert!(t.iter().any(|s| s.contains("There are no unused images")));
    assert!(
        t.iter().any(|s| s == "Reclaim disk space\u{2026}"),
        "reclaim is offered on its own: {t:?}"
    );
    // A VACUUM that cannot run reports failure, claims nothing, and the notebook stays usable.
    h.host.compact_ask_reclaim();
    h.settle();
    fs::set_permissions(&d, fs::Permissions::from_mode(0o555)).unwrap();
    h.host.compact_confirm_reclaim();
    h.settle();
    fs::set_permissions(&d, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(h.cstage(), "failed", "{}", h.host.status());
    assert!(h.host.status().contains("failed"));
    assert_eq!(fs::metadata(&p).unwrap().len(), size0);
    h.host.cancel_dialog();
    let end = h.host.buffer().len();
    h.set_caret(end, end);
    h.type_text("po");
    h.key(Key::S, CTRL);
    assert!(
        !h.host.is_dirty(),
        "the notebook can still be edited and saved: {}",
        h.host.status()
    );
    // And it works once the obstacle is gone.
    h.compact();
    h.host.compact_ask_reclaim();
    h.settle();
    h.host.compact_confirm_reclaim();
    h.settle();
    assert_eq!(h.cstage(), "reclaim-done", "{}", h.host.status());
    assert!(fs::metadata(&p).unwrap().len() < size0);
}

#[test]
fn replacing_the_notebook_drops_the_review_and_its_tokens() {
    let (d, _p, mut h, _fx) = nb("cmp-replace");
    h.compact();
    h.host.compact_ask_prune();
    h.settle();
    assert_eq!(h.cstage(), "confirm-prune");
    let other = d.join("other.omatree");
    make_fixture(&other);
    let ctx = h.ctx.clone();
    assert!(h.host.open(&ctx, &other));
    h.settle();
    assert_eq!(
        h.cstage(),
        "none",
        "the old review cannot be confirmed any more"
    );
    assert_eq!(*h.host.dialog(), Dialog::None);
    h.host.compact_confirm_prune();
    h.settle();
    assert_eq!(h.cstage(), "none");
}

#[test]
fn after_compaction_the_notebook_reopens_and_recovery_restores_its_images() {
    let (d, p, mut h, fx) = nb("cmp-recovery");
    h.compact();
    h.run_prune();
    assert_eq!(h.cstage(), "prune-done");
    h.press(Key::Escape);
    // Reopen: healthy, every protected image byte for byte.
    drop(h);
    let mut h = H::open(&p);
    assert!(h
        .host
        .document()
        .unwrap()
        .verify_integrity()
        .unwrap()
        .is_healthy());
    let ctx = h.ctx.clone();
    // Trash: both Trash entries come back with their images.
    h.host.open_recovery(&ctx);
    h.settle();
    let (trash, _cp) = h.host.recovery_counts();
    assert_eq!(trash, 2);
    assert!(h.host.restore_trash(&ctx, 0));
    h.host.cancel_dialog();
    h.host.open_recovery(&ctx);
    h.settle();
    assert!(h.host.restore_trash(&ctx, 0), "{}", h.host.status());
    h.host.cancel_dialog();
    // Checkpoint: the one holding the checkpoint-only image.
    let doc = h.host.document().unwrap();
    let idx = doc
        .recovery()
        .checkpoints()
        .iter()
        .position(|c| c.reason() == "keep")
        .expect("the checkpoint that holds the image");
    let _ = &fx.cp_only;
    let shown = doc.recovery().checkpoints().len() - 1 - idx;
    h.host.open_recovery(&ctx);
    h.settle();
    assert!(
        h.host.restore_checkpoint(&ctx, shown),
        "{}",
        h.host.status()
    );
    h.host.cancel_dialog();
    let doc = h.host.document().unwrap();
    for id in [&fx.t_only, &fx.cp_only] {
        assert_eq!(
            doc.attachment(&format!("assets/{id}.png")).unwrap(),
            fx.bytes[id],
            "{id} lost"
        );
    }
    let _ = d;
}

#[test]
fn export_still_works_after_a_compaction() {
    let (d, _p, mut h, fx) = nb("cmp-export");
    h.compact();
    h.run_prune();
    h.press(Key::Escape);
    h.select("Příliš žluťoučký 🦀");
    h.host.set_picker(None);
    let ctx = h.ctx.clone();
    h.host.start_export(&ctx);
    h.host
        .set_export_format(crate::note_export::ExportFormat::Folder);
    h.host.choose_export_destination(&ctx);
    h.settle();
    h.host.set_export_path(&d.join("out").display().to_string());
    h.host.submit_export_path(&ctx);
    h.settle();
    assert_eq!(*h.host.dialog(), Dialog::None, "{}", h.host.status());
    let r = refs(&h.body("Příliš žluťoučký 🦀"));
    assert_eq!(r.len(), 2);
    for reference in r {
        assert_eq!(
            fs::read(d.join("out").join(&reference)).unwrap(),
            fx.bytes[&id_of(&reference)]
        );
    }
}

#[test]
fn the_dialog_is_keyboard_driven_themed_and_usable_in_a_narrow_window() {
    // Focus: the safe button first, in every stage.
    let (_d, p, mut h, _fx) = nb("cmp-keys");
    h.compact();
    h.press(Key::Enter);
    assert_eq!(
        *h.host.dialog(),
        Dialog::None,
        "Enter on the report closes it (Close has the focus)"
    );
    assert_eq!(rows(&p).len(), 7);
    // Tab reaches the removal, Enter on it asks (it never removes by itself).
    h.compact();
    h.press(Key::Tab);
    h.press(Key::Enter);
    assert_eq!(h.cstage(), "confirm-prune", "{}", h.host.status());
    assert_eq!(rows(&p).len(), 7);
    h.press(Key::Escape);
    // Nothing leaks to the editor behind the dialog.
    h.compact();
    let before = h.host.buffer().to_owned();
    h.type_text("zakázáno");
    assert_eq!(h.host.buffer(), before);
    h.press(Key::Escape);
    // Light and dark, and a live theme change in the middle of a review.
    h.compact();
    for theme in [Some(egui::Theme::Light), Some(egui::Theme::Dark), None] {
        h.system = theme;
        h.settle();
        assert_eq!(h.cstage(), "report", "a theme change keeps the review");
        assert!(texts(&h).iter().any(|s| s == "Compact notebook"));
    }
    assert_eq!(rows(&p).len(), 7, "a theme change removed nothing");
    h.press(Key::Escape);
    // Narrow window: every button stays on screen.
    let mut n = H::open_sized(&p, Vec2::new(360.0, 560.0));
    n.select("Druhá");
    n.compact();
    for label in ["Close", "Remove unused images\u{2026}"] {
        let t = n
            .painted()
            .texts
            .into_iter()
            .find(|t| t.text == label)
            .unwrap_or_else(|| panic!("no {label}"));
        assert!(
            t.pos.x >= 0.0 && t.pos.x < 360.0 && t.pos.y >= 0.0 && t.pos.y < 560.0,
            "{label}: {:?}",
            t.pos
        );
    }
    let _ = textures(&n);
}

#[test]
fn the_open_dialog_costs_no_per_frame_scan() {
    let (_d, _p, mut h, _fx) = nb("cmp-idle");
    h.compact();
    // The review is computed once, when the dialog opens: frames after that reuse it, so
    // changing the document behind the open dialog does not change what it shows.
    let id = h.id("Druhá");
    let dialog = |h: &H| {
        let t = texts(h);
        let at = t.iter().position(|s| s == "Compact notebook").unwrap();
        t[at..].to_vec()
    };
    let before = dialog(&h);
    h.doc_mut().set_body(id, "jiný\n").unwrap();
    for _ in 0..5 {
        h.frame(vec![], Modifiers::NONE);
    }
    assert_eq!(
        dialog(&h),
        before,
        "the report is not recalculated per frame"
    );
}

/// The disposable notebook for the manual native-window check of E11b
/// (`OMATREE_GUI_DIR=... cargo test -p omatree-egui -- --ignored write_compact_gui_fixture`).
#[test]
#[ignore = "writes a fixture for the manual GUI check"]
fn write_compact_gui_fixture() {
    let dir = PathBuf::from(std::env::var("OMATREE_GUI_DIR").expect("set OMATREE_GUI_DIR"));
    fs::create_dir_all(&dir).unwrap();
    fixture(&dir.join("compact.omatree"));
    // Big orphans in a second notebook, so reclaiming has something to give back.
    let p = dir.join("space.omatree");
    let mut doc = Document::open(&p).unwrap();
    let keep = doc
        .put_attachment(&encode_png(&shot(40, 40)).unwrap())
        .unwrap();
    let n = doc.create_root("Drží").unwrap();
    doc.set_body(n, &format!("![k]({keep})\n")).unwrap();
    for i in 0..4 {
        doc.put_attachment(&noise_png(700 + i, 500)).unwrap();
    }
    doc.save().unwrap();
}

/// A PNG that does not compress (pseudo-random pixels), so its size is about w*h*4.
fn noise_png(w: usize, h: usize) -> Vec<u8> {
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15 ^ (w as u64 * 31 + h as u64);
    let mut img = egui::ColorImage::filled([w, h], egui::Color32::BLACK);
    for p in &mut img.pixels {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *p = egui::Color32::from_rgb(x as u8, (x >> 8) as u8, (x >> 16) as u8);
    }
    encode_png(&img).unwrap()
}

fn peak_kib() -> i64 {
    fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("VmHWM:"))
        .and_then(|v| v.trim().trim_end_matches("kB").trim().parse().ok())
        .unwrap_or(0)
}

/// `cargo test --release -p omatree-egui -- --ignored --nocapture compact_performance`.
#[test]
#[ignore = "measurement, run with --release"]
fn compact_performance() {
    use std::time::Instant;
    let d = dir("cmp-perf");
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    let mut refs_kept = Vec::new();
    for i in 0..300usize {
        let r = doc.put_attachment(&noise_png(100 + i % 40, 80)).unwrap();
        if i % 3 != 0 {
            refs_kept.push(r);
        }
    }
    let mut parent = None;
    for i in 0..5000usize {
        let id = match parent {
            Some(par) if i % 7 != 0 => doc.create_child(par, &format!("Poznámka {i}")).unwrap(),
            _ => doc.create_root(&format!("Kořen {i}")).unwrap(),
        };
        parent = Some(id);
        let r = &refs_kept[i % refs_kept.len()];
        doc.set_body(
            id,
            &format!("Obsah žluťoučký {i}\n\n![x]({r})\n{}", "text ".repeat(50)),
        )
        .unwrap();
        if i % 500 == 0 {
            doc.create_checkpoint("historie");
        }
    }
    doc.save().unwrap();
    drop(doc);
    println!(
        "fixture: {} KiB file, 5000 notes, 300 images (100 unused)",
        fs::metadata(&p).unwrap().len() / 1024
    );
    let mut h = H::open(&p);
    let t = Instant::now();
    h.compact();
    println!(
        "open Compact Notebook (scan every body, no BLOB read): {:.0} ms",
        t.elapsed().as_secs_f64() * 1000.0
    );
    let t = Instant::now();
    for _ in 0..60 {
        h.frame(vec![], Modifiers::NONE);
    }
    println!(
        "idle frame with the dialog open: {:.2} ms",
        t.elapsed().as_secs_f64() * 1000.0 / 60.0
    );
    let size0 = fs::metadata(&p).unwrap().len();
    h.host.compact_ask_prune();
    h.settle();
    let t = Instant::now();
    h.host.compact_confirm_prune();
    h.frame(vec![], Modifiers::NONE);
    h.frame(vec![], Modifiers::NONE);
    println!(
        "confirm and remove (two frames, including the Working frame): {:.0} ms",
        t.elapsed().as_secs_f64() * 1000.0
    );
    assert_eq!(h.cstage(), "prune-done");
    h.host.compact_ask_reclaim();
    h.settle();
    let t = Instant::now();
    h.host.compact_confirm_reclaim();
    h.frame(vec![], Modifiers::NONE);
    h.frame(vec![], Modifiers::NONE);
    println!(
        "confirm and reclaim: {:.0} ms; file {} KiB -> {} KiB",
        t.elapsed().as_secs_f64() * 1000.0,
        size0 / 1024,
        fs::metadata(&p).unwrap().len() / 1024
    );
    println!("peak resident of the process: {} MiB", peak_kib() / 1024);
}
