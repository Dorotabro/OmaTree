//! M1: which notebook is open, and where it is saved. The name sits quietly in the status bar
//! with the full location in its tooltip, and the notebook menu opens with both. Everything is
//! derived from the open `Document` and the host's path: nothing about the notebook is stored a
//! second time.

use super::*;

fn painted_texts(h: &H) -> Vec<String> {
    h.painted().texts.into_iter().map(|t| t.text).collect()
}

#[test]
fn an_unsaved_notebook_says_untitled_and_where_saving_will_ask() {
    let mut h = H::bare(Vec2::new(1000.0, 700.0));
    h.host.start_with_args(&h.ctx.clone(), &[]);
    h.settle();
    let id = h.host.identity().expect("a notebook is open");
    assert_eq!(id.name, "Untitled");
    assert_eq!(id.location, None);
    assert!(
        id.tooltip("Ctrl+S").contains("not been saved"),
        "{}",
        id.tooltip("Ctrl+S")
    );
    assert!(id.tooltip("Ctrl+S").contains("Ctrl+S"));
    assert!(
        painted_texts(&h).iter().any(|t| t == "Untitled"),
        "{:?}",
        painted_texts(&h)
    );
}

#[test]
fn a_saved_notebook_shows_its_file_name_and_full_location() {
    let d = dir("id-saved");
    let p = d.join("n.omatree");
    make_fixture(&p);
    let mut h = H::open(&p);
    let id = h.host.identity().unwrap();
    assert_eq!(id.name, "n.omatree");
    assert_eq!(
        id.location.as_deref(),
        Some(p.display().to_string().as_str())
    );
    assert_eq!(id.tooltip("Ctrl+S"), p.display().to_string());
    assert!(painted_texts(&h).iter().any(|t| t == "n.omatree"));
    // Editing does not change who it is.
    h.dirty_edit("Journal", "x");
    assert_eq!(h.host.identity().unwrap(), id);
}

#[test]
fn spaces_czech_letters_and_emoji_in_the_path_are_shown_exactly() {
    let d = dir("id-unicode");
    let folder = d.join("Příliš žluťoučký kůň 🦀");
    fs::create_dir_all(&folder).unwrap();
    let p = folder.join("poznámky s mezerou.omatree");
    make_fixture(&p);
    let h = H::open(&p);
    let id = h.host.identity().unwrap();
    assert_eq!(id.name, "poznámky s mezerou.omatree");
    assert_eq!(id.location.unwrap(), p.display().to_string());
    assert!(painted_texts(&h)
        .iter()
        .any(|t| t == "poznámky s mezerou.omatree"));
}

#[test]
fn a_relative_path_is_shown_as_an_absolute_location() {
    let d = dir("id-relative");
    let p = d.join("n.omatree");
    make_fixture(&p);
    let rel = std::env::current_dir()
        .ok()
        .and_then(|cwd| p.strip_prefix(&cwd).ok().map(Path::to_path_buf));
    // Only when the temporary directory happens to lie below the working directory can a relative
    // spelling be made; the rule is tested directly either way.
    let shown = identity::absolute_for_display(Path::new("some/where/n.omatree"));
    assert!(Path::new(&shown).is_absolute(), "{shown}");
    assert!(shown.ends_with("n.omatree"));
    let _ = rel;
}

#[test]
fn save_as_updates_the_identity_and_open_replaces_it() {
    let d = dir("id-saveas");
    let p = d.join("n.omatree");
    make_fixture(&p);
    let mut h = H::open(&p);
    h.dirty_edit("Journal", "mine");
    h.host.start_save_as(&h.ctx.clone(), false);
    let target = d.join("kopie s mezerou.omatree");
    h.host.set_save_as_path(&target.display().to_string());
    h.host.submit_save_as(&h.ctx.clone());
    h.settle();
    let id = h.host.identity().unwrap();
    assert_eq!(id.name, "kopie s mezerou.omatree");
    assert_eq!(id.location.unwrap(), target.display().to_string());
    assert!(painted_texts(&h)
        .iter()
        .any(|t| t == "kopie s mezerou.omatree"));

    // Open another notebook.
    let q = d.join("other.omatree");
    make_fixture(&q);
    h.host.request_open_path(&h.ctx.clone(), q.clone());
    h.settle();
    assert_eq!(h.host.identity().unwrap().name, "other.omatree");
    // A new notebook is untitled again.
    h.host.new_notebook(&h.ctx.clone());
    h.settle();
    assert_eq!(h.host.identity().unwrap().name, "Untitled");
    assert_eq!(h.host.identity().unwrap().location, None);
}

#[test]
fn reload_from_disk_keeps_the_identity() {
    let d = dir("id-reload");
    let p = d.join("n.omatree");
    make_fixture(&p);
    let mut h = H::open(&p);
    let before = h.host.identity().unwrap();
    h.host.request_reload(&h.ctx.clone());
    h.frames(6);
    assert_eq!(h.host.identity().unwrap(), before);
}

#[test]
fn a_narrow_window_cuts_the_name_instead_of_crowding_the_bar() {
    let d = dir("id-narrow");
    let p = d.join("a notebook with a really quite long file name indeed.omatree");
    make_fixture(&p);
    let mut h = H::open_sized(&p, Vec2::new(420.0, 600.0));
    h.settle();
    let painted = h.painted();
    let shown = painted
        .texts
        .iter()
        .find(|t| t.text.starts_with("a notebook with"))
        .expect("the name is shown, cut");
    assert!(
        shown.elided || shown.size.x < 420.0 * 0.45,
        "{:?}",
        shown.size
    );
    assert!(
        shown.pos.x + shown.size.x <= 420.0 + 0.5,
        "inside the window: {:?}",
        shown.pos
    );
    // The status bar's other text still fits beside it (the dirty marker, once there is one).
    h.dirty_edit("Journal", "x");
    assert!(painted_texts(&h)
        .iter()
        .any(|t| t.contains("Unsaved changes")));
}

#[test]
fn the_notebook_menu_opens_with_the_name_and_folder() {
    let d = dir("id-menu");
    let p = d.join("n.omatree");
    make_fixture(&p);
    let h = H::open(&p);
    let head = h.host.identity().unwrap().menu_lines();
    assert_eq!(head.0, "n.omatree");
    assert_eq!(head.1.as_deref(), Some(d.display().to_string().as_str()));
    let mut u = H::bare(Vec2::new(1000.0, 700.0));
    u.host.start_with_args(&u.ctx.clone(), &[]);
    let head = u.host.identity().unwrap().menu_lines();
    assert_eq!(head.0, "Untitled");
    assert_eq!(head.1.as_deref(), Some("not saved yet"));
}
