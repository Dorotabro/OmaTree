//! Portable export of a Markdown document and the attachments it references, as a folder
//! (`note.md` + `assets/…`) or as a single ZIP with the same layout.
//!
//! Nothing here knows about egui, the live editor state or a concrete attachment backend: a
//! [`Plan`] is built from a Markdown string and an [`AttachmentStore`], and the same plan is written
//! by [`write_dir`] or [`write_zip`].
//!
//! Rules:
//! - Every local image reference is resolved through the store (`attachments::referenced`) and its
//!   original bytes are copied unchanged, once, to its normalised relative path inside the export
//!   (`./assets//a.png` → `assets/a.png`). `http`, `https` and `data:` images stay external and are
//!   never fetched. Any other reference (missing, absolute, `..`, `file:`, other schemes) fails the
//!   whole export, naming the reference.
//! - The Markdown is written byte for byte as given. Because every reference is reproduced at the
//!   path it already names (relative to the note), no destination ever has to be rewritten; alt text,
//!   titles and `{width=NN%}` annotations are therefore untouched by construction.
//! - Output is staged in a hidden sibling (`.NAME.partial-…`) and renamed into place only when
//!   complete; an existing destination is never replaced.

use crate::attachments::{self, AttachmentError, AttachmentStore, Packaged};
use std::fmt;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum ExportError {
    /// Local references that could not be packaged, with the reason for each.
    Attachments(Vec<(String, AttachmentError)>),
    /// A planned path is not a safe relative path (cannot happen for normalised references; checked
    /// again when writing).
    UnsafePath(String),
    /// Two attachments (or an attachment and the note) would need the same name.
    PathConflict(String),
    BadName(String),
    /// The PDF could not be produced (unsupported image, rendering failure).
    Pdf(String),
    /// The destination already exists.
    Conflict(PathBuf),
    Io(String),
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Attachments(p) => {
                write!(f, "cannot export: ")?;
                for (i, (r, e)) in p.iter().take(3).enumerate() {
                    let why = match e {
                        AttachmentError::NotFound(_) => "not found".to_owned(),
                        AttachmentError::Invalid(_) => "not a supported local reference".to_owned(),
                        AttachmentError::TooLarge(n) => format!("too large ({n} bytes)"),
                        AttachmentError::Io(e) => e.clone(),
                    };
                    write!(f, "{}image `{r}` is {why}", if i > 0 { "; " } else { "" })?;
                }
                if p.len() > 3 {
                    write!(f, "; and {} more", p.len() - 3)?;
                }
                Ok(())
            }
            Self::UnsafePath(p) => write!(f, "unsafe path in export: {p}"),
            Self::PathConflict(p) => write!(f, "two files would be written to `{p}`"),
            Self::BadName(n) => write!(f, "invalid document name `{n}`"),
            Self::Pdf(e) => write!(f, "PDF export failed: {e}"),
            Self::Conflict(p) => write!(f, "{} already exists; choose another name", p.display()),
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}

/// Everything an export contains, ready to be written.
#[derive(Debug, PartialEq, Eq)]
pub struct Plan {
    pub markdown_name: String,
    /// The note exactly as given.
    pub markdown: String,
    /// Referenced local attachments, once each, at their portable relative paths.
    pub attachments: Vec<Packaged>,
    /// External images (`http`, `https`, `data:`) left as they are; never fetched.
    pub external: Vec<String>,
}

fn is_external(url: &str) -> bool {
    let url = url.trim();
    let Some((scheme, _)) = url.split_once(':') else {
        return false;
    };
    scheme.len() >= 2
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'.' | b'-'))
        && ["http", "https", "data"]
            .iter()
            .any(|s| scheme.eq_ignore_ascii_case(s))
}

/// A file name for the note: last path component, `.md` appended unless it already has `.md` or
/// `.markdown`.
pub fn note_name(name: &str) -> Result<String, ExportError> {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("").trim();
    if base.is_empty()
        || base == "."
        || base == ".."
        || base.contains(|c: char| c.is_control() || c == ':')
        || base.len() > 200
    {
        return Err(ExportError::BadName(name.to_owned()));
    }
    let lower = base.to_lowercase();
    Ok(if lower.ends_with(".md") || lower.ends_with(".markdown") {
        base.to_owned()
    } else {
        format!("{base}.md")
    })
}

/// A path that may be written below an export root: relative, `/`-separated, no empty, `.` or `..`
/// components, nothing a ZIP tool or OS would treat specially.
fn safe_entry(path: &str) -> Result<(), ExportError> {
    let bad = || ExportError::UnsafePath(path.to_owned());
    if path.is_empty() || path.starts_with('/') || path.contains('\\') || path.contains('\0') {
        return Err(bad());
    }
    for c in path.split('/') {
        if c.is_empty()
            || c == "."
            || c == ".."
            || c.len() > 255
            || c.contains(|ch: char| ch.is_control() || ch == ':')
        {
            return Err(bad());
        }
    }
    Ok(())
}

/// Collect what to export. Reads the store only; never changes the Markdown or the store.
pub fn plan(markdown: &str, store: &dyn AttachmentStore, name: &str) -> Result<Plan, ExportError> {
    let markdown_name = note_name(name)?;
    let found = attachments::referenced(store, markdown);
    let mut external = Vec::new();
    let mut failed = Vec::new();
    for (url, err) in found.problems {
        if is_external(&url) {
            external.push(url);
        } else {
            failed.push((url, err));
        }
    }
    if !failed.is_empty() {
        return Err(ExportError::Attachments(failed));
    }
    let mut taken = vec![markdown_name.to_lowercase()];
    for a in &found.attachments {
        safe_entry(&a.reference)?;
        let lower = a.reference.to_lowercase();
        // Same name apart from case, or a file that would have to be a folder: refuse.
        let clash = taken.iter().any(|t| {
            *t == lower
                || lower.starts_with(&format!("{t}/"))
                || t.starts_with(&format!("{lower}/"))
        });
        if clash {
            return Err(ExportError::PathConflict(a.reference.clone()));
        }
        taken.push(lower);
    }
    Ok(Plan {
        markdown_name,
        markdown: markdown.to_owned(),
        attachments: found.attachments,
        external,
    })
}

fn entries(plan: &Plan) -> Result<Vec<(&str, &[u8])>, ExportError> {
    let mut v: Vec<(&str, &[u8])> = vec![(&plan.markdown_name, plan.markdown.as_bytes())];
    v.extend(
        plan.attachments
            .iter()
            .map(|a| (a.reference.as_str(), a.bytes.as_slice())),
    );
    for (p, _) in &v {
        safe_entry(p)?;
    }
    Ok(v)
}

/// Fail if `dest` exists (even as a broken link) or its folder is missing.
fn check_destination(dest: &Path) -> Result<(), ExportError> {
    if std::fs::symlink_metadata(dest).is_ok() {
        return Err(ExportError::Conflict(dest.to_owned()));
    }
    let parent = match dest.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    if !parent.is_dir() {
        return Err(ExportError::Io(format!(
            "destination folder {} does not exist",
            parent.display()
        )));
    }
    if dest.file_name().is_none() {
        return Err(ExportError::Io(format!(
            "{} is not a valid destination",
            dest.display()
        )));
    }
    Ok(())
}

fn staging_path(dest: &Path) -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let name = dest
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    dest.with_file_name(format!(
        ".{name}.partial-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ))
}

fn io(what: &str, e: impl fmt::Display) -> ExportError {
    ExportError::Io(format!("{what}: {e}"))
}

/// Write `plan` as the new folder `dest` (`note.md` and the attachments below it).
pub fn write_dir(plan: &Plan, dest: &Path) -> Result<(), ExportError> {
    let files = entries(plan)?;
    check_destination(dest)?;
    let tmp = staging_path(dest);
    std::fs::create_dir(&tmp).map_err(|e| io(&format!("cannot create {}", tmp.display()), e))?;
    let result = (|| {
        for (path, bytes) in &files {
            let full = tmp.join(path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| io(&format!("cannot create {}", parent.display()), e))?;
            }
            let mut f = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&full)
                .map_err(|e| io(&format!("cannot write {path}"), e))?;
            f.write_all(bytes)
                .map_err(|e| io(&format!("cannot write {path}"), e))?;
        }
        check_destination(dest)?;
        std::fs::rename(&tmp, dest).map_err(|e| io(&format!("cannot create {}", dest.display()), e))
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&tmp);
    }
    result
}

/// Publish `bytes` as the new file `dest`: written to a hidden sibling first and renamed into place
/// when complete; an existing destination is never replaced and nothing partial is left behind.
pub(crate) fn write_file(bytes: &[u8], dest: &Path) -> Result<(), ExportError> {
    check_destination(dest)?;
    let tmp = staging_path(dest);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|e| io(&format!("cannot create {}", tmp.display()), e))?;
    let result = (|| {
        file.write_all(bytes)
            .map_err(|e| io("cannot write the file", e))?;
        file.sync_all()
            .map_err(|e| io("cannot write the file", e))?;
        check_destination(dest)?;
        std::fs::rename(&tmp, dest).map_err(|e| io(&format!("cannot create {}", dest.display()), e))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Write `plan` as the new ZIP file `dest`, with the same layout as [`write_dir`].
pub fn write_zip(plan: &Plan, dest: &Path) -> Result<(), ExportError> {
    let files = entries(plan)?;
    check_destination(dest)?;
    let tmp = staging_path(dest);
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|e| io(&format!("cannot create {}", tmp.display()), e))?;
    let result = (|| {
        let mut zip = zip::ZipWriter::new(file);
        // Images are already compressed and the note is small: store everything unchanged.
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .unix_permissions(0o644);
        for (path, bytes) in &files {
            zip.start_file(*path, opts)
                .map_err(|e| io(&format!("cannot add {path}"), e))?;
            zip.write_all(bytes)
                .map_err(|e| io(&format!("cannot add {path}"), e))?;
        }
        let f = zip.finish().map_err(|e| io("cannot finish the ZIP", e))?;
        f.sync_all().map_err(|e| io("cannot finish the ZIP", e))?;
        check_destination(dest)?;
        std::fs::rename(&tmp, dest).map_err(|e| io(&format!("cannot create {}", dest.display()), e))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attachments::FsStore;
    use std::cell::RefCell;
    use std::collections::{BTreeMap, HashMap};

    fn tmp(tag: &str) -> crate::test_dir::TestDir {
        crate::test_dir::TestDir::new(&format!("exp-{tag}"))
    }

    /// Any bytes at any reference, for odd paths a filesystem store could not hold at once.
    #[derive(Default)]
    struct Mem(RefCell<HashMap<String, Vec<u8>>>);

    impl AttachmentStore for Mem {
        fn put(&self, _: &[u8], _: &str) -> Result<String, AttachmentError> {
            unreachable!()
        }
        fn get(&self, r: &str) -> Result<Vec<u8>, AttachmentError> {
            let k = attachments::normalize(r)?;
            self.0
                .borrow()
                .get(&k)
                .cloned()
                .ok_or(AttachmentError::NotFound(k))
        }
        fn info(&self, r: &str) -> Result<attachments::AttachmentInfo, AttachmentError> {
            self.get(r).map(|b| attachments::AttachmentInfo {
                len: b.len() as u64,
                media_type: "image/png",
            })
        }
    }

    /// Every file below `root` as relative path → bytes.
    fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
        fn walk(dir: &Path, root: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
            for e in std::fs::read_dir(dir).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    walk(&p, root, out);
                } else {
                    let rel = p
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/");
                    out.insert(rel, std::fs::read(&p).unwrap());
                }
            }
        }
        let mut m = BTreeMap::new();
        walk(root, root, &mut m);
        m
    }

    fn unzip(path: &Path) -> BTreeMap<String, Vec<u8>> {
        use std::io::Read;
        let mut z = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
        let mut m = BTreeMap::new();
        for i in 0..z.len() {
            let mut f = z.by_index(i).unwrap();
            let name = f.name().unwrap().into_owned();
            assert!(!name.starts_with('/') && !name.contains(".."), "{name}");
            let mut b = Vec::new();
            f.read_to_end(&mut b).unwrap();
            assert!(m.insert(name, b).is_none(), "duplicate entry");
        }
        m
    }

    const PNG_A: &[u8] = &[0x89, b'P', b'N', b'G', 13, 10, 26, 10, 0, 0, 7, 255, 0, 128];
    const PNG_B: &[u8] = &[0x89, b'P', b'N', b'G', 1, 2, 3];

    fn doc_with_images(dir: &Path) -> (FsStore, String, String, String) {
        let s = FsStore::new(dir);
        let a = s.put(PNG_A, "png").unwrap();
        let b = s.put(PNG_B, "png").unwrap();
        let md = format!("# Příliš 🦀\n\n![Snímek A]({a}){{width=60%}}\n\ntext ![B \"t\"]({b} \"Titulek\") a znovu ![A2]({a})\n\n![remote](https://example.com/x.png)\n\nžluťoučký\n");
        (s, md, a, b)
    }

    #[test]
    fn note_without_attachments() {
        let _guard1 = tmp("none-src");
        let s = FsStore::new(&_guard1);
        let md = "# Nadpis\n\nPříliš žluťoučký kůň 🦀\n\n[odkaz](https://x.org)\n";
        let p = plan(md, &s, "note.md").unwrap();
        assert!(p.attachments.is_empty() && p.external.is_empty());
        let _keep1 = tmp("none");
        let dest = _keep1.join("out");
        write_dir(&p, &dest).unwrap();
        assert_eq!(
            tree(&dest),
            BTreeMap::from([("note.md".to_owned(), md.as_bytes().to_vec())])
        );
        let z = dest.with_extension("zip");
        write_zip(&p, &z).unwrap();
        assert_eq!(unzip(&z), tree(&dest));
    }

    #[test]
    fn images_are_copied_once_with_exact_bytes_and_the_markdown_is_untouched() {
        let src = tmp("img-src");
        let (s, md, a, b) = doc_with_images(&src);
        // An unrelated file in the assets folder and in the document folder.
        std::fs::write(src.join("assets/unused.png"), b"unused").unwrap();
        std::fs::write(src.join("secret.txt"), b"secret").unwrap();
        let before = md.clone();
        let p = plan(&md, &s, "Poznámka.md").unwrap();
        assert_eq!(md, before);
        assert_eq!(p.markdown, md, "no reference needed rewriting");
        assert_eq!(p.markdown_name, "Poznámka.md");
        assert_eq!(p.external, ["https://example.com/x.png"]);
        let paths: Vec<_> = p.attachments.iter().map(|x| x.reference.as_str()).collect();
        assert_eq!(
            paths,
            [a.as_str(), b.as_str()],
            "once each, repeated reference deduplicated"
        );
        let _keep2 = tmp("img");
        let dest = _keep2.join("export");
        write_dir(&p, &dest).unwrap();
        let t = tree(&dest);
        assert_eq!(t.len(), 3, "{:?}", t.keys());
        assert_eq!(t["Poznámka.md"], md.as_bytes());
        assert_eq!(t[&a], PNG_A);
        assert_eq!(t[&b], PNG_B);
        assert!(
            md.contains("{width=60%}")
                && String::from_utf8_lossy(&t["Poznámka.md"]).contains("{width=60%}")
        );
        // The source folder is as it was.
        assert_eq!(std::fs::read(src.join(&a)).unwrap(), PNG_A);
        assert_eq!(
            std::fs::read(src.join("assets/unused.png")).unwrap(),
            b"unused"
        );
    }

    #[test]
    fn zip_matches_the_directory_export() {
        let src = tmp("zip-src");
        let (s, md, _, _) = doc_with_images(&src);
        let p = plan(&md, &s, "note").unwrap();
        assert_eq!(p.markdown_name, "note.md");
        let base = tmp("zip");
        write_dir(&p, &base.join("dir")).unwrap();
        write_zip(&p, &base.join("out.zip")).unwrap();
        assert_eq!(unzip(&base.join("out.zip")), tree(&base.join("dir")));
        // A ZIP is a single file and nothing else was left next to it.
        let names: Vec<_> = std::fs::read_dir(&base)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().all(|n| !n.contains("partial")), "{names:?}");
    }

    #[test]
    fn exported_folder_reopens_with_its_images_resolved() {
        let src = tmp("re-src");
        let (s, md, a, b) = doc_with_images(&src);
        let p = plan(&md, &s, "note.md").unwrap();
        let _keep3 = tmp("re");
        let dest = _keep3.join("moved");
        write_dir(&p, &dest).unwrap();
        // Move the folder elsewhere, then resolve the references of its Markdown from there.
        let _keep4 = tmp("re2");
        let elsewhere = _keep4.join("again");
        std::fs::rename(&dest, &elsewhere).unwrap();
        let text = String::from_utf8(std::fs::read(elsewhere.join("note.md")).unwrap()).unwrap();
        let store = FsStore::new(&elsewhere);
        let r = attachments::referenced(&store, &text);
        let got: Vec<_> = r
            .attachments
            .iter()
            .map(|x| (x.reference.clone(), x.bytes.clone()))
            .collect();
        assert_eq!(got, [(a, PNG_A.to_vec()), (b, PNG_B.to_vec())]);
        assert!(r.problems.iter().all(|(u, _)| u.starts_with("https://")));
    }

    #[test]
    fn nested_unicode_and_odd_but_valid_relative_paths() {
        let m = Mem::default();
        for (k, v) in [
            ("assets/obrázek žlutý.png", PNG_A),
            ("img/sub dir/ž ů/a.PNG", PNG_B),
            ("top.png", PNG_A),
        ] {
            m.0.borrow_mut().insert(k.to_owned(), v.to_vec());
        }
        let md = "![a](<assets/obrázek žlutý.png>)\n\n![b](./img//sub dir/ž ů/a.PNG)\n![b2](<img/sub dir/ž ů/a.PNG>)\n\n![c](top.png)\n";
        let p = plan(md, &m, "n.md").unwrap();
        let paths: Vec<_> = p.attachments.iter().map(|x| x.reference.as_str()).collect();
        assert_eq!(
            paths,
            [
                "assets/obrázek žlutý.png",
                "img/sub dir/ž ů/a.PNG",
                "top.png"
            ]
        );
        let base = tmp("nested");
        write_dir(&p, &base.join("o")).unwrap();
        write_zip(&p, &base.join("o.zip")).unwrap();
        let t = tree(&base.join("o"));
        assert_eq!(t["img/sub dir/ž ů/a.PNG"], PNG_B);
        assert_eq!(unzip(&base.join("o.zip")), t);
        // Nothing was written outside the export folder.
        let names: Vec<_> = std::fs::read_dir(&base)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 2, "{names:?}");
    }

    #[test]
    fn missing_and_invalid_local_references_fail_the_export_with_diagnostics() {
        let _guard2 = tmp("bad-src");
        let s = FsStore::new(&_guard2);
        let s_with = s.put(PNG_A, "png").unwrap();
        for (md, needle) in [
            (
                "![x](assets/missing.png)".to_owned(),
                "assets/missing.png` is not found",
            ),
            (
                "![x](../outside.png)".to_owned(),
                "../outside.png` is not a supported",
            ),
            ("![x](/etc/passwd)".to_owned(), "/etc/passwd"),
            ("![x](file:///etc/passwd)".to_owned(), "file:///etc/passwd"),
            ("![x](C:/x.png)".to_owned(), "C:/x.png"),
            (
                "![x](javascript:alert(1))".to_owned(),
                "javascript:alert(1)",
            ),
            (
                format!("![ok]({s_with})\n![x](assets/gone.png)"),
                "gone.png",
            ),
        ] {
            let e = plan(&md, &s, "n.md").unwrap_err();
            let msg = e.to_string();
            assert!(matches!(e, ExportError::Attachments(_)), "{md}");
            assert!(msg.contains(needle), "{md}: {msg}");
        }
        // Several problems are all counted.
        let e = plan(
            "![a](a.png) ![b](b.png) ![c](c.png) ![d](d.png)",
            &s,
            "n.md",
        )
        .unwrap_err();
        assert!(e.to_string().contains("and 1 more"), "{e}");
        // Remote and data images are allowed and never fetched.
        let p = plan(
            "![r](http://x/a.png) ![d](data:image/png;base64,AAAA) ![h](HTTPS://x/b.png)",
            &s,
            "n.md",
        )
        .unwrap();
        assert_eq!(p.external.len(), 3);
        assert!(p.attachments.is_empty());
    }

    #[test]
    fn names_and_paths_are_checked() {
        assert_eq!(note_name("a/b/note").unwrap(), "note.md");
        assert_eq!(note_name("..\\x\\Note.MD").unwrap(), "Note.MD");
        assert_eq!(note_name("x.markdown").unwrap(), "x.markdown");
        for bad in ["", "  ", "..", ".", "a/", "bad\nname", "c:"] {
            assert!(note_name(bad).is_err(), "{bad:?}");
        }
        for bad in [
            "",
            "/a.png",
            "../a.png",
            "a/../b.png",
            "a//b.png",
            "a\\b.png",
            "a/./b",
            "x:y.png",
            "a\0b",
        ] {
            assert!(safe_entry(bad).is_err(), "{bad:?}");
        }
        assert!(safe_entry("assets/ž ů.png").is_ok());
        // A hand-made plan with an unsafe entry is refused before anything is created.
        let _guard3 = tmp("p-src");
        let mut p = plan("x", &FsStore::new(&_guard3), "n.md").unwrap();
        p.attachments.push(Packaged {
            reference: "../escape.png".into(),
            media_type: "image/png",
            bytes: PNG_A.to_vec(),
        });
        let base = tmp("unsafe");
        assert!(matches!(
            write_dir(&p, &base.join("o")),
            Err(ExportError::UnsafePath(_))
        ));
        assert!(matches!(
            write_zip(&p, &base.join("o.zip")),
            Err(ExportError::UnsafePath(_))
        ));
        assert_eq!(std::fs::read_dir(&base).unwrap().count(), 0);
    }

    #[test]
    fn clashing_names_are_refused() {
        let m = Mem::default();
        for k in ["a.png", "A.PNG", "dir", "dir/b.png", "note.md"] {
            m.0.borrow_mut().insert(k.to_owned(), PNG_A.to_vec());
        }
        for md in [
            "![](a.png) ![](A.PNG)",
            "![](dir) ![](dir/b.png)",
            "![](note.md)",
        ] {
            assert!(
                matches!(plan(md, &m, "note.md"), Err(ExportError::PathConflict(_))),
                "{md}"
            );
        }
    }

    #[test]
    fn existing_destinations_are_never_replaced() {
        let _guard4 = tmp("ex-src");
        let s = FsStore::new(&_guard4);
        let p = plan("text", &s, "n.md").unwrap();
        let base = tmp("ex");
        std::fs::create_dir(base.join("out")).unwrap();
        std::fs::write(base.join("out/mine.txt"), b"mine").unwrap();
        std::fs::write(base.join("f.zip"), b"mine").unwrap();
        let e = write_dir(&p, &base.join("out")).unwrap_err();
        assert!(
            matches!(e, ExportError::Conflict(_)) && e.to_string().contains("already exists"),
            "{e}"
        );
        assert!(matches!(
            write_zip(&p, &base.join("f.zip")),
            Err(ExportError::Conflict(_))
        ));
        assert!(matches!(
            write_dir(&p, &base.join("f.zip")),
            Err(ExportError::Conflict(_))
        ));
        assert_eq!(std::fs::read(base.join("out/mine.txt")).unwrap(), b"mine");
        assert_eq!(std::fs::read(base.join("f.zip")).unwrap(), b"mine");
        // A missing parent folder is reported, and nothing is created.
        let e = write_dir(&p, &base.join("nope/out")).unwrap_err();
        assert!(e.to_string().contains("does not exist"), "{e}");
        assert!(!base.join("nope").exists());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(base.join("nowhere"), base.join("dangling")).unwrap();
            assert!(matches!(
                write_dir(&p, &base.join("dangling")),
                Err(ExportError::Conflict(_))
            ));
        }
    }

    #[test]
    fn a_failure_while_writing_leaves_no_partial_export() {
        let _guard5 = tmp("pf-src");
        let mut p = plan("text", &FsStore::new(&_guard5), "n.md").unwrap();
        let png = |r: &str| Packaged {
            reference: r.into(),
            media_type: "image/png",
            bytes: PNG_A.to_vec(),
        };
        // The same entry twice: the second write fails after the first succeeded.
        p.attachments = vec![png("assets/a.png"), png("assets/a.png")];
        let base = tmp("pf");
        assert!(write_dir(&p, &base.join("out")).is_err());
        assert!(write_zip(&p, &base.join("out.zip")).is_err());
        let left: Vec<_> = std::fs::read_dir(&base)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert!(left.is_empty(), "{left:?}");
        // A file where a folder is needed fails halfway through too.
        p.attachments = vec![png("assets"), png("assets/b.png")];
        assert!(write_dir(&p, &base.join("out2")).is_err());
        assert_eq!(std::fs::read_dir(&base).unwrap().count(), 0);
        // And an unwritable destination folder fails before creating anything.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let ro = base.join("ro");
            std::fs::create_dir(&ro).unwrap();
            std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o555)).unwrap();
            let _guard6 = tmp("pf2-src");
            let p = plan("text", &FsStore::new(&_guard6), "n.md").unwrap();
            let can_write = std::fs::write(ro.join("probe"), b"").is_ok();
            if !can_write {
                assert!(matches!(
                    write_dir(&p, &ro.join("out")),
                    Err(ExportError::Io(_))
                ));
                assert!(matches!(
                    write_zip(&p, &ro.join("o.zip")),
                    Err(ExportError::Io(_))
                ));
                assert_eq!(std::fs::read_dir(&ro).unwrap().count(), 0);
            }
            std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
}
