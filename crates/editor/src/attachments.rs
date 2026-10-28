//! Where image attachments live. The editor only ever talks to [`AttachmentStore`]; it never
//! touches the filesystem for images itself, so another backend (for instance a database table)
//! can be plugged in without changing editing, layout or rendering.
//!
//! A *reference* is the string that appears in Markdown: `![alt](REFERENCE)`. For the filesystem
//! backend it is a portable, `/`-separated path relative to the document (`assets/image-1.png`).
//!
//! What any backend must satisfy:
//! - `put` returns a reference that stays valid across restarts and never depends on an absolute
//!   location; it never overwrites an existing attachment; the bytes are stored unchanged.
//! - `get`/`info` accept any string found in a document and fail with an error (never a panic, never
//!   data outside the store) for references that are invalid, unsupported or missing.
//! - All operations are synchronous and cheap enough to call on the UI thread for screenshots.

use std::fmt;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Directory (relative to the document) that new attachments are written to.
pub const ASSET_DIR: &str = "assets";
/// Larger attachments are refused when read: they could never be shown as a texture anyway.
pub const MAX_ATTACHMENT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AttachmentError {
    /// Not something this store may open: absolute, remote, `..`, backslashes, NUL, escaping links.
    Invalid(String),
    /// A well-formed reference to nothing.
    NotFound(String),
    TooLarge(u64),
    Io(String),
}

impl fmt::Display for AttachmentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(r) => write!(f, "unsupported attachment reference: {r}"),
            Self::NotFound(r) => write!(f, "attachment not found: {r}"),
            Self::TooLarge(n) => write!(f, "attachment too large ({n} bytes)"),
            Self::Io(e) => write!(f, "attachment I/O error: {e}"),
        }
    }
}

/// What a renderer or exporter needs to know without reading the bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachmentInfo {
    pub len: u64,
    pub media_type: &'static str,
}

pub trait AttachmentStore {
    /// Store `bytes` as a new attachment; `ext` is a file-type hint (`"png"`). Returns its reference.
    fn put(&self, bytes: &[u8], ext: &str) -> Result<String, AttachmentError>;
    /// The attachment's original bytes.
    fn get(&self, reference: &str) -> Result<Vec<u8>, AttachmentError>;
    /// Size and media type, or the same error `get` would give.
    fn info(&self, reference: &str) -> Result<AttachmentInfo, AttachmentError>;
}

pub fn media_type_for(name: &str) -> &'static str {
    match name
        .rsplit('.')
        .next()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

/// Check a reference and return its normalised form (`./a//b.png` → `a/b.png`). Only relative,
/// `/`-separated references without `..` are accepted; remote URLs, absolute paths, drive letters,
/// backslashes and control characters are not.
pub fn normalize(reference: &str) -> Result<String, AttachmentError> {
    let bad = || AttachmentError::Invalid(reference.to_owned());
    let r = reference.trim();
    let first = r.split('/').next().unwrap_or("");
    if r.is_empty()
        || r.contains("://")
        || r.starts_with('/')
        || r.contains('\\')
        || r.contains(|c: char| c.is_control())
        || first.contains(':')
    {
        return Err(bad());
    }
    let mut parts = Vec::new();
    for p in r.split('/') {
        match p {
            "" | "." => {}
            ".." => return Err(bad()),
            p => parts.push(p),
        }
    }
    if parts.is_empty() {
        return Err(bad());
    }
    Ok(parts.join("/"))
}

/// Attachments stored under the directory of the Markdown document (`<doc dir>/assets/…`).
pub struct FsStore {
    root: PathBuf,
}

impl FsStore {
    pub fn new(doc_dir: impl Into<PathBuf>) -> Self {
        Self {
            root: doc_dir.into(),
        }
    }

    /// The file a reference names, if it is valid and (when it exists) really inside the document
    /// directory after following links.
    fn resolve(&self, reference: &str) -> Result<PathBuf, AttachmentError> {
        let rel = normalize(reference)?;
        let path = self.root.join(&rel);
        if let (Ok(real), Ok(root)) = (path.canonicalize(), self.root.canonicalize()) {
            if !real.starts_with(root) {
                return Err(AttachmentError::Invalid(reference.to_owned()));
            }
        }
        Ok(path)
    }
}

fn io_error(reference: &str, e: &std::io::Error) -> AttachmentError {
    if e.kind() == std::io::ErrorKind::NotFound {
        AttachmentError::NotFound(reference.to_owned())
    } else {
        AttachmentError::Io(e.to_string())
    }
}

impl AttachmentStore for FsStore {
    fn put(&self, bytes: &[u8], ext: &str) -> Result<String, AttachmentError> {
        let ext = if !ext.is_empty()
            && ext.len() <= 8
            && ext.bytes().all(|b| b.is_ascii_alphanumeric())
        {
            ext.to_ascii_lowercase()
        } else {
            "bin".to_owned()
        };
        let dir = self.root.join(ASSET_DIR);
        std::fs::create_dir_all(&dir)
            .map_err(|e| AttachmentError::Io(format!("cannot create {}: {e}", dir.display())))?;
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        for n in 0..10_000 {
            let name = if n == 0 {
                format!("image-{secs}.{ext}")
            } else {
                format!("image-{secs}-{n}.{ext}")
            };
            // `create_new` makes the check-and-create atomic: existing files are never overwritten.
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(dir.join(&name))
            {
                Ok(mut f) => {
                    if let Err(e) = f.write_all(bytes) {
                        drop(f);
                        let _ = std::fs::remove_file(dir.join(&name));
                        return Err(AttachmentError::Io(format!("write failed: {e}")));
                    }
                    return Ok(format!("{ASSET_DIR}/{name}"));
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => {
                    return Err(AttachmentError::Io(format!(
                        "cannot create attachment: {e}"
                    )))
                }
            }
        }
        Err(AttachmentError::Io("no free attachment name".into()))
    }

    fn get(&self, reference: &str) -> Result<Vec<u8>, AttachmentError> {
        let path = self.resolve(reference)?;
        let f = std::fs::File::open(&path).map_err(|e| io_error(reference, &e))?;
        let meta = f.metadata().map_err(|e| io_error(reference, &e))?;
        if !meta.is_file() {
            return Err(AttachmentError::NotFound(reference.to_owned()));
        }
        if meta.len() > MAX_ATTACHMENT_BYTES {
            return Err(AttachmentError::TooLarge(meta.len()));
        }
        let mut bytes = Vec::with_capacity(meta.len() as usize);
        f.take(MAX_ATTACHMENT_BYTES)
            .read_to_end(&mut bytes)
            .map_err(|e| io_error(reference, &e))?;
        Ok(bytes)
    }

    fn info(&self, reference: &str) -> Result<AttachmentInfo, AttachmentError> {
        let path = self.resolve(reference)?;
        let meta = std::fs::metadata(&path).map_err(|e| io_error(reference, &e))?;
        if !meta.is_file() {
            return Err(AttachmentError::NotFound(reference.to_owned()));
        }
        Ok(AttachmentInfo {
            len: meta.len(),
            media_type: media_type_for(&path.to_string_lossy()),
        })
    }
}

/// One referenced attachment, as an exporter would package it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Packaged {
    /// Normalised portable relative path (the Markdown keeps its own spelling).
    pub reference: String,
    pub media_type: &'static str,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Referenced {
    /// Each distinct attachment once, in order of first use.
    pub attachments: Vec<Packaged>,
    /// References that could not be packaged (missing, invalid, remote), each once.
    pub problems: Vec<(String, AttachmentError)>,
}

/// The attachments `markdown` actually refers to, with their original bytes. Read-only: neither the
/// Markdown nor the store is changed, and files in the store that nothing refers to are ignored.
pub fn referenced(store: &dyn AttachmentStore, markdown: &str) -> Referenced {
    let mut out = Referenced::default();
    let mut seen = std::collections::HashSet::new();
    for url in crate::markdown::image_references(markdown) {
        let key = normalize(&url).unwrap_or_else(|_| url.clone());
        if !seen.insert(key.clone()) {
            continue;
        }
        match normalize(&url).and_then(|r| {
            let info = store.info(&r)?;
            Ok((r.clone(), info.media_type, store.get(&r)?))
        }) {
            Ok((reference, media_type, bytes)) => out.attachments.push(Packaged {
                reference,
                media_type,
                bytes,
            }),
            Err(e) => out.problems.push((url, e)),
        }
    }
    out
}

/// Directory this store reads from; used by tests and diagnostics only.
impl FsStore {
    #[cfg(test)]
    pub fn root(&self) -> &std::path::Path {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> crate::test_dir::TestDir {
        crate::test_dir::TestDir::new(&format!("att-{tag}"))
    }

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0, 1, 2, 3, 255, 254];

    #[test]
    fn store_and_retrieve_preserves_bytes_and_survives_reinitialisation() {
        let dir = tmp("rt");
        let s = FsStore::new(&dir);
        let r = s.put(PNG, "png").unwrap();
        assert!(r.starts_with("assets/image-") && r.ends_with(".png") && !r.contains('\\'));
        assert!(
            !r.starts_with('/') && !r.contains(dir.to_str().unwrap()),
            "no absolute path"
        );
        assert_eq!(s.get(&r).unwrap(), PNG);
        let info = s.info(&r).unwrap();
        assert_eq!((info.len, info.media_type), (PNG.len() as u64, "image/png"));
        // A new store over the same directory (a restart) resolves the same reference.
        let again = FsStore::new(&dir);
        assert_eq!(again.get(&r).unwrap(), PNG);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn existing_assets_and_other_relative_paths_still_load() {
        let dir = tmp("old");
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::create_dir_all(dir.join("img/sub")).unwrap();
        std::fs::write(dir.join("assets/obrázek žlutý.png"), PNG).unwrap();
        std::fs::write(dir.join("img/sub/a.PNG"), PNG).unwrap();
        std::fs::write(dir.join("top.jpg"), PNG).unwrap();
        let s = FsStore::new(&dir);
        for r in [
            "assets/obrázek žlutý.png",
            "./assets//obrázek žlutý.png",
            "img/sub/a.PNG",
            "top.jpg",
        ] {
            assert_eq!(s.get(r).unwrap(), PNG, "{r}");
        }
        assert_eq!(s.info("img/sub/a.PNG").unwrap().media_type, "image/png");
        assert_eq!(s.info("top.jpg").unwrap().media_type, "image/jpeg");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn collisions_never_overwrite() {
        let dir = tmp("col");
        let s = FsStore::new(&dir);
        let refs: Vec<String> = (0..30)
            .map(|i| s.put(format!("data{i}").as_bytes(), "png").unwrap())
            .collect();
        let unique: std::collections::HashSet<_> = refs.iter().collect();
        assert_eq!(unique.len(), 30);
        for (i, r) in refs.iter().enumerate() {
            assert_eq!(s.get(r).unwrap(), format!("data{i}").as_bytes());
        }
        // Even a pre-existing file of the name it would pick is left alone.
        let secs_name = refs[0].clone();
        std::fs::write(dir.join(&secs_name), b"mine").unwrap();
        let r = s.put(b"new", "png").unwrap();
        assert_ne!(r, secs_name);
        assert_eq!(std::fs::read(dir.join(&secs_name)).unwrap(), b"mine");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn unsafe_and_unsupported_references_are_rejected() {
        let dir = tmp("sec");
        std::fs::write(dir.join("secret.txt"), b"x").unwrap();
        let outer = dir.parent().unwrap().join("omatree-att-outside.png");
        std::fs::write(&outer, PNG).unwrap();
        let sub = dir.join("doc");
        std::fs::create_dir_all(&sub).unwrap();
        let s = FsStore::new(&sub);
        for bad in [
            "../secret.txt",
            "assets/../../secret.txt",
            "assets/../../omatree-att-outside.png",
            "/etc/passwd",
            "//host/share/a.png",
            "C:/Windows/x.png",
            "c:\\x.png",
            "assets\\..\\a.png",
            "https://example.com/a.png",
            "file:///etc/passwd",
            "data:image/png;base64,AAAA",
            "",
            "   ",
            ".",
            "./",
            "assets/a\0.png",
            "assets/a\n.png",
        ] {
            assert!(
                matches!(s.get(bad), Err(AttachmentError::Invalid(_))),
                "get {bad:?}: {:?}",
                s.get(bad)
            );
            assert!(
                matches!(s.info(bad), Err(AttachmentError::Invalid(_))),
                "info {bad:?}"
            );
        }
        // A link inside the document directory that points outside it is refused too.
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outer, sub.join("link.png")).unwrap();
            assert!(matches!(
                s.get("link.png"),
                Err(AttachmentError::Invalid(_))
            ));
            std::os::unix::fs::symlink(&dir, sub.join("up")).unwrap();
            assert!(matches!(
                s.get("up/secret.txt"),
                Err(AttachmentError::Invalid(_))
            ));
        }
        std::fs::remove_file(outer).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_directories_and_oversized_files_fail_safely() {
        let dir = tmp("miss");
        let s = FsStore::new(&dir);
        assert!(matches!(
            s.get("assets/none.png"),
            Err(AttachmentError::NotFound(_))
        ));
        assert!(matches!(
            s.info("assets/none.png"),
            Err(AttachmentError::NotFound(_))
        ));
        std::fs::create_dir_all(dir.join("assets/folder.png")).unwrap();
        assert!(matches!(
            s.get("assets/folder.png"),
            Err(AttachmentError::NotFound(_))
        ));
        assert!(matches!(
            FsStore::new(dir.join("no-such-dir")).get("a.png"),
            Err(AttachmentError::NotFound(_))
        ));
        let big = dir.join("big.png");
        let f = std::fs::File::create(&big).unwrap();
        f.set_len(MAX_ATTACHMENT_BYTES + 1).unwrap();
        assert!(matches!(
            s.get("big.png"),
            Err(AttachmentError::TooLarge(_))
        ));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn put_fails_cleanly_when_the_directory_is_unusable() {
        let dir = tmp("fail");
        // `assets` is a regular file: nothing can be created inside it.
        std::fs::write(dir.join("assets"), b"file").unwrap();
        let s = FsStore::new(&dir);
        assert!(matches!(s.put(PNG, "png"), Err(AttachmentError::Io(_))));
        assert_eq!(std::fs::read(dir.join("assets")).unwrap(), b"file");
        // Odd extension hints cannot smuggle a path.
        let _guard1 = tmp("ext");
        let ok = FsStore::new(&_guard1);
        let r = ok.put(PNG, "../../x/y").unwrap();
        assert!(r.starts_with("assets/image-") && r.ends_with(".bin"), "{r}");
        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::remove_dir_all(ok.root()).unwrap();
    }

    #[test]
    fn normalisation() {
        assert_eq!(normalize("./assets//a b.png").unwrap(), "assets/a b.png");
        assert_eq!(normalize(" assets/ž.png ").unwrap(), "assets/ž.png");
        assert!(normalize("a/../b").is_err());
    }

    #[test]
    fn enumeration_returns_only_referenced_attachments_once() {
        let dir = tmp("enum");
        let s = FsStore::new(&dir);
        let a = s.put(b"AAAA", "png").unwrap();
        let b = s.put(b"BBBBBB", "png").unwrap();
        let _unrelated = s.put(b"zzz", "png").unwrap();
        std::fs::write(dir.join("assets/obrázek-žlutý.png"), b"UTF8").unwrap();
        let md = format!(
            "# T\n\n![Příliš 🦀]({a}){{width=60%}}\n\ntext ![inline]({b}) more\n\n![again]( ./{a} )\n\n![u](assets/obrázek-žlutý.png)\n\n![gone](assets/missing.png)\n\n![bad](../x.png)\n\n![remote](https://example.com/a.png)\n\n`![code](assets/code.png)`\n\n```\n![fenced](assets/fenced.png)\n```\n",
        );
        let before = md.clone();
        let r = referenced(&s, &md);
        assert_eq!(md, before, "Markdown untouched");
        let refs: Vec<_> = r.attachments.iter().map(|p| p.reference.as_str()).collect();
        assert_eq!(
            refs,
            [a.as_str(), b.as_str(), "assets/obrázek-žlutý.png"],
            "once each, in order"
        );
        assert_eq!(r.attachments[0].bytes, b"AAAA");
        assert_eq!(r.attachments[0].media_type, "image/png");
        assert_eq!(r.attachments[2].bytes, b"UTF8");
        let problems: Vec<_> = r.problems.iter().map(|(u, _)| u.as_str()).collect();
        assert_eq!(
            problems,
            [
                "assets/missing.png",
                "../x.png",
                "https://example.com/a.png"
            ]
        );
        assert!(matches!(r.problems[0].1, AttachmentError::NotFound(_)));
        assert!(matches!(r.problems[1].1, AttachmentError::Invalid(_)));
        assert!(matches!(r.problems[2].1, AttachmentError::Invalid(_)));
        // Nothing else from the directory was included.
        assert!(r.attachments.iter().all(|p| p.bytes != b"zzz"));
        assert_eq!(referenced(&s, "no images here"), Referenced::default());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
