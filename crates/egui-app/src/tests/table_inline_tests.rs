//! T3 in the application: a note whose table uses inline Markdown keeps its exact bytes through
//! rendering, the caret entering and leaving the table, autosave, Save As and reopening.
//! Disposable notebooks throughout.

use super::*;

const LF: &str = "Intro\n\n| Style | Example |\n| :-- | --: |\n| bold | **Fixed** |\n| code | `a \\| b` ~~x~~ |\n| czech | **Příliš** 🦀 [s](https://example.com) |\n\nEnd\n";

fn bytes_on_disk(p: &Path, title: &str) -> String {
    let doc = Document::open_existing(p).unwrap();
    doc.notebook()
        .depth_first()
        .into_iter()
        .find(|n| n.title() == title)
        .unwrap()
        .body()
        .to_owned()
}

fn fixture(name: &str, body: &str) -> (TestDir, PathBuf) {
    let d = dir(name);
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    let a = doc.create_root("Tables").unwrap();
    doc.create_root("Other").unwrap();
    doc.set_body(a, body).unwrap();
    doc.save().unwrap();
    (d, p)
}

#[test]
fn table_bytes_survive_rendering_autosave_save_as_and_reopening_lf_and_crlf() {
    for (name, body) in [
        ("t3-lf", LF.to_owned()),
        ("t3-crlf", LF.replace('\n', "\r\n")),
    ] {
        let (_d, p) = fixture(name, &body);
        let mut h = H::open(&p);
        h.select("Tables");
        assert_eq!(h.host.buffer(), body, "{name}: loaded as stored");
        // The caret through the table and out again, and a frame or two each time.
        let inside = body.find("Fixed").unwrap();
        for at in [0, inside, body.len(), inside + 2, 0] {
            h.set_caret(at, at);
            assert_eq!(h.host.buffer(), body, "{name}: caret at {at}");
        }
        assert!(!h.host.is_dirty(), "{name}: rendering is not an edit");
        // An edit elsewhere autosaves the note with its table byte for byte.
        h.set_caret(0, 0);
        h.type_text("!");
        h.advance(1.5);
        assert!(!h.host.is_dirty(), "{name}: autosaved");
        assert_eq!(bytes_on_disk(&p, "Tables"), format!("!{body}"), "{name}");
        // Save As (the core's) writes the same bytes.
        let q = p.with_file_name("copy.omatree");
        let mut doc = Document::open_existing(&p).unwrap();
        doc.save_as(&q, false).unwrap();
        assert_eq!(
            bytes_on_disk(&q, "Tables"),
            format!("!{body}"),
            "{name}: Save As"
        );
        // Reopened, selected, and shown again.
        drop(h);
        let mut h2 = H::open(&p);
        h2.select("Tables");
        assert_eq!(h2.host.buffer(), format!("!{body}"), "{name}: reopened");
    }
}
