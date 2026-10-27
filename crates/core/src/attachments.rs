//! Notebook-owned image attachments: identifiers, references, validation. Pure Rust, no UI.
//!
//! An attachment is an image (PNG or JPEG) stored as a BLOB in the notebook's own SQLite file. It
//! is named by a random 128-bit identifier (32 lowercase hex digits) that is independent of node
//! ids, and referenced from Markdown as `assets/<id>.png` or `assets/<id>.jpg`. The extension only
//! mirrors the media type found in the bytes. Nothing here reads or writes files, and nothing
//! touches the network.
//!
//! What is validated: the bytes are *structurally* a PNG (signature, an `IHDR` chunk with non-zero
//! size, chunks that fit, ending in `IEND`) or a JPEG (`FFD8`, a start-of-frame marker with
//! non-zero size, ending in `FFD9`). It is not a decoder: a file that passes may still fail to
//! decode in the editor, which then shows its usual "cannot show this image". Bytes are stored
//! exactly as given, never converted or recompressed.

use std::fmt;

/// The largest attachment accepted (the editor library's own limit).
pub const MAX_ATTACHMENT_BYTES: u64 = 64 * 1024 * 1024;
/// The most attachment bytes a notebook holds in memory waiting for a save (all pending images
/// together). Two full-size images fit; a screenshot is typically well under 10 MiB, and a saved
/// notebook commits its pending images within seconds (autosave), so the budget is only felt in
/// an untitled notebook or after save failures. Saving needs about 2.3 times these bytes at its
/// peak (measured in E9c), so the budget keeps that under about 300 MiB.
pub const MAX_PENDING_BYTES: u64 = 128 * 1024 * 1024;
/// The folder-like prefix of every reference.
pub const ASSET_PREFIX: &str = "assets/";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaType {
    Png,
    Jpeg,
}

impl MediaType {
    pub fn mime(self) -> &'static str {
        match self {
            MediaType::Png => "image/png",
            MediaType::Jpeg => "image/jpeg",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            MediaType::Png => "png",
            MediaType::Jpeg => "jpg",
        }
    }

    pub fn from_mime(mime: &str) -> Option<Self> {
        match mime {
            "image/png" => Some(MediaType::Png),
            "image/jpeg" => Some(MediaType::Jpeg),
            _ => None,
        }
    }

    fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "png" => Some(MediaType::Png),
            "jpg" => Some(MediaType::Jpeg),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AttachmentError {
    /// Not a reference or identifier this notebook can have (wrong shape, path tricks, remote).
    InvalidReference(String),
    /// A well-formed reference to nothing.
    NotFound(String),
    /// Not a PNG or JPEG.
    UnsupportedFormat,
    /// Looks like a PNG or JPEG but is damaged or truncated.
    Malformed(&'static str),
    TooLarge(u64),
    /// Adding this image would take the unsaved images past [`MAX_PENDING_BYTES`]; nothing was
    /// added. Carries the bytes already pending.
    PendingFull(u64),
    Empty,
    /// What is stored does not match what was recorded (length, media type).
    Corrupt(String),
    /// Reading or writing the notebook failed.
    Storage(String),
}

impl fmt::Display for AttachmentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidReference(r) => write!(f, "unsupported attachment reference: {r}"),
            Self::NotFound(r) => write!(f, "attachment not found: {r}"),
            Self::UnsupportedFormat => write!(f, "only PNG and JPEG images are supported"),
            Self::Malformed(why) => write!(f, "the image is damaged: {why}"),
            Self::TooLarge(n) => write!(f, "attachment too large ({n} bytes)"),
            Self::PendingFull(held) => write!(
                f,
                "too many unsaved images ({} MiB held, limit {} MiB): save the notebook first",
                held / (1024 * 1024),
                MAX_PENDING_BYTES / (1024 * 1024)
            ),
            Self::Empty => write!(f, "the attachment is empty"),
            Self::Corrupt(why) => write!(f, "stored attachment is inconsistent: {why}"),
            Self::Storage(e) => write!(f, "attachment storage error: {e}"),
        }
    }
}

impl std::error::Error for AttachmentError {}

/// A copy of `bytes` whose allocation can fail: a refusal the caller can report (`Storage`), where
/// `to_vec` would abort the whole process when memory runs out. Attachments are the one place the
/// program holds tens of megabytes in a single buffer.
pub(crate) fn try_copy(bytes: &[u8]) -> Result<Vec<u8>, AttachmentError> {
    let mut copy = Vec::new();
    copy.try_reserve_exact(bytes.len()).map_err(|_| {
        AttachmentError::Storage(format!(
            "not enough memory for an image of {} bytes",
            bytes.len()
        ))
    })?;
    copy.extend_from_slice(bytes);
    Ok(copy)
}

/// A 128-bit identifier as 32 lowercase hex digits.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AttachmentId(String);

impl AttachmentId {
    /// Accepts exactly 32 lowercase hexadecimal digits.
    pub fn parse(text: &str) -> Result<Self, AttachmentError> {
        if text.len() == 32
            && text
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            Ok(AttachmentId(text.to_owned()))
        } else {
            Err(AttachmentError::InvalidReference(text.to_owned()))
        }
    }

    /// A fresh identifier from the operating system's secure random source.
    pub fn random() -> Result<Self, AttachmentError> {
        Self::from_bytes(random_bytes()?)
    }

    pub fn from_bytes(bytes: [u8; 16]) -> Result<Self, AttachmentError> {
        let mut s = String::with_capacity(32);
        for b in bytes {
            s.push(char::from_digit(u32::from(b >> 4), 16).unwrap_or('0'));
            s.push(char::from_digit(u32::from(b & 15), 16).unwrap_or('0'));
        }
        Ok(AttachmentId(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AttachmentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

pub(crate) fn random_bytes() -> Result<[u8; 16], AttachmentError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|e| AttachmentError::Storage(format!("no secure random source: {e}")))?;
    Ok(bytes)
}

/// The reference that names attachment `id` of `media`: `assets/<id>.<ext>`.
pub fn reference(id: &AttachmentId, media: MediaType) -> String {
    format!("{ASSET_PREFIX}{id}.{}", media.extension())
}

/// Splits `assets/<32 hex>.png|jpg` into its parts. Anything else (other folders, `..`, absolute
/// paths, other extensions, upper-case, extra segments, URLs) is refused.
pub fn parse_reference(text: &str) -> Result<(AttachmentId, MediaType), AttachmentError> {
    let bad = || AttachmentError::InvalidReference(text.to_owned());
    let rest = text.strip_prefix(ASSET_PREFIX).ok_or_else(bad)?;
    let (id, ext) = rest.split_once('.').ok_or_else(bad)?;
    let media = MediaType::from_extension(ext).ok_or_else(bad)?;
    let id = AttachmentId::parse(id).map_err(|_| bad())?;
    Ok((id, media))
}

/// Finds every canonical attachment reference in `text` and hands the 32-digit id of each to
/// `found` (a slice of `text`; duplicates are reported each time).
///
/// This is the D9 plain-text rule, not Markdown: it does not know about code blocks, comments,
/// links or escaping, so a reference anywhere in a body counts. The grammar is exactly
///
/// ```text
/// reference = "assets/" id ( ".png" / ".jpg" )      id = 32 * ( DIGIT / %x61-66 )
/// ```
///
/// with nothing required before or after it (`xassets/<id>.png` and `assets/<id>.pngx` both
/// match: conservative). Not recognised, so never protecting anything: upper-case digits or
/// extension, other extensions (`.jpeg`, `.PNG`), ids shorter or longer than 32 digits (the first
/// 32 digits of a longer run are not taken for an id: the next byte must be `.`), percent-escapes,
/// backslashes, and anything that is not `assets/` literally. The id is only ever compared with
/// stored ids; nothing here builds a path or opens anything.
pub fn scan_references<'a>(text: &'a str, mut found: impl FnMut(&'a str)) {
    let bytes = text.as_bytes();
    let mut from = 0;
    while let Some(at) = text[from..].find(ASSET_PREFIX) {
        let start = from + at + ASSET_PREFIX.len();
        // Restart one byte after the `a` (always a character boundary), so overlapping
        // candidates such as `assets/assets/<id>.png` are all examined.
        from += at + 1;
        let Some(id) = bytes.get(start..start + 32) else {
            continue;
        };
        if !id
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
        {
            continue;
        }
        let tail = &bytes[start + 32..];
        if tail.starts_with(b".png") || tail.starts_with(b".jpg") {
            // The 32 bytes are ASCII, so this is a valid slice.
            found(&text[start..start + 32]);
        }
    }
}

/// Check `bytes` and say what they are. Size first, then the format.
pub fn sniff(bytes: &[u8]) -> Result<MediaType, AttachmentError> {
    if bytes.is_empty() {
        return Err(AttachmentError::Empty);
    }
    if bytes.len() as u64 > MAX_ATTACHMENT_BYTES {
        return Err(AttachmentError::TooLarge(bytes.len() as u64));
    }
    if bytes.starts_with(&PNG_SIGNATURE) {
        check_png(bytes)?;
        Ok(MediaType::Png)
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        check_jpeg(bytes)?;
        Ok(MediaType::Jpeg)
    } else {
        Err(AttachmentError::UnsupportedFormat)
    }
}

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

fn check_png(b: &[u8]) -> Result<(), AttachmentError> {
    let mut at = PNG_SIGNATURE.len();
    let mut first = true;
    loop {
        let header = b
            .get(at..at + 8)
            .ok_or(AttachmentError::Malformed("truncated PNG"))?;
        let len = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
        let kind = &header[4..8];
        let data = at + 8;
        let end = data
            .checked_add(len)
            .and_then(|e| e.checked_add(4))
            .filter(|e| *e <= b.len())
            .ok_or(AttachmentError::Malformed("PNG chunk runs past the end"))?;
        if first {
            if kind != b"IHDR" || len != 13 {
                return Err(AttachmentError::Malformed("PNG has no header chunk"));
            }
            let w = u32::from_be_bytes([b[data], b[data + 1], b[data + 2], b[data + 3]]);
            let h = u32::from_be_bytes([b[data + 4], b[data + 5], b[data + 6], b[data + 7]]);
            if w == 0 || h == 0 {
                return Err(AttachmentError::Malformed("PNG has no size"));
            }
            first = false;
        }
        if kind == b"IEND" {
            return if end == b.len() {
                Ok(())
            } else {
                Err(AttachmentError::Malformed("data after the end of the PNG"))
            };
        }
        at = end;
    }
}

fn check_jpeg(b: &[u8]) -> Result<(), AttachmentError> {
    if b.len() < 4 || b[b.len() - 2..] != [0xFF, 0xD9] {
        return Err(AttachmentError::Malformed("JPEG does not end properly"));
    }
    // Walk the marker segments up to the frame header.
    let mut at = 2;
    while at + 4 <= b.len() {
        if b[at] != 0xFF {
            return Err(AttachmentError::Malformed("JPEG segment expected"));
        }
        let marker = b[at + 1];
        if marker == 0xFF {
            at += 1;
            continue;
        }
        let len = usize::from(u16::from_be_bytes([b[at + 2], b[at + 3]]));
        let is_frame = matches!(marker, 0xC0..=0xCF) && !matches!(marker, 0xC4 | 0xC8 | 0xCC);
        if is_frame {
            let f = b
                .get(at + 4..at + 4 + 5)
                .ok_or(AttachmentError::Malformed("JPEG frame header truncated"))?;
            let height = u16::from_be_bytes([f[1], f[2]]);
            let width = u16::from_be_bytes([f[3], f[4]]);
            return if width == 0 || height == 0 {
                Err(AttachmentError::Malformed("JPEG has no size"))
            } else {
                Ok(())
            };
        }
        if len < 2 {
            return Err(AttachmentError::Malformed("JPEG segment too short"));
        }
        at += 2 + len;
    }
    Err(AttachmentError::Malformed("JPEG has no frame header"))
}

/// What a renderer or exporter needs without reading the bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachmentInfo {
    pub len: u64,
    pub media_type: MediaType,
}

/// One attachment held in memory until a save commits it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attachment {
    pub id: AttachmentId,
    pub media_type: MediaType,
    pub created_at: i64,
    pub data: Vec<u8>,
}

impl Attachment {
    pub fn reference(&self) -> String {
        reference(&self.id, self.media_type)
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    /// A valid 1x1 PNG, built (with real CRCs and an Adler-32) rather than pasted.
    pub fn png_1x1() -> Vec<u8> {
        png(1, 1)
    }

    pub fn png(width: u32, height: u32) -> Vec<u8> {
        fn crc(bytes: &[u8]) -> u32 {
            let mut c = 0xFFFF_FFFFu32;
            for b in bytes {
                c ^= u32::from(*b);
                for _ in 0..8 {
                    c = if c & 1 == 1 {
                        0xEDB8_8320 ^ (c >> 1)
                    } else {
                        c >> 1
                    };
                }
            }
            !c
        }
        fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            let mut body = kind.to_vec();
            body.extend_from_slice(data);
            out.extend_from_slice(&body);
            out.extend_from_slice(&crc(&body).to_be_bytes());
        }
        let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&width.to_be_bytes());
        ihdr.extend_from_slice(&height.to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
        chunk(&mut out, b"IHDR", &ihdr);
        let raw = vec![0u8; (height * (1 + 4 * width)) as usize];
        // zlib with stored blocks.
        let mut z = vec![0x78, 0x01];
        let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
        for (n, block) in blocks.iter().enumerate() {
            z.push(u8::from(n + 1 == blocks.len()));
            z.extend_from_slice(&(block.len() as u16).to_le_bytes());
            z.extend_from_slice(&(!(block.len() as u16)).to_le_bytes());
            z.extend_from_slice(block);
        }
        let (mut a, mut b) = (1u32, 0u32);
        for byte in &raw {
            a = (a + u32::from(*byte)) % 65_521;
            b = (b + a) % 65_521;
        }
        z.extend_from_slice(&((b << 16) | a).to_be_bytes());
        chunk(&mut out, b"IDAT", &z);
        chunk(&mut out, b"IEND", &[]);
        out
    }

    /// A structurally valid PNG of exactly `total` bytes: a real 2x2 image padded with an ancillary
    /// `tEXt` chunk before `IEND`, built in one allocation of `total` bytes (so a measurement of the
    /// code under test is not drowned by the fixture). `total` must exceed a 2x2 PNG plus 12.
    pub fn png_of_len(total: usize) -> Vec<u8> {
        fn crc(bytes: &[u8]) -> u32 {
            // Table-driven: the padding can be tens of megabytes.
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
        let base = png(2, 2);
        let pad = total - base.len() - 12;
        let iend = base.len() - 12;
        let mut out = base[..iend].to_vec();
        let mut body = b"tEXt".to_vec();
        body.extend(std::iter::repeat_n(b'x', pad));
        out.extend_from_slice(&(pad as u32).to_be_bytes());
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc(&body).to_be_bytes());
        out.extend_from_slice(&base[iend..]);
        assert_eq!(out.len(), total);
        out
    }

    /// A minimal structurally valid JPEG (SOI, SOF0 2x3, EOI).
    pub fn jpeg() -> Vec<u8> {
        let mut v = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00];
        v.extend_from_slice(&[
            0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x03, 0x00, 0x02, 0x01, 0x01,
        ]);
        v.extend_from_slice(&[0x11, 0x00, 0xFF, 0xD9]);
        v
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    #[test]
    fn ids_are_32_lowercase_hex_and_references_have_exactly_one_shape() {
        let id = AttachmentId::parse("0123456789abcdef0123456789abcdef").unwrap();
        assert_eq!(
            reference(&id, MediaType::Png),
            "assets/0123456789abcdef0123456789abcdef.png"
        );
        for bad in [
            "",
            "0123456789ABCDEF0123456789ABCDEF",
            "0123456789abcdef0123456789abcde",
            "0123456789abcdef0123456789abcdef0",
            "0123456789abcdef0123456789abcdeg",
            "../0123456789abcdef0123456789abcdef",
        ] {
            assert!(AttachmentId::parse(bad).is_err(), "{bad:?}");
        }
        let ok = "assets/0123456789abcdef0123456789abcdef";
        assert_eq!(
            parse_reference(&format!("{ok}.png")).unwrap().1,
            MediaType::Png
        );
        assert_eq!(
            parse_reference(&format!("{ok}.jpg")).unwrap().1,
            MediaType::Jpeg
        );
        for bad in [
            format!("{ok}.jpeg"),
            format!("{ok}.gif"),
            format!("{ok}.PNG"),
            ok.to_string(),
            format!("{ok}.png/extra"),
            format!("./{ok}.png"),
            format!("/{ok}.png"),
            format!("assets/../{ok}.png"),
            "other/0123456789abcdef0123456789abcdef.png".to_string(),
            "assets//0123456789abcdef0123456789abcdef.png".to_string(),
            "assets\\0123456789abcdef0123456789abcdef.png".to_string(),
            format!("https://example.org/{ok}.png"),
            format!("{ok}.png\0"),
            "assets/short.png".to_owned(),
            "C:/assets/0123456789abcdef0123456789abcdef.png".to_owned(),
        ] {
            assert!(parse_reference(&bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn media_type_comes_from_the_bytes_not_from_any_name() {
        assert_eq!(sniff(&png_1x1()).unwrap(), MediaType::Png);
        assert_eq!(sniff(&jpeg()).unwrap(), MediaType::Jpeg);
        assert_eq!(
            sniff(b"GIF89a....").unwrap_err(),
            AttachmentError::UnsupportedFormat
        );
        assert_eq!(
            sniff(b"<svg/>").unwrap_err(),
            AttachmentError::UnsupportedFormat
        );
        assert_eq!(sniff(b"").unwrap_err(), AttachmentError::Empty);
    }

    #[test]
    fn damaged_images_are_refused() {
        for cut in [9, 20, 30, png_1x1().len() - 1] {
            assert!(
                matches!(sniff(&png_1x1()[..cut]), Err(AttachmentError::Malformed(_))),
                "png {cut}"
            );
        }
        let mut trailing = png_1x1().to_vec();
        trailing.push(0);
        assert!(sniff(&trailing).is_err());
        let mut no_size = png_1x1().to_vec();
        no_size[19] = 0; // width 0
        assert!(sniff(&no_size).is_err());
        let mut not_ihdr = png_1x1().to_vec();
        not_ihdr[12] = b'X';
        assert!(sniff(&not_ihdr).is_err());
        let j = jpeg();
        assert!(sniff(&j[..j.len() - 2]).is_err(), "no end marker");
        assert!(sniff(&[0xFF, 0xD8, 0xFF]).is_err());
        let mut zero = j.clone();
        zero[13] = 0;
        zero[14] = 0; // height 0
        assert!(sniff(&zero).is_err());
    }

    #[test]
    fn the_size_limit_is_enforced_before_anything_else() {
        let mut big = vec![0u8; MAX_ATTACHMENT_BYTES as usize + 1];
        big[..8].copy_from_slice(&PNG_SIGNATURE);
        assert_eq!(
            sniff(&big).unwrap_err(),
            AttachmentError::TooLarge(MAX_ATTACHMENT_BYTES + 1)
        );
    }

    #[test]
    fn random_ids_are_valid_and_differ() {
        let a = AttachmentId::random().unwrap();
        let b = AttachmentId::random().unwrap();
        assert_ne!(a, b);
        assert!(AttachmentId::parse(a.as_str()).is_ok());
    }

    fn scan(text: &str) -> Vec<String> {
        let mut v = Vec::new();
        scan_references(text, |id| v.push(id.to_owned()));
        v
    }

    const ID: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn the_scanner_recognises_exactly_the_canonical_grammar() {
        assert_eq!(scan(&format!("![a](assets/{ID}.png)")), [ID]);
        assert_eq!(scan(&format!("![a](assets/{ID}.jpg)")), [ID]);
        // Conservative on both sides.
        assert_eq!(scan(&format!("xassets/{ID}.pngx")), [ID]);
        assert_eq!(scan(&format!("Příliš 🦀assets/{ID}.jpg🦀 žluťoučký")), [ID]);
        assert_eq!(scan(&format!("assets/assets/{ID}.png")), [ID]);
        for bad in [
            format!("assets/{}.png", ID.to_uppercase()),
            format!("assets/{ID}.PNG"),
            format!("assets/{ID}.jpeg"),
            format!("assets/{ID}.gif"),
            format!("assets/{ID}"),
            format!("assets/{}.png", &ID[..31]),
            format!("assets/{ID}0.png"),
            format!("assets/0{ID}.png"),
            format!("assets/{}g.png", &ID[..31]),
            format!("assets%2F{ID}.png"),
            format!("assets\\{ID}.png"),
            format!("Assets/{ID}.png"),
            "assets/".to_owned(),
            "assets/é".to_owned(),
            String::new(),
        ] {
            assert!(scan(&bad).is_empty(), "{bad:?} must not be a reference");
        }
    }
}
