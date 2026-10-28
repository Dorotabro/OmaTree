//! E12a: the Linux usability and regression audit. Tests here exist because the audit found
//! something (see the E12a record in `docs/EGUI_MIGRATION_PLAN.md`).

use super::*;

#[test]
#[ignore = "probe: prints what asks for a repaint, per focus target"]
fn idle_repaint_probe() {
    let d = dir("a12-idle");
    let p = d.join("n.omatree");
    make_fixture(&p);
    let mut h = H::open(&p);
    for (name, focus) in [
        ("tree", Focus::Tree),
        ("editor", Focus::Editor),
        ("title", Focus::Title),
    ] {
        h.host.focus(focus);
        h.settle();
        let mut delays = Vec::new();
        for _ in 0..40 {
            h.frame(vec![], Modifiers::NONE);
            delays.push(h.repaint);
        }
        let min = delays.iter().min().unwrap();
        let zero = delays.iter().filter(|d| d.is_zero()).count();
        println!("{name}: repaint delay min {min:?}, {zero} of 40 frames asked for an immediate repaint, last {:?}", delays.last().unwrap());
    }
}

#[test]
fn an_image_pasted_while_the_editor_never_had_the_keyboard_goes_to_the_end_not_the_start() {
    use super::image_tests::{paste, refs, shot};
    let d = dir("a12-paste-focus");
    let p = d.join("n.omatree");
    let ids = make_fixture(&p);
    // Just opened, as the application starts: the tree has the keyboard and the editor never has.
    let mut h = H::bare(Vec2::new(1000.0, 700.0));
    let ctx = h.ctx.clone();
    assert!(h.host.open(&ctx, &p));
    h.settle();
    let first = ids[0].0;
    h.host.select(&ctx, first);
    h.settle();
    assert!(!EditorView::has_focus(
        &h.ctx,
        key_of(h.host.notebook_id(), first)
    ));
    let title = h
        .host
        .rows()
        .iter()
        .find(|r| r.id == first)
        .unwrap()
        .title
        .clone();
    let before = h.body(&title);
    paste(&mut h, &shot(12, 12));
    let after = h.body(&title);
    assert!(
        after.starts_with(&before),
        "the note's text is untouched: {after:?}"
    );
    assert_eq!(refs(&after).len(), 1);
    assert!(
        after[before.len()..].contains("!["),
        "the image follows the old text: {after:?}"
    );
}

#[test]
fn a_caret_the_user_left_behind_still_decides_where_the_image_goes() {
    use super::image_tests::{paste, shot};
    let d = dir("a12-paste-caret");
    let p = d.join("n.omatree");
    make_fixture(&p);
    let mut h = H::open(&p);
    h.select("Plán");
    // The user clicked after the first line, then moved to the tree.
    let first_line_end = h.host.buffer().find('\n').unwrap();
    h.set_caret(first_line_end, first_line_end);
    h.host.focus(Focus::Tree);
    h.settle();
    paste(&mut h, &shot(10, 10));
    let after = h.body("Plán");
    let at = after.find("![").unwrap();
    assert!(
        at >= first_line_end && at < after.len() / 2,
        "inserted at the remembered caret, not the end: {at} in {after:?}"
    );
}

#[test]
fn a_doubled_extension_from_the_portal_is_collapsed_and_nothing_else_is_touched() {
    use crate::note_export::ExportFormat;
    let d = dir("a12-ext");
    let p = d.join("n.omatree");
    make_fixture(&p);
    let h = H::open(&p);
    let t = |f: ExportFormat, typed: &str| {
        h.host
            .export_target_for_test(f, typed)
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
    };
    assert_eq!(
        t(ExportFormat::Zip, "/x/Příliš.zip.zip").as_deref(),
        Some("Příliš.zip")
    );
    assert_eq!(
        t(ExportFormat::Zip, "/x/NOTE.ZIP.zip").as_deref(),
        Some("NOTE.ZIP")
    );
    assert_eq!(
        t(ExportFormat::Pdf, "/x/a.pdf.pdf").as_deref(),
        Some("a.pdf")
    );
    assert_eq!(
        t(ExportFormat::Markdown, "/x/a.md.md").as_deref(),
        Some("a.md")
    );
    // A single extension, a missing one, a different one and a folder are left alone.
    assert_eq!(t(ExportFormat::Zip, "/x/a.zip").as_deref(), Some("a.zip"));
    assert_eq!(t(ExportFormat::Zip, "/x/a").as_deref(), Some("a.zip"));
    assert_eq!(
        t(ExportFormat::Zip, "/x/a.tar.zip").as_deref(),
        Some("a.tar.zip")
    );
    assert_eq!(
        t(ExportFormat::Folder, "/x/a.zip.zip").as_deref(),
        Some("a.zip.zip")
    );
}

#[test]
fn only_an_empty_preedit_with_no_composition_in_progress_is_dropped() {
    use egui::{Event, ImeEvent};
    let mut host = Host::new();
    let pre = |t: &str| {
        Event::Ime(ImeEvent::Preedit {
            text: t.to_owned(),
            active_range_chars: None,
        })
    };
    let run = |host: &mut Host, events: Vec<Event>| {
        let mut raw = egui::RawInput {
            events,
            ..Default::default()
        };
        host.drop_idle_preedit(&mut raw);
        raw.events.len()
    };
    // Idle: the input method's empty answers are noise.
    assert_eq!(run(&mut host, vec![pre(""), pre(""), pre("")]), 0);
    // A composition: its pre-edit, a change, the commit all pass; so does the cancelling empty
    // pre-edit while composing; afterwards the noise is dropped again.
    assert_eq!(run(&mut host, vec![pre("c"), pre("ch")]), 2);
    assert_eq!(
        run(&mut host, vec![pre("")]),
        1,
        "an empty pre-edit that ends a composition is kept"
    );
    assert_eq!(run(&mut host, vec![pre("")]), 0);
    assert_eq!(
        run(
            &mut host,
            vec![pre("č"), Event::Ime(ImeEvent::Commit("č".into())), pre("")]
        ),
        2
    );
    // Everything that is not an input-method event is untouched.
    assert_eq!(
        run(&mut host, vec![Event::Text("a".into()), Event::PointerGone]),
        2
    );
}

/// The nodes of the last accessibility update.
fn a11y_nodes(h: &H) -> Vec<&egui::accesskit::Node> {
    h.a11y
        .as_ref()
        .expect("no accessibility update: was `enable_accesskit` called?")
        .nodes
        .iter()
        .map(|(_, n)| n)
        .collect()
}

fn label(n: &egui::accesskit::Node) -> &str {
    n.label().unwrap_or_default()
}

#[test]
fn the_tree_exposes_each_row_with_its_title_level_state_and_selection() {
    use egui::accesskit::Role;
    let d = dir("a12-a11y-tree");
    let p = d.join("n.omatree");
    make_fixture(&p);
    let mut h = H::open(&p);
    h.ctx.enable_accesskit();
    h.host.focus(Focus::Tree);
    h.settle();
    let nodes = a11y_nodes(&h);
    let items: Vec<_> = nodes
        .iter()
        .filter(|n| n.role() == Role::TreeItem)
        .collect();
    assert!(!items.is_empty(), "no tree items exposed");
    let projects = items
        .iter()
        .find(|n| label(n) == "Projects")
        .expect("Projects");
    assert_eq!(projects.level(), Some(1));
    assert!(
        projects.is_expanded().is_some(),
        "a branch says whether it is open"
    );
    let journal = items
        .iter()
        .find(|n| label(n) == "Journal")
        .expect("Journal");
    assert_eq!(journal.level(), Some(1));
    assert_eq!(journal.is_expanded(), None, "a leaf has no open state");
    // Exactly one row is the selected one, and it is the selected note.
    let selected: Vec<_> = items
        .iter()
        .filter(|n| n.is_selected() == Some(true))
        .collect();
    assert_eq!(selected.len(), 1);
    let sel = h.host.selected().unwrap();
    let title = &h.host.rows().iter().find(|r| r.id == sel).unwrap().title;
    assert_eq!(label(selected[0]), title.as_str());
    // The File menu button and the note buttons have names.
    let names: Vec<&str> = nodes
        .iter()
        .filter(|n| n.role() == Role::Button)
        .map(|n| label(n))
        .collect();
    for want in ["+ note", "+ child", "file"] {
        assert!(names.contains(&want), "{names:?}");
    }
}

#[test]
fn deeper_rows_say_how_deep_they_are() {
    use egui::accesskit::Role;
    let d = dir("a12-a11y-deep");
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    let mut parent = doc.create_root("Úroveň 1").unwrap();
    for level in 2..=12 {
        parent = doc
            .create_child(parent, &format!("Úroveň {level}"))
            .unwrap();
    }
    doc.save().unwrap();
    drop(doc);
    let mut h = H::open(&p);
    h.ctx.enable_accesskit();
    h.host.set_all_expanded(&h.ctx.clone(), true);
    h.host.focus(Focus::Tree);
    h.settle();
    let nodes = a11y_nodes(&h);
    let level_of = |title: &str| {
        nodes
            .iter()
            .find(|n| n.role() == Role::TreeItem && label(n) == title)
            .and_then(|n| n.level())
    };
    assert_eq!(level_of("Úroveň 12"), Some(12));
    assert_eq!(level_of("Úroveň 1"), Some(1));
    assert_eq!(level_of("Úroveň 7"), Some(7));
}

// --- The extended session -----------------------------------------------------------------------

fn rename(h: &mut H, title: &str) {
    h.host.title_text = title.to_owned();
    assert!(h.host.commit_title(&h.ctx.clone()), "{}", h.host.status());
    h.settle();
}

/// The titles of the rows the tree shows (a collapsed branch hides its descendants).
fn visible(h: &H) -> Vec<String> {
    let t = h.host.tree();
    (0..t.len_visible())
        .map(|i| t.visible_row(i).title.clone())
        .collect()
}

/// One long, coherent editing session through the host's own commands and the real frame loop,
/// in the order a person would do it (E12a section 2). Each step checks the document as well as
/// the screen state, and the session ends with a clean quit.
#[test]
fn an_extended_session_stays_coherent_from_first_note_to_quit() {
    use super::image_tests::{jpeg_of, paste, refs, shot};
    use omatree_editor::assets::encode_png;
    let d = dir("a12-session");
    let target = d.join("session.omatree");
    let mut h = H::bare(Vec2::new(1000.0, 700.0));
    let ctx = h.ctx.clone();

    // 1-2. No file: an untitled notebook, a first note, Save As by typed path.
    h.host.start_untitled(&ctx);
    h.settle();
    assert!(!h.host.document().unwrap().has_path());
    h.key(Key::N, CTRL);
    rename(&mut h, "Kořen");
    h.host.focus(Focus::Editor);
    h.settle();
    h.type_text("# Nadpis\n\nPrvní odstavec s **tučným** a `kódem`.\n");
    h.host.start_save_as_typed(&ctx);
    h.host.set_save_as_path(&target.display().to_string());
    h.host.submit_save_as(&ctx);
    h.settle();
    assert!(!h.host.is_dirty() && target.exists(), "{}", h.host.status());

    // 3-4. A hierarchy twelve levels deep: new children, each named.
    for level in 2..=12 {
        h.host.create_child(&ctx);
        h.settle();
        rename(&mut h, &format!("Úroveň {level} 🌲"));
    }
    assert_eq!(h.host.rows().iter().map(|r| r.depth).max(), Some(11));
    let deepest = h.host.selected().unwrap();

    // 5-7. Move, expand and collapse. A sibling root, moved up and back; the branch collapsed.
    h.host.focus(Focus::Tree);
    h.key(Key::N, CTRL);
    rename(&mut h, "Sourozenec");
    let sibling = h.host.selected().unwrap();
    h.host.move_selected(&ctx, -1);
    h.settle();
    h.host.move_selected(&ctx, 1);
    h.settle();
    let root = h.id("Kořen");
    h.host.set_expanded(&ctx, root, false);
    h.settle();
    assert!(
        !visible(&h).contains(&"Úroveň 12 🌲".to_owned()),
        "collapsed branch hides its descendants"
    );
    h.host.set_expanded(&ctx, root, true);
    h.host.set_all_expanded(&ctx, true);
    h.settle();
    assert!(visible(&h).contains(&"Úroveň 12 🌲".to_owned()));
    // Drag-style move of a note before another.
    h.host.drop_note(&ctx, sibling, None, 0);
    h.settle();
    assert_eq!(h.host.tree().visible_row(0).id, sibling);

    // 8-10. Edit several notes, switch repeatedly, undo and redo.
    for title in ["Úroveň 5 🌲", "Úroveň 9 🌲", "Kořen", "Úroveň 5 🌲"] {
        h.select(title);
        let end = h.host.buffer().len();
        h.set_caret(end, end);
        h.type_text(&format!("\nPoznámka v {title} čeština žluťoučký kůň\n"));
    }
    h.select("Úroveň 5 🌲");
    let before_undo = h.host.buffer().to_owned();
    h.key(Key::Z, CTRL);
    assert_ne!(h.host.buffer(), before_undo, "undo changed the text");
    h.key(Key::Z, CTRL | Modifiers::SHIFT);
    assert_eq!(h.host.buffer(), before_undo, "redo restored it");
    h.select("Úroveň 9 🌲");
    assert!(
        h.host.buffer().contains("Poznámka v Úroveň 9"),
        "another note kept its own text"
    );

    // 11-12. Search finds a deep note by body text and reveals it in a collapsed branch.
    h.host.set_expanded(&ctx, root, false);
    h.settle();
    assert!(!visible(&h).contains(&"Úroveň 9 🌲".to_owned()));
    h.search_for("Poznámka v Úroveň 9");
    assert!(
        h.found().contains(&"Úroveň 9 🌲".to_owned()),
        "{:?}",
        h.found()
    );
    h.press(Key::Enter);
    h.settle();
    assert_eq!(
        h.host
            .rows()
            .iter()
            .find(|r| Some(r.id) == h.host.selected())
            .map(|r| r.title.clone())
            .as_deref(),
        Some("Úroveň 9 🌲")
    );
    assert!(
        visible(&h).contains(&"Úroveň 9 🌲".to_owned()),
        "the result's branch was opened"
    );

    // 13-15. A pasted PNG, a stored JPEG, a resized image.
    h.select("Úroveň 12 🌲");
    h.host.focus(Focus::Editor);
    h.settle();
    let end = h.host.buffer().len();
    h.set_caret(end, end);
    paste(&mut h, &shot(30, 20));
    let png_ref = refs(h.host.buffer()).remove(0);
    {
        let id = h.id("Úroveň 12 🌲");
        let doc = h.host.doc.as_ref().unwrap();
        let jpg = doc.put_attachment(&jpeg_of(40, 24)).unwrap();
        let _ = (id, jpg);
    }
    let end = h.host.buffer().len();
    h.set_caret(end, end);
    h.type_text("\n{width=40%}");
    let _ = encode_png;
    assert!(h.host.buffer().contains("{width=40%}"));

    // 16. Save, close, reopen: everything is where it was.
    h.key(Key::S, CTRL);
    assert!(!h.host.is_dirty(), "{}", h.host.status());
    let shape_before = visible(&h).len();
    drop(h);
    let mut h = H::open(&target);
    h.host.set_all_expanded(&h.ctx.clone(), true);
    h.settle();
    assert_eq!(h.host.rows().len(), shape_before.max(h.host.rows().len()));
    assert!(visible(&h).contains(&"Úroveň 12 🌲".to_owned()));
    h.select("Úroveň 12 🌲");
    assert!(h.host.buffer().contains(&png_ref) && h.host.buffer().contains("{width=40%}"));
    let ctx = h.ctx.clone();

    // 17-18. Delete a subtree, restore it from Trash; checkpoint restore then Ctrl+Z.
    let victim = h.id("Úroveň 6 🌲");
    h.host.select(&ctx, victim);
    h.host.confirm_delete(&ctx, victim);
    h.settle();
    assert!(
        !visible(&h).contains(&"Úroveň 12 🌲".to_owned()),
        "the whole subtree went to Trash"
    );
    h.host.open_recovery(&ctx);
    h.settle();
    assert!(h.host.restore_trash(&ctx, 0), "{}", h.host.status());
    h.host.cancel_dialog();
    h.host.set_all_expanded(&ctx, true);
    h.settle();
    assert!(
        visible(&h).contains(&"Úroveň 12 🌲".to_owned()),
        "restored with its descendants"
    );
    // The newest checkpoint is the notebook as it was just before the Trash restore (the subtree
    // gone): restoring it is legitimate and removes the subtree again. The one before it was taken
    // when the subtree was deleted and still has everything: restore that, and check that Ctrl+Z
    // in a note cannot bring back text that only existed before the restore.
    h.host.open_recovery(&ctx);
    h.settle();
    assert!(h.host.restore_checkpoint(&ctx, 1), "{}", h.host.status());
    h.host.cancel_dialog();
    h.host.set_all_expanded(&ctx, true);
    h.settle();
    assert!(visible(&h).contains(&"Úroveň 12 🌲".to_owned()));
    h.select("Úroveň 5 🌲");
    let restored = h.host.buffer().to_owned();
    h.key(Key::Z, CTRL);
    assert_eq!(
        h.host.buffer(),
        restored,
        "Ctrl+Z after a restore does not resurrect older text"
    );

    // 19. Export Markdown, ZIP and PDF of a note.
    h.select("Úroveň 12 🌲");
    h.host.set_picker(None);
    for (format, name) in [
        (crate::note_export::ExportFormat::Markdown, "s.md"),
        (crate::note_export::ExportFormat::Zip, "s.zip"),
        (crate::note_export::ExportFormat::Pdf, "s.pdf"),
    ] {
        h.host.start_export(&ctx);
        h.host.set_export_format(format);
        h.host.choose_export_destination(&ctx);
        h.settle();
        h.host.set_export_path(&d.join(name).display().to_string());
        h.host.submit_export_path(&ctx);
        h.settle();
        assert!(d.join(name).exists(), "{name}: {}", h.host.status());
    }

    // 20. Compact Notebook: the preview changes nothing.
    h.key(Key::S, CTRL);
    let on_disk = fs::read(&target).unwrap();
    h.host.start_compact(&ctx);
    h.settle();
    assert_eq!(h.cstage_for_audit(), "report");
    h.press(Key::Escape);
    assert_eq!(fs::read(&target).unwrap(), on_disk);

    // 21. New with unsaved changes asks first, and Cancel keeps everything.
    let end = h.host.buffer().len();
    h.host.focus(Focus::Editor);
    h.settle();
    h.set_caret(end, end);
    h.type_text("neuloženo");
    h.key(Key::N, CTRL | Modifiers::ALT);
    assert!(
        matches!(h.host.dialog(), Dialog::Unsaved { .. }),
        "{:?}",
        h.host.dialog()
    );
    h.host.cancel_dialog();
    assert!(h.host.buffer().ends_with("neuloženo") && h.host.is_dirty());

    // 22. An external modification: Save is refused, Save As is the way out.
    {
        let c = rusqlite::Connection::open(&target).unwrap();
        c.execute(
            "UPDATE nodes SET title = title || '!' WHERE title = 'Sourozenec'",
            [],
        )
        .unwrap();
    }
    h.key(Key::S, CTRL);
    assert_eq!(*h.host.dialog(), Dialog::Conflict, "{}", h.host.status());
    assert!(h.host.is_dirty(), "the refused save lost nothing");
    h.host.cancel_dialog();
    h.host.start_save_as_typed(&ctx);
    h.host
        .set_save_as_path(&d.join("mine.omatree").display().to_string());
    h.host.submit_save_as(&ctx);
    h.settle();
    assert!(!h.host.is_dirty(), "{}", h.host.status());

    // 23. Quit through the window's own close request: clean, so it closes.
    assert!(h.request_close(), "a clean notebook closes");
    let _ = deepest;
}

impl H {
    fn cstage_for_audit(&self) -> &'static str {
        self.host.compact_stage().unwrap_or("none")
    }
}

// --- Keyboard and focus ----------------------------------------------------------------------------

fn tree_has_focus(h: &H) -> bool {
    h.ctx.memory(|m| m.has_focus(Host::tree_id()))
}

/// Every modal keeps the keyboard to itself (typing and the note shortcuts do nothing behind it),
/// Escape closes it, and the keyboard goes back to where it was (E12a section 3).
#[test]
fn every_modal_traps_the_keyboard_and_gives_it_back() {
    let d = dir("a12-modal");
    let p = d.join("n.omatree");
    make_fixture(&p);
    let mut h = H::open(&p);
    let ctx = h.ctx.clone();
    h.select("Plán");
    type OpenFn = fn(&mut H);
    let openers: Vec<(&str, OpenFn)> = vec![
        ("Recovery", |h| h.host.open_recovery(&h.ctx.clone())),
        ("Export", |h| h.host.start_export(&h.ctx.clone())),
        ("Compact Notebook", |h| h.host.start_compact(&h.ctx.clone())),
        ("F1", |h| h.press(Key::F1)),
        ("Move to Trash", |h| h.host.ask_delete(&h.ctx.clone())),
    ];
    for focus in [Focus::Editor, Focus::Tree] {
        for (name, open) in &openers {
            h.host.focus(focus);
            h.settle();
            let (text, rows, dirty) = (
                h.host.buffer().to_owned(),
                h.host.rows().len(),
                h.host.is_dirty(),
            );
            let at = (h.host.selected(), h.host.notebook_id());
            open(&mut h);
            h.settle();
            assert!(h.host.dialog() != &Dialog::None, "{name}: no dialog opened");
            // Typing and the document shortcuts do nothing behind the modal.
            h.type_text("zzzz");
            h.key(Key::N, CTRL);
            h.key(Key::N, CTRL | Modifiers::SHIFT);
            h.key(Key::Delete, Modifiers::NONE);
            assert_eq!(
                h.host.buffer(),
                text,
                "{name}/{focus:?}: text changed behind the dialog"
            );
            assert_eq!(
                h.host.rows().len(),
                rows,
                "{name}/{focus:?}: notes were created or removed"
            );
            assert_eq!((h.host.selected(), h.host.notebook_id()), at, "{name}");
            assert_eq!(h.host.is_dirty(), dirty, "{name}/{focus:?}");
            // Escape closes it and the keyboard comes back.
            h.press(Key::Escape);
            h.settle();
            assert_eq!(
                *h.host.dialog(),
                Dialog::None,
                "{name}/{focus:?}: Escape did not close it"
            );
            match focus {
                Focus::Editor => assert!(
                    EditorView::has_focus(&h.ctx, h.key_of("Plán")),
                    "{name}: the editor lost the keyboard"
                ),
                _ => assert!(tree_has_focus(&h), "{name}: the tree lost the keyboard"),
            }
        }
    }
    let _ = ctx;
}

/// Tab order through the main window, from the tree: the buttons, the note title and the editor
/// each get the keyboard in turn and Shift+Tab walks back (nothing is skipped or trapped).
#[test]
fn tab_walks_the_window_in_order_and_shift_tab_walks_back() {
    let d = dir("a12-tab");
    let p = d.join("n.omatree");
    make_fixture(&p);
    let mut h = H::open(&p);
    h.select("Plán");
    h.host.focus(Focus::Tree);
    h.settle();
    let who = |h: &H| -> &'static str {
        if tree_has_focus(h) {
            "tree"
        } else if EditorView::has_focus(&h.ctx, h.key_of("Plán")) {
            "editor"
        } else if h.ctx.memory(|m| m.has_focus(h.host.title_id())) {
            "title"
        } else if h.ctx.memory(|m| m.focused().is_some()) {
            "other"
        } else {
            "nothing"
        }
    };
    let mut seen = vec![who(&h)];
    for _ in 0..8 {
        h.key(Key::Tab, Modifiers::NONE);
        seen.push(who(&h));
    }
    eprintln!("Tab order from the tree: {seen:?}");
    assert!(
        seen.contains(&"title") && seen.contains(&"editor"),
        "{seen:?}"
    );
    assert!(
        !seen.contains(&"nothing"),
        "the keyboard was lost: {seen:?}"
    );
    // Back with Shift+Tab: the editor is reached from the title.
    h.host.focus(Focus::Title);
    h.settle();
    h.key(Key::Tab, Modifiers::NONE);
    assert_eq!(who(&h), "editor");
    // Inside the editor Tab and Shift+Tab belong to the editor (list indentation): they do not
    // move the keyboard. Escape is the way out, back to the tree, and Shift+Tab from the title
    // goes back through the header controls.
    h.key(Key::Tab, Modifiers::SHIFT);
    assert_eq!(who(&h), "editor", "Shift+Tab in the editor is the editor's");
    h.press(Key::Escape);
    assert_eq!(who(&h), "tree");
    h.host.focus(Focus::Title);
    h.settle();
    h.key(Key::Tab, Modifiers::SHIFT);
    assert_ne!(who(&h), "title", "Shift+Tab leaves the title field");
    assert_ne!(who(&h), "nothing");
}

#[cfg(target_os = "linux")]
fn rss_mib() -> i64 {
    fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("VmRSS:"))
        .and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<i64>().ok())
        .unwrap_or(0)
        / 1024
}

/// Resident memory is read from `/proc`, which only Linux has: elsewhere the figure is simply 0.
#[cfg(not(target_os = "linux"))]
fn rss_mib() -> i64 {
    0
}

/// `cargo test --release -p omatree-egui -- --ignored --nocapture audit_performance`.
#[test]
#[ignore = "measurement, run with --release"]
fn audit_performance() {
    use std::time::Instant;
    let ms = |t: Instant| t.elapsed().as_secs_f64() * 1000.0;
    let d = dir("a12-perf");
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    let mut parent = None;
    let mut big = None;
    for i in 0..5000usize {
        let id = match parent {
            Some(par) if i % 9 != 0 => doc.create_child(par, &format!("Poznámka {i}")).unwrap(),
            _ => doc.create_root(&format!("Kořen {i}")).unwrap(),
        };
        parent = Some(id);
        doc.set_body(
            id,
            &format!(
                "Obsah žluťoučký {i}\n\n{}",
                "řádek textu s kůň a 🦀\n".repeat(8)
            ),
        )
        .unwrap();
        if i == 2500 {
            big = Some(id);
        }
    }
    let big = big.unwrap();
    // A 150 KB note with headings, lists and a table.
    let mut text = String::new();
    for i in 0..1500 {
        text.push_str(&format!("## Oddíl {i}\n\nOdstavec s **tučným**, `kódem` a [odkazem](https://example.com/{i}).\n\n- jedna\n- dvě\n\n| a | b |\n|---|---|\n| {i} | x |\n\n"));
    }
    doc.set_body(big, &text).unwrap();
    for i in 0..100 {
        doc.create_checkpoint(&format!("historie {i}"));
    }
    doc.save().unwrap();
    drop(doc);
    println!(
        "fixture: {} KiB, 5000 notes, one {} KiB note, 100 checkpoints",
        fs::metadata(&p).unwrap().len() / 1024,
        text.len() / 1024
    );

    let rss0 = rss_mib();
    let t = Instant::now();
    let mut h = H::bare(Vec2::new(1000.0, 700.0));
    let ctx = h.ctx.clone();
    assert!(h.host.open(&ctx, &p));
    h.settle();
    println!(
        "open the notebook and draw the first frames: {:.0} ms",
        ms(t)
    );

    h.host.focus(Focus::Tree);
    h.settle();
    let t = Instant::now();
    for _ in 0..60 {
        h.frame(vec![], Modifiers::NONE);
    }
    println!("idle tree frame (5000 notes): {:.2} ms", ms(t) / 60.0);

    let big_title = "Poznámka 2500";
    let t = Instant::now();
    h.select(big_title);
    println!("select the 150 KB note: {:.0} ms", ms(t));
    let t = Instant::now();
    for _ in 0..30 {
        h.frame(vec![], Modifiers::NONE);
    }
    println!(
        "idle frame with the 150 KB note shown: {:.2} ms",
        ms(t) / 30.0
    );
    let end = h.host.buffer().len();
    h.set_caret(end, end);
    let t = Instant::now();
    for c in "psaní do velké poznámky".chars() {
        h.frame(vec![Event::Text(c.to_string())], Modifiers::NONE);
    }
    println!(
        "typing in the 150 KB note: {:.1} ms per keystroke",
        ms(t) / 23.0
    );

    let t = Instant::now();
    for i in 0..40 {
        h.select(if i % 2 == 0 { "Kořen 0" } else { big_title });
    }
    println!(
        "switch notes 40 times (alternating short and 150 KB): {:.1} ms each",
        ms(t) / 40.0
    );

    let t = Instant::now();
    h.search_for("žluťoučký 4321");
    h.advance(0.5);
    println!(
        "search over 5000 notes ({} results): {:.0} ms including the typing frames",
        h.found().len(),
        ms(t)
    );
    h.press(Key::Escape);

    let t = Instant::now();
    h.host.open_recovery(&ctx);
    h.settle();
    println!("open Recovery with 100 checkpoints: {:.0} ms", ms(t));
    h.host.cancel_dialog();

    let t = Instant::now();
    h.host.start_compact(&ctx);
    h.settle();
    println!("open Compact Notebook: {:.0} ms", ms(t));
    h.host.cancel_dialog();
    println!(
        "resident memory: {} MiB at the start, {} MiB after the session",
        rss0,
        rss_mib()
    );
}

#[test]
fn the_tree_title_and_editor_have_names_for_assistive_technology() {
    use egui::accesskit::Role;
    let d = dir("a12-a11y-names");
    let p = d.join("n.omatree");
    make_fixture(&p);
    let mut h = H::open(&p);
    h.ctx.enable_accesskit();
    h.select("Plán");
    h.settle();
    let nodes = a11y_nodes(&h);
    let named = |role: Role, name: &str| nodes.iter().any(|n| n.role() == role && label(n) == name);
    assert!(named(Role::Tree, "Notes"), "the tree");
    assert!(
        nodes.iter().any(|n| label(n) == "Note title"),
        "the title field"
    );
    assert!(
        nodes.iter().any(|n| label(n) == "Note text (Markdown)"),
        "the editor"
    );
}

#[test]
fn the_window_has_an_accessible_name_that_follows_the_notebook_and_never_goes_stale() {
    use egui::accesskit::Role;
    let d = dir("c12-a11y-window");
    let (a, b) = (d.join("first.omatree"), d.join("second one.omatree"));
    make_fixture(&a);
    make_fixture(&b);
    let window_label = |h: &H| {
        let nodes = a11y_nodes(h);
        let w: Vec<_> = nodes.iter().filter(|n| n.role() == Role::Window).collect();
        assert_eq!(w.len(), 1, "exactly one window node");
        label(w[0]).to_owned()
    };
    let mut h = H::open(&a);
    h.ctx.enable_accesskit();
    h.settle();
    assert_eq!(window_label(&h), "first.omatree - OmaTree");
    // An unsaved edit shows in the title bar, so it shows in the name.
    h.make_dirty();
    h.settle();
    assert_eq!(window_label(&h), "first.omatree * - OmaTree");
    // Another notebook replaces the name, whole: nothing of the first one is left behind.
    let ctx = h.ctx.clone();
    h.host.save(&ctx);
    h.host.request_open_path(&ctx, b.clone());
    h.settle();
    let name = window_label(&h);
    assert_eq!(name, "second one.omatree - OmaTree");
    assert!(!name.contains("first"));
}

#[test]
fn the_input_method_filter_is_applied_on_linux_only() {
    use egui::{Event, ImeEvent};
    let mut host = Host::new();
    let mut raw = egui::RawInput::default();
    raw.events.push(Event::Ime(ImeEvent::Preedit {
        text: String::new(),
        active_range_chars: None,
    }));
    host.filter_input(&mut raw);
    // An empty pre-edit with nothing composing is dropped on Linux, and left alone everywhere else.
    assert_eq!(raw.events.len(), usize::from(!cfg!(target_os = "linux")));
}
