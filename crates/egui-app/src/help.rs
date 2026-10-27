//! The F1 keyboard reference. Text only; nothing in it acts and opening it changes nothing in
//! the notebook. Application commands are printed from `keymap`, the editor's from the
//! `EditorKeys` the editor is given, and the structural keys that are fixed in the tree, the
//! search field and the editor are listed here beside the code that handles them
//! (`tree_ui::tree_keys`, `search::search_header`, the editor library's own handling).

use super::layout::GUTTER;
use super::lifecycle::{dialog_buttons, dialog_frame, dialog_heading, dialog_open};
use super::*;
use eframe::egui::KeyboardShortcut;
use egui::{Key, Vec2};

type Section = (&'static str, &'static str, Vec<(String, &'static str)>);

impl Host {
    /// The sections of the reference: heading, where it applies, and (keys, what it does).
    pub(crate) fn shortcut_sections(&self, ctx: &egui::Context) -> Vec<Section> {
        let key = |s: &KeyboardShortcut| ctx.format_shortcut(s);
        let opt = |s: &Option<KeyboardShortcut>| s.as_ref().map(|s| ctx.format_shortcut(s));
        let mut editor: Vec<(String, &'static str)> = Vec::new();
        for (k, what) in [
            (&self.keys.bold, "Bold"),
            (&self.keys.italic, "Italic"),
            (&self.keys.code, "Inline code"),
            (&self.keys.strike, "Strikethrough"),
            (&self.keys.bullets, "Bulleted list"),
            (&self.keys.numbers, "Numbered list"),
            (
                &self.keys.toggle_task,
                "Tick or untick the task on this line",
            ),
        ] {
            if let Some(label) = opt(k) {
                editor.push((label, what));
            }
        }
        editor.push((
            "/".to_owned(),
            "Slash menu at the start of a line or after a space (headings, lists, tasks, code, \
             tables, emoji, images)",
        ));
        if self.keys.strike_command {
            editor.push(("/strike".to_owned(), "Strikethrough, from the slash menu"));
        }
        editor.push((
            ctx.format_shortcut(&KeyboardShortcut::new(egui::Modifiers::COMMAND, Key::Z)),
            "Undo (this note only)",
        ));
        editor.push((
            ctx.format_shortcut(&KeyboardShortcut::new(
                egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
                Key::Z,
            )),
            "Redo",
        ));
        editor.push(("Escape".to_owned(), "Back to the tree"));
        vec![
            (
                "Notes",
                "Anywhere in the note window",
                vec![
                    (key(&keymap::NEW_NOTE), "New note"),
                    (key(&keymap::NEW_CHILD), "New child note"),
                    (key(&keymap::RENAME), "Rename the note"),
                    (key(&keymap::MOVE_UP), "Move the note up among its siblings"),
                    (
                        key(&keymap::MOVE_DOWN),
                        "Move the note down among its siblings",
                    ),
                    (key(&keymap::RECOVERY), "Recovery: Trash and checkpoints"),
                ],
            ),
            (
                "Tree",
                "While the tree has the keyboard",
                vec![
                    ("Up / Down".to_owned(), "Move through the notes"),
                    ("Page Up / Page Down".to_owned(), "Move a page"),
                    ("Enter".to_owned(), "Edit the note"),
                    ("Left / Right".to_owned(), "Collapse / expand"),
                    (key(&keymap::SUBTREE_CLOSE), "Collapse the whole subtree"),
                    (key(&keymap::SUBTREE_OPEN), "Expand the whole subtree"),
                    ("Delete".to_owned(), "Move the note to Trash (asks first)"),
                    (key(&keymap::TREE_MENU), "Open the tree menu"),
                    ("Ctrl+click a marker".to_owned(), "Toggle a whole subtree"),
                ],
            ),
            ("Editor", "While you are editing a note", editor),
            (
                "Search",
                "Ctrl+F opens it from anywhere",
                vec![
                    (key(&keymap::FIND), "Open Search"),
                    ("Up / Down".to_owned(), "Choose a result"),
                    ("Page Up / Page Down".to_owned(), "Move a page of results"),
                    ("Enter".to_owned(), "Open the result"),
                    ("Escape".to_owned(), "Close Search"),
                ],
            ),
            (
                "Files",
                "Anywhere in the note window",
                vec![
                    (key(&keymap::SAVE), "Save"),
                    (key(&keymap::SAVE_AS), "Save As"),
                ],
            ),
            (
                "General",
                "",
                vec![
                    (key(&keymap::HELP), "This reference"),
                    (
                        "Escape".to_owned(),
                        "Close a question, a menu or this reference",
                    ),
                    ("Tab / Shift+Tab".to_owned(), "Move between controls"),
                    ("Enter".to_owned(), "Press the focused button"),
                ],
            ),
        ]
    }

    pub(crate) fn show_shortcuts(&mut self, ctx: &egui::Context, first_frame: bool) {
        let ch = *self.theme.chrome();
        let bold = self.bold.clone();
        let sections = self.shortcut_sections(ctx);
        let max_h = (ctx.content_rect().height() - 190.0).clamp(120.0, 560.0);
        let mut close = false;
        let none = egui::Modifiers::NONE;
        // The list scrolls from the keyboard wherever the keyboard is in this dialog.
        let (down, up, pd, pu, home, end, enter) = ctx.input_mut(|i| {
            (
                i.consume_key(none, Key::ArrowDown),
                i.consume_key(none, Key::ArrowUp),
                i.consume_key(none, Key::PageDown),
                i.consume_key(none, Key::PageUp),
                i.consume_key(none, Key::Home),
                i.consume_key(none, Key::End),
                false,
            )
        });
        let _ = enter;
        let delta = if down {
            -30.0
        } else if up {
            30.0
        } else if pd {
            -max_h
        } else if pu {
            max_h
        } else if end {
            -1.0e6
        } else if home {
            1.0e6
        } else {
            0.0
        };
        let m = egui::Modal::new(egui::Id::unique("dlg-shortcuts"))
            .frame(dialog_frame(ctx))
            .show(ctx, |ui| {
                dialog_open(ui, ctx, 480.0);
                dialog_heading(ui, "Keyboard shortcuts", bold.as_ref(), &ch);
                egui::ScrollArea::vertical()
                    .max_height(max_h)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        if delta != 0.0 {
                            ui.scroll_with_delta(Vec2::new(0.0, delta));
                        }
                        for (heading, context, entries) in &sections {
                            ui.add_space(8.0);
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    egui::RichText::new(heading.to_uppercase())
                                        .monospace()
                                        .size(12.5)
                                        .color(ch.accent),
                                );
                                if !context.is_empty() {
                                    ui.label(
                                        egui::RichText::new(*context).size(12.0).color(ch.muted),
                                    );
                                }
                            });
                            for (keys, what) in entries {
                                ui.horizontal_top(|ui| {
                                    ui.add_sized(
                                        [150.0, 18.0],
                                        egui::Label::new(
                                            egui::RichText::new(keys)
                                                .monospace()
                                                .size(12.5)
                                                .color(ch.accent),
                                        )
                                        .truncate(),
                                    )
                                    .on_hover_text(keys);
                                    ui.add(
                                        egui::Label::new(egui::RichText::new(*what).color(ch.text))
                                            .wrap(),
                                    );
                                });
                            }
                        }
                        ui.add_space(GUTTER / 2.0);
                    });
                dialog_buttons(ui, |ui| {
                    let b = ui.button("Close");
                    if first_frame {
                        b.request_focus();
                    }
                    if b.clicked() {
                        close = true;
                    }
                });
            });
        if close || m.should_close() {
            self.cancel_dialog();
        }
    }
}
