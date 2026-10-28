//! Embedded images (ticket E10): the clipboard-image workflow, display, persistence and recovery
//! through real frames of a `Host` on disposable notebooks. Pastes are `Event::PasteImage`
//! events, exactly what egui hands over for a clipboard image; no test inserts reference text
//! where the behaviour under test is a paste.

use super::*;
use omatree_editor::assets::encode_png;
use std::sync::Arc;

/// A deterministic picture (a gradient, so no two sizes encode alike).
pub(super) fn shot(w: usize, h: usize) -> egui::ColorImage {
    let mut img = egui::ColorImage::filled([w, h], egui::Color32::WHITE);
    for y in 0..h {
        for x in 0..w {
            img.pixels[y * w + x] =
                egui::Color32::from_rgb((x * 255 / w.max(1)) as u8, (y * 255 / h.max(1)) as u8, 90);
        }
    }
    img
}

fn id_only(reference: &str) -> String {
    reference["assets/".len()..][..32].to_owned()
}

pub(super) fn paste(h: &mut H, img: &egui::ColorImage) {
    h.frame(
        vec![Event::PasteImage(Arc::new(img.clone()))],
        Modifiers::NONE,
    );
    h.frame(vec![], Modifiers::NONE);
}

/// The image references in `text`, in order.
pub(super) fn refs(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("assets/") {
        let r = &rest[i..];
        if r.len() >= 7 + 32 + 4 {
            out.push(r[..7 + 32 + 4].to_owned());
        }
        rest = &rest[i + 7..];
    }
    out
}

/// Decoded images the editor holds as textures: (name, size).
pub(super) fn textures(h: &H) -> Vec<(String, [usize; 2])> {
    let mut v: Vec<_> = h
        .ctx
        .tex_manager()
        .read()
        .allocated()
        .filter(|(_, m)| m.name.starts_with("img:"))
        .map(|(_, m)| (m.name.clone(), m.size))
        .collect();
    v.sort();
    v
}

pub(super) fn stored_bytes(path: &Path, reference: &str) -> Vec<u8> {
    Document::open_existing(path)
        .unwrap()
        .attachment(reference)
        .unwrap()
}

fn new_note(path: &Path) -> (NodeId, NodeId) {
    let mut doc = Document::open(path).unwrap();
    let a = doc.create_root("Obrázky").unwrap();
    let b = doc.create_root("Druhá").unwrap();
    doc.set_body(a, "text před\n").unwrap();
    doc.set_body(b, "druhá\n").unwrap();
    doc.save().unwrap();
    (a, b)
}

/// Opens `path` with the first note selected and the caret at its end.
fn open_note(path: &Path) -> H {
    let mut h = H::open(path);
    h.select("Obrázky");
    let n = h.host.buffer().len();
    h.set_caret(n, n);
    h
}

#[test]
fn a_pasted_png_is_stored_referenced_rendered_saved_and_reopened_byte_for_byte() {
    let d = dir("img-paste");
    let p = d.join("n.omatree");
    new_note(&p);
    let mut h = open_note(&p);
    let img = shot(24, 16);
    paste(&mut h, &img);

    // The editor wrote the reference through its own edit; the host did not.
    let body = h.body("Obrázky");
    let r = refs(&body);
    assert_eq!(r.len(), 1, "{body}");
    assert!(r[0].ends_with(".png") && body.contains("!["), "{body}");
    assert!(body.starts_with("text před\n"), "{body}");
    // Staged, not yet in the file: the notebook is dirty, nothing was written yet.
    assert!(h.host.is_dirty());
    assert_eq!(h.host.document().unwrap().pending_attachments().0, 1);
    assert!(Document::open_existing(&p)
        .unwrap()
        .stored_attachment_ids()
        .unwrap()
        .is_empty());
    assert_eq!(
        fs::read_dir(&d).unwrap().count(),
        1,
        "no file beside the notebook"
    );
    // Shown inline.
    assert_eq!(textures(&h), vec![(format!("img:{}", r[0]), [24, 16])]);

    // Autosave restarts after the paste and commits it.
    h.advance(3.0);
    h.advance(3.0);
    assert!(!h.host.is_dirty(), "autosaved: {}", h.host.status());
    assert_eq!(h.host.document().unwrap().pending_attachments().0, 0);
    assert_eq!(stored_bytes(&p, &r[0]), encode_png(&img).unwrap());

    // Close and reopen: same reference, same bytes, rendered again.
    drop(h);
    let mut h = open_note(&p);
    h.settle();
    assert_eq!(refs(h.host.buffer()), r);
    assert_eq!(textures(&h), vec![(format!("img:{}", r[0]), [24, 16])]);
    assert_eq!(stored_bytes(&p, &r[0]), encode_png(&img).unwrap());
}

#[test]
fn one_paste_is_one_undo_step_and_the_orphan_stays_stored_but_is_never_pruned_by_itself() {
    let d = dir("img-undo");
    let p = d.join("n.omatree");
    new_note(&p);
    let mut h = open_note(&p);
    let before = h.host.buffer().to_owned();
    let img = shot(10, 10);
    paste(&mut h, &img);
    let pasted = h.host.buffer().to_owned();
    assert_ne!(pasted, before);
    // Ctrl+Z once: back to the text before the paste.
    h.key(Key::Z, CTRL);
    assert_eq!(h.host.buffer(), before, "one undo reverses the whole paste");
    // Redo brings the same reference back.
    h.key(Key::Z, CTRL | Modifiers::SHIFT);
    assert_eq!(h.host.buffer(), pasted);
    // Undo again, save: the image stays as an unreferenced attachment.
    h.key(Key::Z, CTRL);
    h.key(Key::S, CTRL);
    assert!(!h.host.is_dirty());
    let doc = Document::open_existing(&p).unwrap();
    assert_eq!(doc.stored_attachment_ids().unwrap().len(), 1, "orphan kept");
    let preview = doc.preview_attachment_prune().unwrap();
    assert_eq!(
        preview.report.eligible.len(),
        1,
        "listed, never deleted by itself"
    );
    assert_eq!(doc.stored_attachment_ids().unwrap().len(), 1);
}

#[test]
fn pasting_into_an_untitled_notebook_creates_no_file_until_save_as() {
    let d = dir("img-untitled");
    let mut h = H::bare(Vec2::new(1000.0, 700.0));
    let ctx = h.ctx.clone();
    h.host.start_untitled(&ctx);
    h.host.create_root(&ctx);
    h.host.focus(Focus::Editor);
    h.settle();
    let img = shot(12, 9);
    paste(&mut h, &img);
    let r = refs(h.host.buffer());
    assert_eq!(r.len(), 1, "{}", h.host.buffer());
    h.advance(10.0);
    assert!(
        h.host.is_dirty(),
        "autosave never writes an untitled notebook"
    );
    assert_eq!(fs::read_dir(&d).unwrap().count(), 0);
    assert_eq!(h.host.document().unwrap().pending_attachments().0, 1);
    assert_eq!(textures(&h).len(), 1);
    let target = d.join("kept.omatree");
    h.host.start_save_as_typed(&ctx);
    h.host.set_save_as_path(&target.display().to_string());
    h.host.submit_save_as(&ctx);
    h.settle();
    assert!(!h.host.is_dirty(), "{}", h.host.status());
    assert_eq!(stored_bytes(&target, &r[0]), encode_png(&img).unwrap());
}

#[test]
fn a_refused_paste_changes_nothing_and_says_why() {
    let d = dir("img-budget");
    let p = d.join("n.omatree");
    new_note(&p);
    let mut h = open_note(&p);
    // Fill the unsaved-image budget through the document itself (two images of the largest size).
    let filler = padded_png(omatree_core::attachments::MAX_ATTACHMENT_BYTES as usize, 0);
    let filler2 = padded_png(omatree_core::attachments::MAX_ATTACHMENT_BYTES as usize, 1);
    let doc = h.host.document().unwrap();
    doc.put_attachment(&filler).unwrap();
    doc.put_attachment(&filler2).unwrap();
    let before = h.host.buffer().to_owned();
    paste(&mut h, &shot(8, 8));
    assert_eq!(
        h.host.buffer(),
        before,
        "no reference to an image that was not stored"
    );
    let status = h.host.status().to_owned();
    assert!(
        status.contains("Image not added") && status.contains("save the notebook first"),
        "{status}"
    );
    assert_eq!(
        h.host.document().unwrap().pending_attachments().0,
        2,
        "nothing discarded"
    );
    // A save releases the budget; the same paste then works.
    h.key(Key::S, CTRL);
    assert!(!h.host.is_dirty(), "{}", h.host.status());
    paste(&mut h, &shot(8, 8));
    assert_eq!(refs(h.host.buffer()).len(), 1);
}

/// A structurally valid PNG of exactly `total` bytes (a 1x1 image padded with a `tEXt` chunk);
/// `variant` makes two of the same size differ.
fn padded_png(total: usize, variant: u8) -> Vec<u8> {
    fn crc(bytes: &[u8]) -> u32 {
        let mut table = [0u32; 256];
        for (n, slot) in table.iter_mut().enumerate() {
            let mut c = n as u32;
            for _ in 0..8 {
                c = if c & 1 == 1 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
            *slot = c;
        }
        let mut c = 0xFFFF_FFFFu32;
        for b in bytes {
            c = table[((c ^ u32::from(*b)) & 0xFF) as usize] ^ (c >> 8);
        }
        !c
    }
    let base = encode_png(&egui::ColorImage::filled([1, 1], egui::Color32::RED)).unwrap();
    let iend = base.len() - 12;
    let pad = total - base.len() - 12;
    let mut out = base[..iend].to_vec();
    let mut body = b"tEXt".to_vec();
    body.extend(std::iter::repeat_n(b'x', pad));
    body[10] = variant;
    out.extend_from_slice(&(pad as u32).to_be_bytes());
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc(&body).to_be_bytes());
    out.extend_from_slice(&base[iend..]);
    out
}

/// A real baseline JPEG from the `image` crate's encoder.
pub(super) fn jpeg_of(w: u32, h: u32) -> Vec<u8> {
    let mut rgb = Vec::new();
    for y in 0..h {
        for x in 0..w {
            rgb.extend_from_slice(&[(x * 255 / w.max(1)) as u8, (y * 255 / h.max(1)) as u8, 40]);
        }
    }
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85)
        .encode(&rgb, w, h, image::ExtendedColorType::Rgb8)
        .unwrap();
    out
}

/// The same JPEG with an EXIF (APP1) segment right after SOI, as a camera or phone writes it.
pub(super) fn with_exif(jpeg: &[u8]) -> Vec<u8> {
    let mut exif = b"Exif\0\0MM\0*\0\0\0\x08\0\0".to_vec();
    exif.extend_from_slice(&[0; 6]);
    let len = (exif.len() + 2) as u16;
    let mut out = jpeg[..2].to_vec();
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(&exif);
    out.extend_from_slice(&jpeg[2..]);
    out
}

#[test]
fn real_jpeg_attachments_are_decoded_and_shown_including_exif_and_unusual_sizes() {
    let d = dir("img-jpeg");
    let p = d.join("n.omatree");
    let (a, _) = new_note(&p);
    let cases = [(31u32, 17u32), (1, 1), (1, 300), (400, 2)];
    let mut body = String::from("jpegy\n\n");
    let mut doc = Document::open(&p).unwrap();
    let mut want = Vec::new();
    for (i, (w, hgt)) in cases.into_iter().enumerate() {
        let bytes = if i == 0 {
            with_exif(&jpeg_of(w, hgt))
        } else {
            jpeg_of(w, hgt)
        };
        let r = doc.put_attachment(&bytes).unwrap();
        assert!(r.ends_with(".jpg"));
        body.push_str(&format!("![j{i}]({r})\n\n"));
        want.push((format!("img:{r}"), [w as usize, hgt as usize]));
    }
    doc.set_body(a, &body).unwrap();
    doc.save().unwrap();
    drop(doc);
    let mut h = open_note(&p);
    for _ in 0..4 {
        h.frame(vec![], Modifiers::NONE);
    }
    want.sort();
    assert_eq!(textures(&h), want, "every JPEG decoded to its own size");
    assert_eq!(h.host.buffer(), body, "the source is untouched");
}

#[test]
fn external_and_malformed_references_never_load_anything_and_stay_editable_text() {
    let d = dir("img-external");
    let p = d.join("n.omatree");
    // A decoy exactly where a file-backed store would look.
    fs::create_dir_all(d.join("assets")).unwrap();
    let decoy = encode_png(&shot(5, 5)).unwrap();
    fs::write(d.join("assets/decoy.png"), &decoy).unwrap();
    fs::write(d.join("secret.png"), &decoy).unwrap();
    let id = "ab".repeat(16);
    let body = format!(
        "![a](https://example.com/x.png)\n\n![b](http://127.0.0.1:9/x.png)\n\n![c](file:///etc/passwd)\n\n\
         ![d](/etc/passwd)\n\n![e](../secret.png)\n\n![f](assets/decoy.png)\n\n![g](secret.png)\n\n\
         ![h](assets/{id}.PNG)\n\n![i](assets/{}.png)\n\n![j](assets/{id}.gif)\n\n![k](assets/{id}.png)\n",
        &id[..31]
    );
    let mut doc = Document::open(&p).unwrap();
    let n = doc.create_root("Vnější").unwrap();
    doc.set_body(n, &body).unwrap();
    doc.save().unwrap();
    drop(doc);
    let before = fs::read(&p).unwrap();
    let mut h = H::open(&p);
    h.select("Vnější");
    for _ in 0..5 {
        h.frame(vec![], Modifiers::NONE);
    }
    assert!(
        textures(&h).is_empty(),
        "nothing was loaded: {:?}",
        textures(&h)
    );
    assert_eq!(h.host.buffer(), body);
    assert!(!h.host.is_dirty());
    assert_eq!(fs::read(&p).unwrap(), before);
    // The note stays editable.
    let end = h.host.buffer().len();
    h.set_caret(end, end);
    h.type_text("více");
    assert!(h.host.buffer().ends_with("více"));
}

#[test]
fn a_damaged_stored_image_is_reported_as_damaged_the_note_stays_editable_and_nothing_is_pruned() {
    let d = dir("img-damaged");
    let p = d.join("n.omatree");
    let (a, _) = new_note(&p);
    let mut doc = Document::open(&p).unwrap();
    let r = doc
        .put_attachment(&encode_png(&shot(9, 9)).unwrap())
        .unwrap();
    doc.set_body(a, &format!("![x]({r})\n")).unwrap();
    doc.save().unwrap();
    drop(doc);
    // Truncate the stored image, keeping the recorded size consistent: damage, not a size error.
    {
        let c = rusqlite::Connection::open(&p).unwrap();
        c.execute(
            "UPDATE attachments SET data = substr(data, 1, 40), byte_len = 40",
            [],
        )
        .unwrap();
    }
    let mut h = open_note(&p);
    for _ in 0..3 {
        h.frame(vec![], Modifiers::NONE);
    }
    assert!(textures(&h).is_empty());
    assert!(h.host.status().contains("damaged"), "{}", h.host.status());
    assert_eq!(*h.host.dialog(), Dialog::None, "no intrusive dialog");
    h.type_text("a");
    assert!(h.host.buffer().contains(&r) && h.host.buffer().ends_with('a'));
    // The damaged image is still referenced, so no prune would ever list it.
    h.key(Key::S, CTRL);
    let doc = h.host.document().unwrap();
    let preview = doc.preview_attachment_prune();
    assert!(preview.is_err() || preview.unwrap().report.eligible.is_empty());
}

#[test]
fn notebooks_with_the_same_ids_and_the_same_reference_text_never_share_a_texture() {
    let d = dir("img-isolation");
    let (pa, pb) = (d.join("a.omatree"), d.join("b.omatree"));
    let mut refs_seen: Vec<String> = Vec::new();
    for (path, w) in [(&pa, 11usize), (&pb, 23usize)] {
        let mut doc = Document::open(path).unwrap();
        let n = doc.create_root("Obrázky").unwrap();
        let r = doc
            .put_attachment(&encode_png(&shot(w, 7)).unwrap())
            .unwrap();
        doc.set_body(n, &format!("![x]({r})\n")).unwrap();
        doc.save().unwrap();
        refs_seen.push(r);
    }
    // Give the second notebook's own image the first one's id, and its note the same text:
    // identical node ids, identical reference text, different bytes.
    {
        let (first, second) = (id_only(&refs_seen[0]), id_only(&refs_seen[1]));
        let c = rusqlite::Connection::open(&pb).unwrap();
        c.execute(
            "UPDATE attachments SET id = ?1 WHERE id = ?2",
            [&first, &second],
        )
        .unwrap();
        c.execute(
            "UPDATE nodes SET body = replace(body, ?1, ?2)",
            [&second, &first],
        )
        .unwrap();
        refs_seen[1] = refs_seen[0].clone();
    }
    assert_eq!(refs_seen[0], refs_seen[1], "identical textual reference");
    let mut h = H::open(&pa);
    h.select("Obrázky");
    h.settle();
    assert_eq!(
        textures(&h),
        vec![(format!("img:{}", refs_seen[0]), [11, 7])]
    );
    let ctx = h.ctx.clone();
    assert!(h.host.open(&ctx, &pb));
    h.select("Obrázky");
    h.settle();
    assert_eq!(
        textures(&h),
        vec![(format!("img:{}", refs_seen[0]), [23, 7])],
        "the second notebook shows its own image, and the first one's texture is gone"
    );
}

fn trash_note(h: &mut H, title: &str) {
    let ctx = h.ctx.clone();
    let id = h.id(title);
    h.host.select(&ctx, id);
    h.host.confirm_delete(&ctx, id);
    h.settle();
}

/// Paste one image into "Obrázky" of a fresh saved notebook and save it.
fn pasted_and_saved(name: &str) -> (PathBuf, PathBuf, H, String, Vec<u8>) {
    let d = dir(name);
    let p = d.join("n.omatree");
    new_note(&p);
    let mut h = open_note(&p);
    let img = shot(20, 12);
    paste(&mut h, &img);
    h.key(Key::S, CTRL);
    assert!(!h.host.is_dirty(), "{}", h.host.status());
    let r = refs(&h.body("Obrázky")).remove(0);
    (d, p, h, r, encode_png(&img).unwrap())
}

#[test]
fn trash_checkpoint_save_as_and_reload_all_keep_and_show_the_original_image() {
    let (d, p, mut h, r, bytes) = pasted_and_saved("img-recovery");
    let ctx = h.ctx.clone();
    let tex = |h: &H| {
        textures(h)
            .into_iter()
            .filter(|t| t.0 == format!("img:{r}"))
            .count()
    };
    assert_eq!(tex(&h), 1);

    // Delete the note: the image bytes stay in the file (nothing deletes attachments).
    trash_note(&mut h, "Obrázky");
    h.key(Key::S, CTRL);
    assert_eq!(stored_bytes(&p, &r), bytes);
    // Restore it from Trash: its image is shown again, byte for byte.
    h.host.open_recovery(&ctx);
    h.settle();
    assert!(h.host.restore_trash(&ctx, 0), "{}", h.host.status());
    h.settle();
    h.host.cancel_dialog();
    h.select("Obrázky");
    assert!(h.host.buffer().contains(&r));
    assert_eq!(tex(&h), 1);
    h.key(Key::S, CTRL);

    // Delete again (which checkpoints the note with its image) and restore that checkpoint.
    trash_note(&mut h, "Obrázky");
    h.host.open_recovery(&ctx);
    h.settle();
    assert!(h.host.restore_checkpoint(&ctx, 0), "{}", h.host.status());
    h.settle();
    h.host.cancel_dialog();
    h.select("Obrázky");
    assert!(h.host.buffer().contains(&r), "{}", h.host.buffer());
    h.settle();
    assert_eq!(tex(&h), 1);
    h.key(Key::S, CTRL);
    assert_eq!(stored_bytes(&p, &r), bytes);

    // Save As into another file keeps the reference and the exact bytes.
    let copy = d.join("copy.omatree");
    h.host.start_save_as_typed(&ctx);
    h.host.set_save_as_path(&copy.display().to_string());
    h.host.submit_save_as(&ctx);
    h.settle();
    assert_eq!(stored_bytes(&copy, &r), bytes);
    assert!(Document::open_existing(&copy)
        .unwrap()
        .notebook()
        .depth_first()
        .iter()
        .any(|n| n.body().contains(&r)));

    // Reload from Disk: a new notebook, so a new decode; the old textures are gone.
    assert!(h.host.reload_from_disk(&ctx));
    h.select("Obrázky");
    h.settle();
    assert_eq!(tex(&h), 1);
    assert_eq!(textures(&h).len(), 1);
}

#[cfg(unix)]
#[test]
fn a_failed_or_conflicting_save_keeps_the_pending_image_and_nothing_is_lost() {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    let d = dir("img-failed-save");
    let p = d.join("n.omatree");
    new_note(&p);
    let mut h = open_note(&p);
    let img = shot(15, 15);
    paste(&mut h, &img);
    let r = refs(h.host.buffer()).remove(0);
    // A save that cannot write.
    fs::set_permissions(&d, fs::Permissions::from_mode(0o555)).unwrap();
    h.key(Key::S, CTRL);
    fs::set_permissions(&d, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(h.host.is_dirty(), "still dirty after a failed save");
    assert_eq!(
        h.host.document().unwrap().pending_attachments().0,
        1,
        "the pending image is kept"
    );
    assert!(h.host.buffer().contains(&r));
    // An external write (the file changed under us): Save is refused, nothing is overwritten.
    {
        let c = rusqlite::Connection::open(&p).unwrap();
        c.execute(
            "UPDATE nodes SET title = title || 'x' WHERE title = 'Druhá'",
            [],
        )
        .unwrap();
    }
    h.key(Key::S, CTRL);
    assert!(h.host.is_dirty());
    assert_eq!(h.host.document().unwrap().pending_attachments().0, 1);
    assert!(Document::open_existing(&p)
        .unwrap()
        .stored_attachment_ids()
        .unwrap()
        .is_empty());
    // Save As is the way out and carries the image.
    let mine = d.join("mine.omatree");
    let ctx = h.ctx.clone();
    h.host.cancel_dialog();
    h.host.start_save_as_typed(&ctx);
    h.host.set_save_as_path(&mine.display().to_string());
    h.host.submit_save_as(&ctx);
    h.settle();
    assert!(!h.host.is_dirty(), "{}", h.host.status());
    assert_eq!(stored_bytes(&mine, &r), encode_png(&img).unwrap());
}

#[test]
fn switching_notes_right_after_a_paste_loses_nothing_and_autosave_commits_it() {
    let d = dir("img-switch");
    let p = d.join("n.omatree");
    new_note(&p);
    let mut h = open_note(&p);
    let img = shot(18, 10);
    paste(&mut h, &img);
    let r = refs(h.host.buffer()).remove(0);
    h.select("Druhá"); // before the autosave fires
    assert!(
        h.body("Obrázky").contains(&r),
        "the reference reached the document"
    );
    assert_eq!(h.host.document().unwrap().pending_attachments().0, 1);
    h.advance(3.0);
    h.advance(3.0);
    assert!(!h.host.is_dirty(), "{}", h.host.status());
    assert_eq!(stored_bytes(&p, &r), encode_png(&img).unwrap());
    h.select("Obrázky");
    h.settle();
    assert_eq!(textures(&h).len(), 1);
}

#[test]
fn closing_with_a_pending_image_offers_save_discard_and_cancel() {
    let d = dir("img-close");
    for answer in ["cancel", "save", "discard"] {
        let p = d.join(format!("{answer}.omatree"));
        new_note(&p);
        let mut h = open_note(&p);
        paste(&mut h, &shot(14, 14));
        let r = refs(h.host.buffer()).remove(0);
        // Autosave must not get there first.
        assert!(!h.request_close(), "asks first");
        assert!(
            matches!(h.host.dialog(), Dialog::Unsaved { .. }),
            "{:?}",
            h.host.dialog()
        );
        match answer {
            "cancel" => {
                h.host.cancel_dialog();
                assert!(h.host.is_dirty());
                assert_eq!(h.host.document().unwrap().pending_attachments().0, 1);
                assert!(h.host.buffer().contains(&r));
            }
            "save" => {
                h.host.unsaved_save(&h.ctx.clone());
                h.settle();
                assert!(h.host.quit_allowed(), "{}", h.host.status());
                assert!(Document::open_existing(&p).unwrap().attachment(&r).is_ok());
            }
            _ => {
                h.host.unsaved_discard();
                h.host.confirm_discard(&h.ctx.clone());
                h.settle();
                assert!(Document::open_existing(&p)
                    .unwrap()
                    .stored_attachment_ids()
                    .unwrap()
                    .is_empty());
            }
        }
    }
}

#[test]
fn a_width_annotation_is_text_only_and_never_touches_the_image_bytes() {
    let (_d, p, mut h, r, bytes) = pasted_and_saved("img-width");
    let ids_before = Document::open_existing(&p)
        .unwrap()
        .stored_attachment_ids()
        .unwrap();
    // The annotation the slider writes, typed by hand after the reference.
    let body = h.host.buffer().to_owned();
    let at = body.find(&r).unwrap() + r.len() + 1; // after the closing parenthesis
    h.set_caret(at, at);
    h.type_text("{width=40%}");
    assert!(h.host.buffer().contains("{width=40%}"));
    h.key(Key::S, CTRL);
    assert_eq!(stored_bytes(&p, &r), bytes, "not re-encoded");
    assert_eq!(
        Document::open_existing(&p)
            .unwrap()
            .stored_attachment_ids()
            .unwrap(),
        ids_before
    );
    assert_eq!(
        h.host.document().unwrap().pending_attachments().0,
        0,
        "no new attachment"
    );
    assert_eq!(textures(&h).len(), 1);
}

#[test]
fn long_notes_with_several_images_decode_each_image_once() {
    let d = dir("img-long");
    let p = d.join("n.omatree");
    let (a, _) = new_note(&p);
    let mut doc = Document::open(&p).unwrap();
    let mut body = String::new();
    for i in 0..6 {
        let r = doc
            .put_attachment(&encode_png(&shot(30 + i, 20)).unwrap())
            .unwrap();
        body.push_str(&format!(
            "{}\n\n![i{i}]({r})\n\n",
            "řádek žluťoučký kůň ".repeat(40)
        ));
    }
    doc.set_body(a, &body).unwrap();
    doc.save().unwrap();
    drop(doc);
    let mut h = open_note(&p);
    for _ in 0..4 {
        h.frame(vec![], Modifiers::NONE);
    }
    let first = textures(&h);
    assert!(!first.is_empty() && first.len() <= 6);
    // Many more frames: the set of textures does not change (no re-decode, no leak).
    for _ in 0..30 {
        h.frame(vec![], Modifiers::NONE);
    }
    assert_eq!(textures(&h), first);
}

/// Writes the disposable notebooks used for the manual native-window check of E10 into
/// `$OMATREE_GUI_DIR` (`cargo test -p omatree-egui -- --ignored write_gui_fixtures`).
#[test]
#[ignore = "writes fixtures for the manual GUI check"]
fn write_gui_fixtures() {
    let dir = PathBuf::from(std::env::var("OMATREE_GUI_DIR").expect("set OMATREE_GUI_DIR"));
    fs::create_dir_all(&dir).unwrap();
    // An empty notebook to paste into.
    let mut doc = Document::open(&dir.join("paste.omatree")).unwrap();
    let n = doc.create_root("Vložení").unwrap();
    doc.set_body(n, "Sem vložím obrázek:\n\n").unwrap();
    doc.save().unwrap();
    // JPEG (with EXIF), a missing image, a damaged one, and remote and filesystem references.
    let mut doc = Document::open(&dir.join("mixed.omatree")).unwrap();
    let n = doc.create_root("Smíšené").unwrap();
    let jpg = doc.put_attachment(&with_exif(&jpeg_of(240, 140))).unwrap();
    let png = doc
        .put_attachment(&encode_png(&shot(200, 120)).unwrap())
        .unwrap();
    let missing = format!("assets/{}.png", "cd".repeat(16));
    doc.set_body(
        n,
        &format!(
            "JPEG:\n\n![jpeg]({jpg})\n\nPNG:\n\n![png]({png})\n\nChybí:\n\n![missing]({missing})\n\n\
             Vzdálený:\n\n![remote](https://example.com/x.png)\n\n![file](file:///etc/passwd)\n\n![abs](/etc/hostname)\n\n![rel](../secret.png)\n\nKonec.\n"
        ),
    )
    .unwrap();
    doc.save().unwrap();
    // Two notebooks with the same note id and the same reference text but different images.
    for (name, w, colour) in [("same-a", 160usize, 0u8), ("same-b", 90, 200)] {
        let path = dir.join(format!("{name}.omatree"));
        let mut doc = Document::open(&path).unwrap();
        let n = doc.create_root("Stejné").unwrap();
        let mut img = shot(w, 90);
        for p in &mut img.pixels {
            *p = egui::Color32::from_rgb(p.r().max(colour), p.g(), p.b());
        }
        let r = doc.put_attachment(&encode_png(&img).unwrap()).unwrap();
        doc.set_body(n, &format!("![x]({r})\n")).unwrap();
        doc.save().unwrap();
    }
    let a = rusqlite::Connection::open(dir.join("same-a.omatree")).unwrap();
    let id: String = a
        .query_row("SELECT id FROM attachments", [], |r| r.get(0))
        .unwrap();
    let b = rusqlite::Connection::open(dir.join("same-b.omatree")).unwrap();
    let old: String = b
        .query_row("SELECT id FROM attachments", [], |r| r.get(0))
        .unwrap();
    b.execute("UPDATE attachments SET id = ?1", [&id]).unwrap();
    b.execute("UPDATE nodes SET body = replace(body, ?1, ?2)", [&old, &id])
        .unwrap();
}

#[cfg(not(target_os = "linux"))]
fn rss_kib() -> i64 {
    0 // `/proc` is Linux-only
}

#[cfg(target_os = "linux")]
fn rss_kib() -> i64 {
    fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("VmRSS:"))
        .and_then(|v| v.trim().trim_end_matches("kB").trim().parse().ok())
        .unwrap_or(0)
}

/// Timings and memory of the image paths (`cargo test --release -p omatree-egui --
/// --ignored --nocapture image_performance`). Indicative: one machine, headless frames, no GPU.
#[test]
#[ignore = "measurement, run with --release"]
fn image_performance() {
    use std::time::Instant;
    let ms = |t: Instant| t.elapsed().as_secs_f64() * 1000.0;
    // Paste latency of a full-screen screenshot (encode to PNG, store, insert the reference).
    let d = dir("img-perf");
    let p = d.join("n.omatree");
    new_note(&p);
    let mut h = open_note(&p);
    let big = shot(1920, 1080);
    let t = Instant::now();
    h.frame(vec![Event::PasteImage(Arc::new(big))], Modifiers::NONE);
    println!(
        "paste of a 1920x1080 screenshot (one frame, incl. PNG encode and decode): {:.0} ms",
        ms(t)
    );
    let t = Instant::now();
    h.frame(vec![], Modifiers::NONE);
    println!("next frame, image cached: {:.1} ms", ms(t));

    // A long document with 24 images of moderate size.
    let q = d.join("long.omatree");
    let mut doc = Document::open(&q).unwrap();
    let a = doc.create_root("Dlouhá").unwrap();
    let b = doc.create_root("Druhá").unwrap();
    let mut body = String::new();
    let mut pixels = 0usize;
    for i in 0..24 {
        let (w, hh) = (640 + i, 360);
        pixels += w * hh;
        let r = doc
            .put_attachment(&encode_png(&shot(w, hh)).unwrap())
            .unwrap();
        body.push_str(&format!(
            "{}\n\n![i{i}]({r})\n\n",
            "řádek žluťoučký kůň úpěl ďábelské ódy ".repeat(30)
        ));
    }
    doc.set_body(a, &body).unwrap();
    doc.set_body(b, "krátká\n").unwrap();
    doc.save().unwrap();
    drop(doc);
    let rss0 = rss_kib();
    let t = Instant::now();
    let mut h = H::open(&q);
    h.select("Dlouhá");
    println!(
        "open the notebook and show the first screen of the 24-image note: {:.0} ms",
        ms(t)
    );
    let t = Instant::now();
    for _ in 0..5 {
        h.frame(vec![], Modifiers::NONE);
    }
    println!(
        "five more frames: {:.1} ms each (textures held: {})",
        ms(t) / 5.0,
        textures(&h).len()
    );
    let t = Instant::now();
    for _ in 0..50 {
        h.frame(vec![], Modifiers::NONE);
    }
    println!("steady state: {:.2} ms per frame", ms(t) / 50.0);
    let t = Instant::now();
    h.select("Druhá");
    println!("switch to a short note: {:.1} ms", ms(t));
    let t = Instant::now();
    h.select("Dlouhá");
    println!("switch back (images cached): {:.1} ms", ms(t));
    println!(
        "memory: {} MiB of decoded pixels held for {} textures; resident grew by {} MiB",
        pixels * 4 / (1024 * 1024),
        textures(&h).len(),
        (rss_kib() - rss0) / 1024
    );
}
