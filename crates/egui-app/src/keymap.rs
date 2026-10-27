//! The application-wide shortcuts, defined once. `Host::handle_shortcuts` consumes them from
//! here and the F1 reference prints them from here (through egui's own formatter, so the labels
//! name the keys that work on the running platform), and the editor's bindings are read from the
//! `EditorKeys` the host hands the editor. Nothing is spelled out twice.

use eframe::egui::{Key, KeyboardShortcut, Modifiers};

const CMD: Modifiers = Modifiers::COMMAND;
const CMD_SHIFT: Modifiers = Modifiers::COMMAND.plus(Modifiers::SHIFT);

pub(crate) const SAVE: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::S);
pub(crate) const SAVE_AS: KeyboardShortcut = KeyboardShortcut::new(CMD_SHIFT, Key::S);
pub(crate) const NEW_NOTE: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::N);
pub(crate) const NEW_CHILD: KeyboardShortcut = KeyboardShortcut::new(CMD_SHIFT, Key::N);
pub(crate) const OPEN: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::O);
pub(crate) const NEW_NOTEBOOK: KeyboardShortcut =
    KeyboardShortcut::new(CMD.plus(Modifiers::ALT), Key::N);
pub(crate) const FIND: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::F);
pub(crate) const RECOVERY: KeyboardShortcut = KeyboardShortcut::new(CMD_SHIFT, Key::R);
pub(crate) const MOVE_UP: KeyboardShortcut = KeyboardShortcut::new(Modifiers::ALT, Key::ArrowUp);
pub(crate) const MOVE_DOWN: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::ALT, Key::ArrowDown);
pub(crate) const RENAME: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::F2);
pub(crate) const HELP: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::F1);
pub(crate) const TREE_MENU: KeyboardShortcut = KeyboardShortcut::new(Modifiers::SHIFT, Key::F10);
pub(crate) const SUBTREE_OPEN: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::ArrowRight);
pub(crate) const SUBTREE_CLOSE: KeyboardShortcut = KeyboardShortcut::new(CMD, Key::ArrowLeft);
