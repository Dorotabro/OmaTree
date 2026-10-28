//! Export of the active note (ticket E11a): Markdown, folder, ZIP and PDF, through real frames of
//! a `Host` on disposable notebooks, with independent readers (`python3 zipfile`, `unzip`,
//! `pdfinfo`, `pdftotext`, `pdfimages`, `qpdf`) checking the files produced.

use super::image_tests::{jpeg_of, paste, refs, shot, stored_bytes, with_exif};
use super::*;
use crate::note_export::{suggest_stem, ExportFormat};
use omatree_editor::assets::encode_png;
use std::process::Command;

const TITLE: &str = "Příliš žluťoučký 🦀: plán/1";

fn have(tool: &str) -> bool {
    Command::new(tool)
        .arg("-v")
        .output()
        .map(|_| true)
        .unwrap_or(false)
}

/// A saved notebook: a rich note (every Markdown feature the PDF renders, Czech text and emoji,
/// a PNG and a JPEG, a width annotation), an orphan image, a Trash-only image, and a plain note.
fn rich(path: &Path) -> (Vec<u8>, Vec<u8>, String) {
    let mut doc = Document::open(path).unwrap();
    let png = encode_png(&shot(64, 40)).unwrap();
    let jpg = with_exif(&jpeg_of(48, 30));
    let rp = doc.put_attachment(&png).unwrap();
    let rj = doc.put_attachment(&jpg).unwrap();
    let orphan = doc
        .put_attachment(&encode_png(&shot(9, 9)).unwrap())
        .unwrap();
    let trashed = doc
        .put_attachment(&encode_png(&shot(11, 7)).unwrap())
        .unwrap();
    let note = doc.create_root(TITLE).unwrap();
    let body = format!(
        "# Nadpis žluťoučký 🦀\n\nOdstavec s **tučným** a [odkazem](https://example.com/cesta).\n\n\
         - položka jedna\n- [x] hotový úkol\n- [ ] otevřený úkol\n\n\
         | Název | Počet |\n|---|---|\n| jablka 🍎 | 3 |\n| hrušky | 5 |\n\n\
         > citace o kůni\n\n```rust\nfn main() {{ println!(\"ahoj\"); }}\n```\n\n\
         ![png]({rp}){{width=50%}}\n\n![jpeg]({rj})\n"
    );
    doc.set_body(note, &body).unwrap();
    let other = doc.create_root("Druhá").unwrap();
    doc.set_body(other, &format!("![o]({orphan})\n")).unwrap();
    let gone = doc.create_root("Koš").unwrap();
    doc.set_body(gone, &format!("![t]({trashed})\n")).unwrap();
    doc.save().unwrap();
    doc.delete(gone).unwrap();
    doc.set_body(other, "bez obrázku\n").unwrap();
    doc.save().unwrap();
    (png, jpg, body)
}

impl H {
    /// Export the selected note through the host's own dialogs (typed destination).
    fn export(&mut self, format: ExportFormat, dest: &Path) {
        let ctx = self.ctx.clone();
        self.host.start_export(&ctx);
        self.settle();
        self.host.set_export_format(format);
        self.host.choose_export_destination(&ctx);
        self.settle();
        self.host.set_export_path(&dest.display().to_string());
        self.host.submit_export_path(&ctx);
        self.settle();
    }

    pub(super) fn state(&self) -> (bool, usize, usize, String) {
        let d = self.host.document().unwrap();
        (
            self.host.is_dirty(),
            d.recovery().checkpoints().len(),
            d.pending_attachments().0,
            self.host.buffer().to_owned(),
        )
    }
}

fn rich_host(name: &str) -> (PathBuf, PathBuf, H, Vec<u8>, Vec<u8>, String) {
    let d = dir(name);
    let p = d.join("n.omatree");
    let (png, jpg, body) = rich(&p);
    let mut h = H::open(&p);
    h.select(TITLE);
    (d, p, h, png, jpg, body)
}

fn zip_entries(zip: &Path) -> Vec<(String, Vec<u8>)> {
    let script = "import sys,zipfile,json\nz=zipfile.ZipFile(sys.argv[1])\nassert z.testzip() is None\nprint(json.dumps([[i.filename, list(z.read(i.filename))] for i in z.infolist()]))";
    let out = Command::new("python3")
        .args(["-I", "-c", script])
        .arg(zip)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    // [["name", [1,2,3]], ...] without a JSON crate: parse by hand.
    let mut entries = Vec::new();
    let text = text.trim();
    let mut rest = &text[1..text.len() - 1];
    while let Some(start) = rest.find("[\"") {
        rest = &rest[start + 2..];
        let end = rest.find('"').unwrap();
        let name = rest[..end].to_owned();
        let l = rest.find('[').unwrap();
        let r = rest.find(']').unwrap();
        let bytes = rest[l + 1..r]
            .split(',')
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().parse::<u8>().unwrap())
            .collect();
        entries.push((name, bytes));
        rest = &rest[r + 1..];
    }
    entries
}

fn leftovers(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with('.'))
        .collect()
}

#[test]
fn nothing_is_exported_without_a_selected_note() {
    let d = dir("exp-none");
    let mut h = H::bare(Vec2::new(1000.0, 700.0));
    let ctx = h.ctx.clone();
    h.host.start_untitled(&ctx);
    h.settle();
    h.host.start_export(&ctx);
    h.settle();
    assert_eq!(*h.host.dialog(), Dialog::None);
    assert!(
        h.host.status().contains("Select a note to export"),
        "{}",
        h.host.status()
    );
    assert_eq!(fs::read_dir(&d).unwrap().count(), 0);
}

#[test]
fn plain_markdown_is_the_note_byte_for_byte_with_a_title_derived_name() {
    let (d, _p, mut h, _png, _jpg, body) = rich_host("exp-md");
    // The suggested name comes from the title, made safe, Czech letters and emoji kept.
    let ctx = h.ctx.clone();
    h.host.start_export(&ctx);
    h.host.set_export_format(ExportFormat::Markdown);
    h.host.choose_export_destination(&ctx);
    h.settle();
    let Dialog::ExportPath { path, .. } = h.host.dialog().clone() else {
        panic!("{:?}", h.host.dialog())
    };
    assert!(path.ends_with("Příliš žluťoučký 🦀_ plán_1.md"), "{path}");
    assert_eq!(suggest_stem(TITLE), "Příliš žluťoučký 🦀_ plán_1");
    assert_eq!(suggest_stem("  ...  "), "note");
    h.host.cancel_dialog();
    // Export under a Czech name; the note's text, widths and links are untouched.
    let dest = d.join("Úkol žluťoučký 🦀.md");
    let before = h.state();
    h.export(ExportFormat::Markdown, &dest);
    assert_eq!(*h.host.dialog(), Dialog::None, "{}", h.host.status());
    assert_eq!(fs::read(&dest).unwrap(), body.as_bytes());
    assert!(body.contains("{width=50%}"));
    assert!(h.host.status().contains("text only") && h.host.status().contains("not included"));
    assert!(leftovers(&d).is_empty());
    assert_eq!(h.state(), before, "export changed nothing");
}

#[test]
fn unsaved_edits_and_pending_images_are_exported_and_stay_unsaved() {
    let (d, p, mut h, _png, _jpg, body) = rich_host("exp-unsaved");
    let end = h.host.buffer().len();
    h.set_caret(end, end);
    h.type_text("\nnovinka neuložená\n");
    paste(&mut h, &shot(20, 20));
    let text = h.host.buffer().to_owned();
    let new_ref = refs(&text).pop().unwrap();
    assert!(text.contains("novinka neuložená") && text.starts_with(&body));
    assert!(h.host.is_dirty());
    let on_disk = fs::read(&p).unwrap();
    let before = h.state();
    let dest = d.join("pending.zip");
    h.export(ExportFormat::Zip, &dest);
    assert_eq!(*h.host.dialog(), Dialog::None, "{}", h.host.status());
    let entries = zip_entries(&dest);
    let md = entries.iter().find(|e| e.0.ends_with(".md")).unwrap();
    assert_eq!(md.1, text.as_bytes(), "the unsaved text, exactly");
    let img = entries
        .iter()
        .find(|e| e.0 == new_ref)
        .expect("the pending image is in the archive");
    assert_eq!(img.1, encode_png(&shot(20, 20)).unwrap());
    assert_eq!(
        h.state(),
        before,
        "still dirty, still pending, nothing checkpointed"
    );
    assert_eq!(
        fs::read(&p).unwrap(),
        on_disk,
        "the notebook file was not written"
    );
}

#[test]
fn an_untitled_notebook_exports_without_being_saved() {
    let d = dir("exp-untitled");
    let mut h = H::bare(Vec2::new(1000.0, 700.0));
    let ctx = h.ctx.clone();
    h.host.start_untitled(&ctx);
    h.host.create_root(&ctx);
    h.host.focus(Focus::Editor);
    h.settle();
    h.type_text("untitled text ž\n");
    paste(&mut h, &shot(12, 12));
    let r = refs(h.host.buffer()).remove(0);
    h.export(ExportFormat::Folder, &d.join("out"));
    assert_eq!(*h.host.dialog(), Dialog::None, "{}", h.host.status());
    assert_eq!(
        fs::read(d.join("out").join(&r)).unwrap(),
        encode_png(&shot(12, 12)).unwrap()
    );
    assert!(
        !h.host.document().unwrap().has_path(),
        "no notebook file was created"
    );
    assert!(h.host.is_dirty());
    assert_eq!(fs::read_dir(&d).unwrap().count(), 1, "only the export");
}

#[test]
fn the_folder_holds_the_note_and_only_the_images_it_uses_byte_for_byte() {
    let (d, _p, mut h, png, jpg, body) = rich_host("exp-folder");
    let before = h.state();
    let out = d.join("Složka exportu");
    h.export(ExportFormat::Folder, &out);
    assert_eq!(*h.host.dialog(), Dialog::None, "{}", h.host.status());
    let r = refs(&body);
    assert_eq!(r.len(), 2);
    let mut files: Vec<String> = Vec::new();
    for e in walk(&out) {
        files.push(e.strip_prefix(&out).unwrap().to_string_lossy().into_owned());
    }
    files.sort();
    let mut want = vec![
        r[0].clone(),
        r[1].clone(),
        format!("{}.md", suggest_stem(TITLE)),
    ];
    want.sort();
    assert_eq!(files, want, "no orphan, no Trash-only image, nothing else");
    assert!(r[0].starts_with("assets/") && r[0].ends_with(".png") && r[1].ends_with(".jpg"));
    assert_eq!(fs::read(out.join(&r[0])).unwrap(), png);
    assert_eq!(fs::read(out.join(&r[1])).unwrap(), jpg);
    assert_eq!(
        fs::read(out.join(format!("{}.md", suggest_stem(TITLE)))).unwrap(),
        body.as_bytes()
    );
    assert!(leftovers(&d).is_empty());
    assert_eq!(h.state(), before);
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}

#[test]
fn the_zip_is_valid_and_has_identical_entries() {
    let (d, _p, mut h, png, jpg, body) = rich_host("exp-zip");
    let dest = d.join("note.zip");
    h.export(ExportFormat::Zip, &dest);
    assert_eq!(*h.host.dialog(), Dialog::None, "{}", h.host.status());
    let entries = zip_entries(&dest);
    let r = refs(&body);
    assert_eq!(
        entries.len(),
        3,
        "{:?}",
        entries.iter().map(|e| &e.0).collect::<Vec<_>>()
    );
    assert_eq!(entries.iter().find(|e| e.0 == r[0]).unwrap().1, png);
    assert_eq!(entries.iter().find(|e| e.0 == r[1]).unwrap().1, jpg);
    assert_eq!(
        entries.iter().find(|e| e.0.ends_with(".md")).unwrap().1,
        body.as_bytes()
    );
    for (name, _) in &entries {
        assert!(!name.starts_with('/') && !name.contains(".."), "{name}");
    }
    if have("unzip") {
        let t = Command::new("unzip").arg("-t").arg(&dest).output().unwrap();
        assert!(t.status.success(), "{}", String::from_utf8_lossy(&t.stdout));
    }
}

#[test]
fn the_pdf_is_valid_shows_the_text_the_table_the_code_and_embeds_both_images() {
    if !have("pdfinfo") || !have("pdftotext") || !have("pdfimages") {
        eprintln!("SKIPPED: poppler tools are not installed; PDF content UNVERIFIED here");
        return;
    }
    let (d, _p, mut h, _png, _jpg, _body) = rich_host("exp-pdf");
    let dest = d.join("note.pdf");
    let before = h.state();
    h.export(ExportFormat::Pdf, &dest);
    assert_eq!(*h.host.dialog(), Dialog::None, "{}", h.host.status());
    assert!(h.host.status().contains("2 images"), "{}", h.host.status());
    assert_eq!(h.state(), before);
    let info = Command::new("pdfinfo").arg(&dest).output().unwrap();
    assert!(info.status.success());
    let info = String::from_utf8_lossy(&info.stdout).to_string();
    assert!(info.contains("Pages:"), "{info}");
    let text = Command::new("pdftotext")
        .args(["-layout"])
        .arg(&dest)
        .arg("-")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&text.stdout).to_string();
    for want in [
        "Nadpis",
        "Odstavec",
        "tučným",
        "položka jedna",
        "úkol",
        "jablka",
        "hrušky",
        "citace",
        "println",
        "ahoj",
    ] {
        assert!(
            text.contains(want),
            "missing {want:?} in the extracted text:\n{text}"
        );
    }
    let images = Command::new("pdfimages")
        .arg("-list")
        .arg(&dest)
        .output()
        .unwrap();
    let rows = String::from_utf8_lossy(&images.stdout)
        .lines()
        .filter(|l| l.trim_start().starts_with(|c: char| c.is_ascii_digit()))
        .count();
    assert!(rows >= 2, "two images embedded, found {rows}");
    if have("qpdf") {
        let c = Command::new("qpdf")
            .arg("--check")
            .arg(&dest)
            .output()
            .unwrap();
        assert!(c.status.success(), "{}", String::from_utf8_lossy(&c.stdout));
    }
}

#[test]
fn a_missing_or_damaged_image_fails_the_export_and_leaves_no_artifact() {
    let d = dir("exp-bad");
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    let good = doc
        .put_attachment(&encode_png(&shot(8, 8)).unwrap())
        .unwrap();
    let n = doc.create_root("Vadný").unwrap();
    let missing = format!("assets/{}.png", "ab".repeat(16));
    doc.set_body(n, &format!("![g]({good})\n![m]({missing})\n"))
        .unwrap();
    doc.save().unwrap();
    drop(doc);
    let mut h = H::open(&p);
    h.select("Vadný");
    let before = h.state();
    for (f, name) in [
        (ExportFormat::Folder, "f"),
        (ExportFormat::Zip, "z.zip"),
        (ExportFormat::Pdf, "p.pdf"),
    ] {
        let dest = d.join(name);
        h.export(f, &dest);
        assert!(
            h.host.status().starts_with("Export failed"),
            "{f:?}: {}",
            h.host.status()
        );
        assert!(
            h.host.status().contains("not found") || h.host.status().contains("not a supported"),
            "{}",
            h.host.status()
        );
        assert!(!dest.exists(), "{f:?}: an artifact was left");
        assert!(leftovers(&d).is_empty(), "{f:?}: {:?}", leftovers(&d));
        h.host.cancel_dialog();
    }
    // Markdown carries no images, so it still works and says so.
    h.export(ExportFormat::Markdown, &d.join("ok.md"));
    assert!(d.join("ok.md").exists());
    assert_eq!(h.state(), before);

    // A damaged stored image is an error, named as damaged, not silently skipped.
    {
        let c = rusqlite::Connection::open(&p).unwrap();
        c.execute(
            "UPDATE attachments SET data = substr(data, 1, 40), byte_len = 40",
            [],
        )
        .unwrap();
    }
    let mut doc = Document::open(&p).unwrap();
    let n = doc.create_root("Poškozený").unwrap();
    doc.set_body(n, &format!("![g]({good})\n")).unwrap();
    doc.save().unwrap();
    drop(doc);
    let mut h = H::open(&p);
    h.select("Poškozený");
    let dest = d.join("damaged.zip");
    h.export(ExportFormat::Zip, &dest);
    assert!(
        h.host.status().starts_with("Export failed"),
        "{}",
        h.host.status()
    );
    assert!(h.host.status().contains("damaged"), "{}", h.host.status());
    assert!(!dest.exists());
    assert!(leftovers(&d).is_empty());
}

#[test]
fn external_references_are_never_fetched_and_a_file_url_fails_the_export() {
    let d = dir("exp-external");
    let p = d.join("n.omatree");
    fs::write(d.join("decoy.png"), encode_png(&shot(5, 5)).unwrap()).unwrap();
    let mut doc = Document::open(&p).unwrap();
    let a = doc.create_root("Vnější").unwrap();
    doc.set_body(
        a,
        "![w](https://example.invalid/x.png)\n\n[odkaz](http://127.0.0.1:9/y)\n",
    )
    .unwrap();
    let b = doc.create_root("Soubor").unwrap();
    doc.set_body(b, "![f](file:///etc/passwd)\n![r](decoy.png)\n")
        .unwrap();
    doc.save().unwrap();
    drop(doc);
    let mut h = H::open(&p);
    h.select("Vnější");
    h.export(ExportFormat::Folder, &d.join("ext"));
    assert_eq!(*h.host.dialog(), Dialog::None, "{}", h.host.status());
    let files: Vec<_> = walk(&d.join("ext"))
        .into_iter()
        .map(|f| f.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        files,
        vec!["Vnější.md".to_owned()],
        "the remote image was left as text, nothing fetched"
    );
    h.select("Soubor");
    h.export(ExportFormat::Zip, &d.join("bad.zip"));
    assert!(
        h.host.status().starts_with("Export failed"),
        "{}",
        h.host.status()
    );
    assert!(!d.join("bad.zip").exists());
    assert!(!walk(&d)
        .iter()
        .any(|f| f.to_string_lossy().contains("passwd")));
}

#[test]
fn cancel_and_overwrite_rules() {
    let (d, _p, mut h, _png, _jpg, body) = rich_host("exp-dest");
    let ctx = h.ctx.clone();
    let before = h.state();
    // Native dialog cancelled: nothing is written.
    let log = h.with_picker(vec![None]);
    h.host.start_export(&ctx);
    h.host.set_export_format(ExportFormat::Zip);
    h.host.choose_export_destination(&ctx);
    h.settle();
    assert_eq!(log.borrow().len(), 1);
    assert_eq!(*h.host.dialog(), Dialog::None);
    assert_eq!(
        fs::read_dir(&d)
            .unwrap()
            .filter(|e| e
                .as_ref()
                .unwrap()
                .path()
                .extension()
                .is_some_and(|x| x == "zip"))
            .count(),
        0
    );
    // Escape in the format dialog does nothing either.
    h.host.start_export(&ctx);
    h.settle();
    h.press(Key::Escape);
    assert_eq!(*h.host.dialog(), Dialog::None);
    // A native answer for a folder names its parent; the folder is made inside.
    let parent = d.join("dest-parent");
    fs::create_dir(&parent).unwrap();
    let _log = h.with_picker(vec![Some(parent.clone())]);
    h.host.start_export(&ctx);
    h.host.set_export_format(ExportFormat::Folder);
    h.host.choose_export_destination(&ctx);
    h.settle();
    assert_eq!(*h.host.dialog(), Dialog::None, "{}", h.host.status());
    assert!(parent
        .join(suggest_stem(TITLE))
        .join(format!("{}.md", suggest_stem(TITLE)))
        .exists());
    // Back to typed destinations (the scripted picker has no answers left).
    h.host.set_picker(None);
    // An existing file asks; Cancel keeps it exactly.
    let target = d.join("keep.md");
    fs::write(&target, "mine").unwrap();
    h.export(ExportFormat::Markdown, &target);
    assert!(
        matches!(h.host.dialog(), Dialog::ExportOverwrite { .. }),
        "{:?}",
        h.host.dialog()
    );
    h.host.cancel_dialog();
    assert_eq!(fs::read(&target).unwrap(), b"mine");
    // Yes replaces it, atomically, with the note.
    h.export(ExportFormat::Markdown, &target);
    h.host.confirm_export_overwrite(&ctx);
    h.settle();
    assert_eq!(fs::read(&target).unwrap(), body.as_bytes());
    assert!(leftovers(&d).is_empty());
    // An existing folder is never replaced.
    let dir_target = d.join("existing");
    fs::create_dir(&dir_target).unwrap();
    fs::write(dir_target.join("precious.txt"), "x").unwrap();
    h.export(ExportFormat::Folder, &dir_target);
    assert!(
        matches!(h.host.dialog(), Dialog::ExportPath { error: Some(_), .. }),
        "{:?}",
        h.host.dialog()
    );
    assert!(dir_target.join("precious.txt").exists());
    h.host.cancel_dialog();
    assert_eq!(h.state().3, before.3);
}

#[test]
fn a_failed_export_never_replaces_a_valid_existing_zip_and_the_notebook_file_is_refused() {
    let d = dir("exp-keep");
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    let n = doc.create_root("Vadný").unwrap();
    doc.set_body(n, &format!("![m](assets/{}.png)\n", "cd".repeat(16)))
        .unwrap();
    doc.save().unwrap();
    drop(doc);
    let mut h = H::open(&p);
    h.select("Vadný");
    let ctx = h.ctx.clone();
    let existing = d.join("old.zip");
    fs::write(&existing, b"PK-valid-looking-old-archive").unwrap();
    h.export(ExportFormat::Zip, &existing);
    h.host.confirm_export_overwrite(&ctx);
    h.settle();
    assert!(
        h.host.status().starts_with("Export failed"),
        "{}",
        h.host.status()
    );
    assert_eq!(
        fs::read(&existing).unwrap(),
        b"PK-valid-looking-old-archive"
    );
    assert!(leftovers(&d).is_empty());
    h.host.cancel_dialog();
    // The notebook's own file is never a destination.
    let before = fs::read(&p).unwrap();
    h.export(ExportFormat::Markdown, &p);
    assert!(
        matches!(h.host.dialog(), Dialog::ExportPath { error: Some(e), .. } if e.contains("own file")),
        "{:?}",
        h.host.dialog()
    );
    assert_eq!(fs::read(&p).unwrap(), before);
}

#[cfg(unix)]
#[test]
fn an_unwritable_destination_fails_cleanly_and_a_retry_works() {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    let (d, _p, mut h, _png, _jpg, _body) = rich_host("exp-readonly");
    let ro = d.join("ro");
    fs::create_dir(&ro).unwrap();
    fs::set_permissions(&ro, fs::Permissions::from_mode(0o555)).unwrap();
    let before = h.state();
    for (f, name) in [
        (ExportFormat::Markdown, "a.md"),
        (ExportFormat::Folder, "f"),
        (ExportFormat::Zip, "z.zip"),
        (ExportFormat::Pdf, "p.pdf"),
    ] {
        h.export(f, &ro.join(name));
        assert!(
            h.host.status().starts_with("Export failed"),
            "{f:?}: {}",
            h.host.status()
        );
        h.host.cancel_dialog();
    }
    fs::set_permissions(&ro, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        fs::read_dir(&ro).unwrap().count(),
        0,
        "nothing partial left behind"
    );
    h.export(ExportFormat::Zip, &ro.join("z.zip"));
    assert_eq!(*h.host.dialog(), Dialog::None, "{}", h.host.status());
    assert_eq!(h.state(), before);
}

#[test]
fn export_follows_save_as_and_recovery_restores() {
    let (d, _p, mut h, png, _jpg, body) = rich_host("exp-recovery");
    let ctx = h.ctx.clone();
    let r = refs(&body);
    // After Save As.
    let copy = d.join("copy.omatree");
    h.host.start_save_as_typed(&ctx);
    h.host.set_save_as_path(&copy.display().to_string());
    h.host.submit_save_as(&ctx);
    h.settle();
    h.export(ExportFormat::Zip, &d.join("after-save-as.zip"));
    assert_eq!(*h.host.dialog(), Dialog::None, "{}", h.host.status());
    assert_eq!(
        zip_entries(&d.join("after-save-as.zip"))
            .iter()
            .find(|e| e.0 == r[0])
            .unwrap()
            .1,
        png
    );
    assert_eq!(stored_bytes(&copy, &r[0]), png);
    // After Trash and a Trash restore.
    let id = h.id(TITLE);
    h.host.select(&ctx, id);
    h.host.confirm_delete(&ctx, id);
    h.settle();
    h.host.open_recovery(&ctx);
    h.settle();
    assert!(h.host.restore_trash(&ctx, 0), "{}", h.host.status());
    h.host.cancel_dialog();
    h.select(TITLE);
    h.export(ExportFormat::Folder, &d.join("after-trash"));
    assert_eq!(*h.host.dialog(), Dialog::None, "{}", h.host.status());
    assert_eq!(fs::read(d.join("after-trash").join(&r[0])).unwrap(), png);
    // After a checkpoint restore.
    h.host.confirm_delete(&ctx, h.id(TITLE));
    h.settle();
    h.host.open_recovery(&ctx);
    h.settle();
    assert!(h.host.restore_checkpoint(&ctx, 0), "{}", h.host.status());
    h.host.cancel_dialog();
    h.select(TITLE);
    h.export(ExportFormat::Zip, &d.join("after-checkpoint.zip"));
    assert_eq!(*h.host.dialog(), Dialog::None, "{}", h.host.status());
    assert_eq!(
        zip_entries(&d.join("after-checkpoint.zip"))
            .iter()
            .find(|e| e.0 == r[0])
            .unwrap()
            .1,
        png
    );
}

#[test]
fn the_shortcut_is_owned_by_the_editor_and_reaches_the_host_from_anywhere() {
    use omatree_editor::EditorKeys;
    // The host's profile names the export key; the editor reports it, the host acts.
    assert_eq!(
        EditorKeys::omatree().export,
        None,
        "the library profile leaves it to the host"
    );
    let (_d, _p, mut h, _png, _jpg, body) = rich_host("exp-keys");
    let ctrl_shift = Modifiers::COMMAND | Modifiers::SHIFT;
    // From the editor: the dialog opens and the text is not formatted (Ctrl+E is inline code).
    h.host.focus(Focus::Editor);
    h.settle();
    h.key(Key::E, ctrl_shift);
    assert!(
        matches!(h.host.dialog(), Dialog::Export { .. }),
        "{:?}",
        h.host.dialog()
    );
    assert_eq!(h.host.buffer(), body);
    // With the dialog open a second press does not stack another.
    h.key(Key::E, ctrl_shift);
    assert!(matches!(h.host.dialog(), Dialog::Export { .. }));
    h.press(Key::Escape);
    assert_eq!(*h.host.dialog(), Dialog::None);
    // From the tree.
    h.host.focus(Focus::Tree);
    h.settle();
    h.key(Key::E, ctrl_shift);
    assert!(
        matches!(h.host.dialog(), Dialog::Export { .. }),
        "{:?}",
        h.host.dialog()
    );
    h.press(Key::Escape);
    // Ctrl+E alone is still the editor's inline code, not an export.
    h.host.focus(Focus::Editor);
    h.settle();
    h.key(Key::E, CTRL);
    assert_eq!(*h.host.dialog(), Dialog::None);
}

#[test]
fn f1_and_the_file_menu_list_export() {
    let (_d, _p, mut h, _png, _jpg, _body) = rich_host("exp-help");
    // F1's rows come from the keymap: the Export row prints the shortcut that works.
    let sections = h.host.shortcut_sections(&h.ctx);
    let row = sections
        .iter()
        .flat_map(|s| s.2.iter())
        .find(|r| r.1.contains("Export the note"))
        .expect("an Export row in F1");
    assert_eq!(row.0, h.ctx.format_shortcut(&keymap::EXPORT));
    assert!(row.1.contains("PDF"));
    // The File menu (opened from the keyboard, as the tree's header button) lists it, with the key.
    h.host.focus(Focus::Tree);
    h.settle();
    h.key(Key::Tab, Modifiers::SHIFT);
    h.press(Key::Enter);
    h.advance(0.3);
    let texts: Vec<String> = h.painted().texts.into_iter().map(|t| t.text).collect();
    assert!(
        texts.iter().any(|t| t == "Export note\u{2026}"),
        "{texts:?}"
    );
    assert!(
        texts
            .iter()
            .any(|t| *t == h.ctx.format_shortcut(&keymap::EXPORT)),
        "the key is shown"
    );
}

#[test]
fn the_format_dialog_is_fully_keyboard_operable() {
    let (d, _p, mut h, _png, _jpg, _body) = rich_host("exp-kbd");
    h.host.set_picker(None);
    h.host.focus(Focus::Editor);
    h.settle();
    h.key(Key::E, Modifiers::COMMAND | Modifiers::SHIFT);
    // The note has images, so the sensible default is the ZIP (a single portable file).
    assert_eq!(
        *h.host.dialog(),
        Dialog::Export {
            format: ExportFormat::Zip,
            images: 2
        }
    );
    h.press(Key::ArrowDown);
    assert!(
        matches!(
            h.host.dialog(),
            Dialog::Export {
                format: ExportFormat::Pdf,
                ..
            }
        ),
        "{:?}",
        h.host.dialog()
    );
    h.press(Key::ArrowUp);
    h.press(Key::ArrowUp);
    assert!(matches!(
        h.host.dialog(),
        Dialog::Export {
            format: ExportFormat::Folder,
            ..
        }
    ));
    // Enter goes on to the destination (typed here: there is no native dialog); Enter again exports.
    h.press(Key::Enter);
    assert!(
        matches!(
            h.host.dialog(),
            Dialog::ExportPath {
                format: ExportFormat::Folder,
                ..
            }
        ),
        "{:?}",
        h.host.dialog()
    );
    h.host
        .set_export_path(&d.join("kbd-folder").display().to_string());
    h.press(Key::Enter);
    h.settle();
    assert_eq!(*h.host.dialog(), Dialog::None, "{}", h.host.status());
    assert!(d.join("kbd-folder").is_dir());
    // A note without images defaults to the plain Markdown file.
    h.select("Druhá");
    h.host.focus(Focus::Editor);
    h.settle();
    h.key(Key::E, Modifiers::COMMAND | Modifiers::SHIFT);
    assert_eq!(
        *h.host.dialog(),
        Dialog::Export {
            format: ExportFormat::Markdown,
            images: 0
        }
    );
    h.press(Key::Escape);
    assert_eq!(*h.host.dialog(), Dialog::None);
}

// --- A really full filesystem ---------------------------------------------------------------------
//
// As in E9c: the parent re-runs `export_enospc_child` inside `unshare -Urm` over a small tmpfs that
// is filled until a write fails. Where `unshare` is not permitted the test says so and returns.

#[test]
fn export_enospc_child() {
    use std::io::Write as _;
    let Ok(out) = std::env::var("E9C_EXPORT_ENOSPC_DIR") else {
        return;
    };
    let out = PathBuf::from(out);
    let src = dir("exp-enospc-src");
    let p = src.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    let n = doc.create_root("Velký").unwrap();
    let mut body = String::new();
    for i in 0..4u32 {
        let r = doc
            .put_attachment(&encode_png(&shot(400 + i as usize, 300)).unwrap())
            .unwrap();
        body.push_str(&format!("![i{i}]({r})\n\n"));
    }
    doc.set_body(n, &body).unwrap();
    doc.save().unwrap();
    drop(doc);
    let mut h = H::open(&p);
    h.select("Velký");
    let mut failed = 0;
    for leave in [0u64, 8192, 100_000, 600_000] {
        for (f, name) in [
            (ExportFormat::Markdown, "a.md"),
            (ExportFormat::Folder, "f"),
            (ExportFormat::Zip, "z.zip"),
            (ExportFormat::Pdf, "p.pdf"),
        ] {
            for e in fs::read_dir(&out).unwrap() {
                let p = e.unwrap().path();
                let _ = if p.is_dir() {
                    fs::remove_dir_all(&p)
                } else {
                    fs::remove_file(&p)
                };
            }
            let filler = out.join("filler-file");
            let mut fh = fs::File::create(&filler).unwrap();
            let chunk = vec![0u8; 4096];
            while fh.write_all(&chunk).is_ok() {}
            let len = fh.metadata().unwrap().len();
            fh.set_len(len.saturating_sub(leave)).unwrap();
            drop(fh);
            let before = h.state();
            h.export(f, &out.join(name));
            let outcome_failed = h.host.status().starts_with("Export failed");
            fs::remove_file(&filler).unwrap();
            if outcome_failed {
                failed += 1;
                assert!(
                    !out.join(name).exists(),
                    "{f:?}/{leave}: a partial export was left"
                );
                assert!(
                    leftovers(&out).is_empty(),
                    "{f:?}/{leave}: {:?}",
                    leftovers(&out)
                );
                h.host.cancel_dialog();
                // Space freed: the same export now works.
                h.export(f, &out.join(name));
                assert_eq!(
                    *h.host.dialog(),
                    Dialog::None,
                    "{f:?}/{leave}: {}",
                    h.host.status()
                );
            }
            assert!(out.join(name).exists(), "{f:?}/{leave}");
            assert_eq!(h.state(), before, "{f:?}/{leave}: the notebook changed");
        }
    }
    println!("EXPORT-ENOSPC failed-cleanly={failed}");
}

#[test]
fn a_really_full_filesystem_fails_every_export_cleanly() {
    let d = dir("exp-enospc");
    let mount = d.join("small");
    fs::create_dir(&mount).unwrap();
    if !Command::new("unshare")
        .args(["-Urm", "true"])
        .status()
        .is_ok_and(|s| s.success())
    {
        eprintln!("SKIPPED: unprivileged mount namespaces are unavailable; real ENOSPC is UNVERIFIED here");
        return;
    }
    let out = Command::new("unshare")
        .args(["-Urm", "sh", "-c"])
        .arg(r#"mount -t tmpfs -o size=4m tmpfs "$E9C_EXPORT_ENOSPC_DIR" && exec "$0" --exact tests::export_tests::export_enospc_child --nocapture --test-threads=1"#)
        .arg(std::env::current_exe().unwrap())
        .env("E9C_EXPORT_ENOSPC_DIR", &mount)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let line = text
        .lines()
        .find(|l| l.contains("EXPORT-ENOSPC"))
        .unwrap_or("");
    eprintln!("{line}");
    assert!(
        !line.contains("failed-cleanly=0"),
        "no export actually ran out of space: {text}"
    );
}

/// The disposable notebook for the manual native-window check of E11a
/// (`OMATREE_GUI_DIR=... cargo test -p omatree-egui -- --ignored write_export_gui_fixture`).
#[test]
#[ignore = "writes a fixture for the manual GUI check"]
fn write_export_gui_fixture() {
    let dir = PathBuf::from(std::env::var("OMATREE_GUI_DIR").expect("set OMATREE_GUI_DIR"));
    fs::create_dir_all(&dir).unwrap();
    rich(&dir.join("export.omatree"));
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
        .find_map(|l| l.strip_prefix("VmHWM:"))
        .and_then(|v| v.trim().trim_end_matches("kB").trim().parse().ok())
        .unwrap_or(0)
}

/// Export durations and peak memory (`cargo test --release -p omatree-egui -- --ignored
/// --nocapture export_performance`). Indicative: one machine, one run, headless.
#[test]
#[ignore = "measurement, run with --release"]
fn export_performance() {
    use std::time::Instant;
    let d = dir("exp-perf");
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    // 40 other attachments that no exported note uses.
    for i in 0..40 {
        doc.put_attachment(&encode_png(&shot(300 + i, 200)).unwrap())
            .unwrap();
    }
    let text_only = doc.create_root("Text").unwrap();
    doc.set_body(
        text_only,
        &"Řádek žluťoučký kůň úpěl ďábelské ódy.\n\n".repeat(20),
    )
    .unwrap();
    let shots = doc.create_root("Screenshots").unwrap();
    let mut body = String::new();
    for i in 0..6 {
        let r = doc
            .put_attachment(&encode_png(&shot(1280 + i, 720)).unwrap())
            .unwrap();
        body.push_str(&format!("Obrázek {i}\n\n![s{i}]({r})\n\n"));
    }
    doc.set_body(shots, &body).unwrap();
    let long = doc.create_root("Long").unwrap();
    let mut body = String::new();
    for i in 0..3000 {
        body.push_str(&format!("## Oddíl {i}\n\nOdstavec číslo {i} s **tučným**, `kódem` a [odkazem](https://example.com/{i}).\n\n- jedna\n- dvě\n\n"));
    }
    doc.set_body(long, &body).unwrap();
    let big = doc.create_root("Big JPEG").unwrap();
    let jpg = jpeg_of(3000, 2000);
    let jpg_len = jpg.len();
    let r = doc.put_attachment(&jpg).unwrap();
    doc.set_body(big, &format!("![j]({r})\n")).unwrap();
    doc.save().unwrap();
    drop(doc);
    let mut h = H::open(&p);
    println!(
        "fixture: file {} KiB, large JPEG {} KiB",
        fs::metadata(&p).unwrap().len() / 1024,
        jpg_len / 1024
    );
    for title in ["Text", "Screenshots", "Long", "Big JPEG"] {
        h.select(title);
        for (f, name) in [
            (ExportFormat::Markdown, "o.md"),
            (ExportFormat::Folder, "o-dir"),
            (ExportFormat::Zip, "o.zip"),
            (ExportFormat::Pdf, "o.pdf"),
        ] {
            let dest = d.join(format!("{title}-{name}"));
            let t = Instant::now();
            h.export(f, &dest);
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            assert_eq!(
                *h.host.dialog(),
                Dialog::None,
                "{title}/{f:?}: {}",
                h.host.status()
            );
            let size = if dest.is_dir() {
                walk(&dest)
                    .iter()
                    .map(|f| fs::metadata(f).unwrap().len())
                    .sum()
            } else {
                fs::metadata(&dest).unwrap().len()
            };
            println!(
                "{title:12} {f:?}: {ms:7.1} ms, {:6} KiB output, peak resident so far {} MiB",
                size / 1024,
                rss_kib() / 1024
            );
        }
    }
}

#[test]
fn the_dialogs_stay_usable_in_a_narrow_window() {
    let d = dir("exp-narrow");
    let p = d.join("n.omatree");
    rich(&p);
    let mut h = H::open_sized(&p, Vec2::new(360.0, 560.0));
    h.select(TITLE);
    let ctx = h.ctx.clone();
    h.host.start_export(&ctx);
    h.settle();
    let painted = h.painted();
    for label in [
        "Choose destination\u{2026}",
        "Type a path\u{2026}",
        "Cancel",
    ] {
        let t = painted
            .texts
            .iter()
            .find(|t| t.text == label)
            .unwrap_or_else(|| panic!("no {label} button painted"));
        assert!(
            t.pos.x >= 0.0 && t.pos.x < 360.0 && t.pos.y >= 0.0 && t.pos.y < 560.0,
            "{label} is off screen at {:?}",
            t.pos
        );
    }
    h.host.set_export_format(ExportFormat::Markdown);
    h.host.open_typed_export(&ctx, ExportFormat::Markdown);
    h.settle();
    let painted = h.painted();
    assert!(painted
        .texts
        .iter()
        .any(|t| t.text == "Export" && t.pos.x < 360.0));
}
