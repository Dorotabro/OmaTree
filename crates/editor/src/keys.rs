//! Keyboard bindings the editor itself consumes, supplied by the host.
//!
//! The editor consumes only what is configured here. Document-level actions (Save, Save As, Open,
//! opening an export dialog) belong to the host and are never handled by [`crate::EditorView`];
//! `export` is the one exception that only *reports* intent ([`crate::EditorOutput`]) so a host can
//! reuse the standalone editor's Ctrl+Shift+E without an extra key handler. Structural keys that are
//! part of editing itself (Enter and Backspace in list items, the slash menu and emoji picker
//! navigation, Escape) are not configurable.
//!
//! Bindings use `Modifiers::COMMAND`, which egui maps to Ctrl on Linux and Windows and to Cmd on
//! macOS.

use egui::{Context, Key, KeyboardShortcut, Modifiers};

/// What an editing shortcut does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Bold,
    Italic,
    Code,
    Strike,
    Bullets,
    Numbers,
    ToggleTask,
    Export,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditorKeys {
    pub bold: Option<KeyboardShortcut>,
    pub italic: Option<KeyboardShortcut>,
    pub code: Option<KeyboardShortcut>,
    pub strike: Option<KeyboardShortcut>,
    pub bullets: Option<KeyboardShortcut>,
    pub numbers: Option<KeyboardShortcut>,
    /// Flip the task box on the caret line.
    pub toggle_task: Option<KeyboardShortcut>,
    /// Report [`crate::EditorOutput::export_requested`]; the host decides what that opens.
    pub export: Option<KeyboardShortcut>,
    /// Offer `/strike` in the slash menu (for profiles without a strikethrough shortcut).
    pub strike_command: bool,
}

fn cmd(key: Key) -> Option<KeyboardShortcut> {
    Some(KeyboardShortcut::new(Modifiers::COMMAND, key))
}

fn cmd_shift(key: Key) -> Option<KeyboardShortcut> {
    Some(KeyboardShortcut::new(
        Modifiers::COMMAND | Modifiers::SHIFT,
        key,
    ))
}

impl EditorKeys {
    /// The standalone editor: Ctrl+B/I/E, Ctrl+Shift+S strikethrough, Ctrl+Shift+8/7 lists,
    /// Ctrl+Enter task toggle, Ctrl+Shift+E reports an export request.
    pub fn standalone() -> Self {
        EditorKeys {
            bold: cmd(Key::B),
            italic: cmd(Key::I),
            code: cmd(Key::E),
            strike: cmd_shift(Key::S),
            bullets: cmd_shift(Key::Num8),
            numbers: cmd_shift(Key::Num7),
            toggle_task: cmd(Key::Enter),
            export: cmd_shift(Key::E),
            strike_command: false,
        }
    }

    /// OmaTree: Ctrl+E is inline code and Ctrl+Shift+S is left to the application (Save As), so
    /// strikethrough is the `/strike` slash command. Export is an application command too.
    pub fn omatree() -> Self {
        EditorKeys {
            strike: None,
            export: None,
            strike_command: true,
            ..Self::standalone()
        }
    }

    /// No shortcuts at all (everything stays with the host).
    pub fn none() -> Self {
        EditorKeys {
            bold: None,
            italic: None,
            code: None,
            strike: None,
            bullets: None,
            numbers: None,
            toggle_task: None,
            export: None,
            strike_command: false,
        }
    }

    fn bindings(&self) -> [(Action, Option<KeyboardShortcut>); 8] {
        [
            (Action::Bold, self.bold),
            (Action::Italic, self.italic),
            (Action::Code, self.code),
            (Action::Strike, self.strike),
            (Action::Bullets, self.bullets),
            (Action::Numbers, self.numbers),
            (Action::ToggleTask, self.toggle_task),
            (Action::Export, self.export),
        ]
    }

    /// Consume every configured shortcut pressed this frame and return their actions.
    ///
    /// egui matches a shortcut even when the user holds *extra* Shift, so shortcuts with more
    /// modifiers are tried first: with Ctrl+E and Ctrl+Shift+E both configured, Ctrl+Shift+E must not
    /// be taken by Ctrl+E.
    pub fn consume(&self, ctx: &Context, only: &[Action]) -> Vec<Action> {
        let mut list: Vec<(Action, KeyboardShortcut)> = self
            .bindings()
            .into_iter()
            .filter(|(a, _)| only.contains(a))
            .filter_map(|(a, s)| s.map(|s| (a, s)))
            .collect();
        // Alt and Shift each count one; Ctrl, Cmd and "command" together count one.
        let weight = |m: &Modifiers| {
            u8::from(m.alt) + u8::from(m.shift) + u8::from(m.ctrl || m.command || m.mac_cmd)
        };
        list.sort_by_key(|(_, s)| std::cmp::Reverse(weight(&s.modifiers)));
        ctx.input_mut(|i| {
            list.into_iter()
                .filter(|(_, s)| i.consume_shortcut(s))
                .map(|(a, _)| a)
                .collect()
        })
    }

    /// Whether `action`'s shortcut is pressed now, without consuming it.
    pub fn pressed(&self, ctx: &Context, action: Action) -> bool {
        let Some((_, Some(s))) = self.bindings().into_iter().find(|(a, _)| *a == action) else {
            return false;
        };
        ctx.input(|i| i.key_pressed(s.logical_key) && i.modifiers.matches_logically(s.modifiers))
    }

    /// Consume one action's shortcut.
    pub fn consume_one(&self, ctx: &Context, action: Action) -> bool {
        self.consume(ctx, &[action]).contains(&action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_differ_only_where_omatree_needs_them_to() {
        let (s, o) = (EditorKeys::standalone(), EditorKeys::omatree());
        assert_eq!(s.code, o.code, "Ctrl+E is inline code in both");
        assert_eq!(s.bold, o.bold);
        assert!(s.strike.is_some() && o.strike.is_none());
        assert!(s.export.is_some() && o.export.is_none());
        assert!(!s.strike_command && o.strike_command);
        assert_eq!(
            EditorKeys {
                strike: None,
                export: None,
                strike_command: true,
                ..s
            },
            o
        );
    }
}
