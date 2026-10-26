//! Drawing and keyboard handling of the tree pane and the editable title.
//!
//! The tree is one focusable widget (`Host::tree_id`); rows are painted by hand and only the rows
//! in the viewport are touched, so a frame costs the same for 50 notes or 50,000. Keys act only
//! while the tree has the keyboard, which keeps them away from text being edited; the commands
//! that are application-wide (new note, rename, move) are handled in `Host::handle_shortcuts`.

use super::*;
use egui::{Color32, FontId, Key, Pos2, Rect, Sense, Shape, Stroke, Vec2};

const ROW_H: f32 = 26.0;
const INDENT: f32 = 16.0;
const LEFT: f32 = 8.0;

/// What a click, key or menu entry asked for; applied after drawing, when the tree is not borrowed.
enum Cmd {
    Select(NodeId),
    Toggle(NodeId),
    ToggleSubtree(NodeId),
    Offset(isize),
    Page(bool),
    Left,
    Right,
    SubtreeOpen(bool),
    EditBody,
    Delete,
    NewRoot,
    NewChild,
    ExpandAll(bool),
    ExpandSubtree(NodeId, bool),
}

fn with_alpha(c: Color32, a: f32) -> Color32 {
    c.gamma_multiply(a)
}

impl Host {
    pub(crate) fn tree_pane(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut cmds: Vec<Cmd> = Vec::new();

        // The command strip: quiet, fixed commands (as the Qt footer).
        egui::Panel::bottom("tree-footer").show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .small_button("+ note")
                    .on_hover_text("New note   Ctrl+N")
                    .clicked()
                {
                    cmds.push(Cmd::NewRoot);
                }
                let child = ui
                    .add_enabled(
                        self.selected.is_some(),
                        egui::Button::new("+ child").small(),
                    )
                    .on_hover_text("New child note   Ctrl+Shift+N");
                if child.clicked() {
                    cmds.push(Cmd::NewChild);
                }
            });
        });

        if self.tree.rows().is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(24.0);
                ui.weak("No notes yet");
                if ui.small_button("+ create the first note").clicked() {
                    cmds.push(Cmd::NewRoot);
                }
            });
        } else {
            self.tree_rows(ui, &mut cmds);
        }
        self.apply(&ctx, cmds);
    }

    fn tree_rows(&mut self, ui: &mut egui::Ui, cmds: &mut Vec<Cmd>) {
        let ctx = ui.ctx().clone();
        let tree_id = Host::tree_id();
        let n = self.tree.len_visible();
        let visuals = ui.visuals().clone();
        let text_color = visuals.text_color();
        let muted = visuals.weak_text_color();
        let accent = visuals.hyperlink_color;
        let guide = visuals.widgets.noninteractive.bg_stroke.color;
        let selection = visuals.selection.bg_fill;
        let has_focus = ctx.memory(|m| m.has_focus(tree_id));
        let font = FontId::proportional(14.5);
        let bold = self.bold.clone().map(|f| FontId::new(14.5, f));

        // Escape closes a menu that is open (egui's own handling leaves the keyboard-opened one).
        if egui::Popup::is_any_open(&ctx)
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape))
        {
            egui::Popup::close_all(&ctx);
        }
        if has_focus && !self.dialog_open() && !egui::Popup::is_any_open(&ctx) {
            self.tree_keys(&ctx, cmds);
        }
        let want_menu = std::mem::take(&mut self.menu_request);
        let scroll_to = self
            .scroll_to
            .take()
            .and_then(|id| self.tree.visible_position(id));
        let focus_now = self.wants_focus(Focus::Tree);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_viewport(ui, |ui, viewport| {
                self.page_rows = (viewport.height() / ROW_H).floor().max(1.0) as usize;
                let width = ui.available_width().max(60.0);
                let (area, _) = ui.allocate_exact_size(
                    Vec2::new(width, (n as f32 * ROW_H).max(viewport.height())),
                    Sense::hover(),
                );
                // The focusable surface, below the rows; empty space shows the tree-wide menu.
                let surface = ui.interact(area, tree_id, Sense::click());
                if surface.clicked() {
                    ctx.memory_mut(|m| m.request_focus(tree_id));
                }
                if focus_now {
                    ctx.memory_mut(|m| m.request_focus(tree_id));
                }
                if ctx.memory(|m| m.has_focus(tree_id)) {
                    self.focus_reached(Focus::Tree);
                }
                if has_focus {
                    ctx.memory_mut(|m| {
                        m.set_focus_lock_filter(
                            tree_id,
                            egui::EventFilter {
                                horizontal_arrows: true,
                                vertical_arrows: true,
                                tab: false,
                                escape: true,
                            },
                        );
                    });
                }
                surface.context_menu(|ui| {
                    menu_entries(ui, None, false, cmds);
                });

                let first = (viewport.min.y / ROW_H).floor().max(0.0) as usize;
                let last = ((viewport.max.y / ROW_H).ceil() as usize + 1).min(n);
                let painter = ui.painter_at(ui.clip_rect());
                for i in first..last {
                    let r = self.tree.visible_row(i).clone();
                    let rect = Rect::from_min_size(
                        area.min + Vec2::new(0.0, i as f32 * ROW_H),
                        Vec2::new(width, ROW_H),
                    );
                    let current = self.selected == Some(r.id);
                    let open =
                        r.has_children && self.doc.as_ref().is_some_and(|d| d.is_expanded(r.id));
                    let resp = ui.interact(
                        rect,
                        egui::Id::unique(("omatree-row", r.id.get())),
                        Sense::click(),
                    );
                    let marker_left = rect.left() + LEFT + r.depth as f32 * INDENT;
                    let marker = Rect::from_min_size(
                        Pos2::new(marker_left, rect.top()),
                        Vec2::new(INDENT, ROW_H),
                    );

                    if current {
                        painter.rect_filled(rect, 0.0, with_alpha(selection, 0.55));
                        let bar = if has_focus { accent } else { guide };
                        painter.rect_filled(
                            Rect::from_min_size(rect.min, Vec2::new(3.0, ROW_H)),
                            0.0,
                            bar,
                        );
                    } else if resp.hovered() {
                        painter.rect_filled(rect, 0.0, with_alpha(visuals.faint_bg_color, 0.8));
                    }

                    // Guides: continuing ancestor lines, then this note's elbow.
                    if r.depth > 0 {
                        let line = if current {
                            with_alpha(accent, 0.5)
                        } else {
                            guide
                        };
                        let stroke = Stroke::new(1.0, line);
                        let col_x = |level: usize| {
                            (rect.left() + LEFT + level as f32 * INDENT + (INDENT / 2.0).floor())
                                .round()
                                + 0.5
                        };
                        for level in 0..r.depth.saturating_sub(1) {
                            if (r.guides >> level) & 1 == 1 {
                                let x = col_x(level);
                                painter.line_segment(
                                    [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                                    stroke,
                                );
                            }
                        }
                        let x = col_x(r.depth - 1);
                        let mid = rect.center().y.floor() + 0.5;
                        let bottom = if r.last { mid } else { rect.bottom() };
                        painter
                            .line_segment([Pos2::new(x, rect.top()), Pos2::new(x, bottom)], stroke);
                        let to = if r.has_children {
                            x + (INDENT / 2.0).floor() - 1.0
                        } else {
                            x + INDENT + 4.0
                        };
                        painter.line_segment([Pos2::new(x, mid), Pos2::new(to, mid)], stroke);
                    }

                    // Disclosure marker: closed branches hide something, so they are brighter.
                    if r.has_children {
                        let c = marker.center();
                        let colour = if current {
                            accent
                        } else if open {
                            muted
                        } else {
                            text_color
                        };
                        let pts = if open {
                            vec![
                                Pos2::new(c.x - 4.0, c.y - 2.0),
                                Pos2::new(c.x + 4.0, c.y - 2.0),
                                Pos2::new(c.x, c.y + 3.5),
                            ]
                        } else {
                            vec![
                                Pos2::new(c.x - 2.5, c.y - 4.0),
                                Pos2::new(c.x - 2.5, c.y + 4.0),
                                Pos2::new(c.x + 3.0, c.y),
                            ]
                        };
                        painter.add(Shape::convex_polygon(pts, colour, Stroke::NONE));
                    }

                    // Title, one line, ellipsis when too long.
                    let text_left = marker_left + INDENT + 4.0;
                    let mut job = egui::text::LayoutJob::single_section(
                        r.title.clone(),
                        egui::TextFormat::simple(
                            if current {
                                bold.clone().unwrap_or_else(|| font.clone())
                            } else {
                                font.clone()
                            },
                            text_color,
                        ),
                    );
                    job.wrap = egui::text::TextWrapping {
                        max_width: (rect.right() - text_left - 6.0).max(10.0),
                        max_rows: 1,
                        break_anywhere: true,
                        overflow_character: Some('\u{2026}'),
                    };
                    let galley = painter.layout_job(job);
                    painter.galley(
                        Pos2::new(text_left, rect.center().y - galley.size().y / 2.0),
                        galley,
                        text_color,
                    );

                    if resp.clicked() || resp.secondary_clicked() {
                        ctx.memory_mut(|m| m.request_focus(tree_id));
                        cmds.push(Cmd::Select(r.id));
                        if resp.clicked() && r.has_children {
                            let on_marker = resp
                                .interact_pointer_pos()
                                .is_some_and(|p| marker.expand2(Vec2::new(2.0, 0.0)).contains(p));
                            if on_marker {
                                if ctx.input(|i| i.modifiers.command) {
                                    cmds.push(Cmd::ToggleSubtree(r.id));
                                } else {
                                    cmds.push(Cmd::Toggle(r.id));
                                }
                            }
                        }
                    }
                    // The menu of this note (right-click, or Shift+F10 / the Menu key).
                    let mut popup = egui::Popup::context_menu(&resp);
                    if current {
                        let open =
                            egui::Popup::is_id_open(&ctx, egui::Popup::default_response_id(&resp));
                        if want_menu {
                            self.menu_pos =
                                Some(Pos2::new(rect.left() + 3.0 * INDENT, rect.bottom()));
                            popup = popup.open_memory(Some(egui::SetOpenCommand::Bool(true)));
                        }
                        // A menu opened from the keyboard has no pointer to anchor to: keep it
                        // under its row for as long as it is open.
                        match self.menu_pos {
                            Some(pos) if open || want_menu => popup = popup.at_position(pos),
                            _ => self.menu_pos = None,
                        }
                    }
                    popup.show(|ui| menu_entries(ui, Some(r.id), r.has_children, cmds));
                }
                if let Some(i) = scroll_to {
                    let rect = Rect::from_min_size(
                        area.min + Vec2::new(0.0, i as f32 * ROW_H),
                        Vec2::new(width, ROW_H),
                    );
                    ui.scroll_to_rect(rect, None);
                }
            });
    }

    /// Keys that act on the tree while it has the keyboard (the same set as the Qt TreeView and
    /// `TreePane.qml`; verified against the running Qt app).
    fn tree_keys(&mut self, ctx: &egui::Context, cmds: &mut Vec<Cmd>) {
        let none = egui::Modifiers::NONE;
        let ctrl = egui::Modifiers::COMMAND;
        ctx.input_mut(|i| {
            if i.consume_key(none, Key::ArrowDown) {
                cmds.push(Cmd::Offset(1));
            }
            if i.consume_key(none, Key::ArrowUp) {
                cmds.push(Cmd::Offset(-1));
            }
            if i.consume_key(none, Key::PageDown) {
                cmds.push(Cmd::Page(true));
            }
            if i.consume_key(none, Key::PageUp) {
                cmds.push(Cmd::Page(false));
            }
            // Ctrl variants first: egui ignores... only extra Shift, but be explicit.
            if i.consume_key(ctrl, Key::ArrowRight) {
                cmds.push(Cmd::SubtreeOpen(true));
            } else if i.consume_key(ctrl, Key::ArrowLeft) {
                cmds.push(Cmd::SubtreeOpen(false));
            } else if i.consume_key(none, Key::ArrowRight) {
                cmds.push(Cmd::Right);
            } else if i.consume_key(none, Key::ArrowLeft) {
                cmds.push(Cmd::Left);
            }
            if i.consume_key(none, Key::Enter) {
                cmds.push(Cmd::EditBody);
            }
            if i.consume_key(none, Key::Delete) {
                cmds.push(Cmd::Delete);
            }
        });
        let menu = ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, Key::F10));
        if menu && self.selected.is_some() {
            self.menu_request = true;
        }
    }

    fn apply(&mut self, ctx: &egui::Context, cmds: Vec<Cmd>) {
        for c in cmds {
            match c {
                Cmd::Select(id) => {
                    self.select(ctx, id);
                }
                Cmd::Toggle(id) => self.toggle_expanded(ctx, id),
                Cmd::ToggleSubtree(id) => {
                    let open = self.doc.as_ref().is_some_and(|d| d.is_expanded(id));
                    self.set_subtree_expanded(ctx, id, !open);
                }
                Cmd::Offset(d) => self.select_offset(ctx, d),
                Cmd::Page(down) => self.select_page(ctx, down),
                Cmd::Left => {
                    if let Some(id) = self.selected {
                        let open = self.doc.as_ref().is_some_and(|d| d.is_expanded(id));
                        if open {
                            self.set_expanded(ctx, id, false);
                        }
                    }
                }
                Cmd::Right => {
                    if let Some(id) = self.selected {
                        let open = self.doc.as_ref().is_some_and(|d| d.is_expanded(id));
                        if !open {
                            self.set_expanded(ctx, id, true);
                        }
                    }
                }
                Cmd::SubtreeOpen(open) => {
                    if let Some(id) = self.selected {
                        self.set_subtree_expanded(ctx, id, open);
                    }
                }
                Cmd::EditBody => {
                    if self.selected.is_some() {
                        self.set_focus(Focus::Editor);
                    }
                }
                Cmd::Delete => self.ask_delete(ctx),
                Cmd::NewRoot => self.create_root(ctx),
                Cmd::NewChild => self.create_child(ctx),
                Cmd::ExpandAll(open) => self.set_all_expanded(ctx, open),
                Cmd::ExpandSubtree(id, open) => self.set_subtree_expanded(ctx, id, open),
            }
        }
    }

    /// The editable title above the note. It is written to the notebook only when committed:
    /// Enter (then on to the body), Escape (then back to the tree), leaving the field, or any
    /// action that leaves the note.
    pub(crate) fn title_field(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let id = self.title_id();
        let take_focus = self.wants_focus(Focus::Title);
        if take_focus {
            // Focus first, then select the whole title (as the Qt app does for F2 and for a new
            // note): egui collapses a range selection in a frame where the field is not focused.
            ctx.memory_mut(|m| m.request_focus(id));
            let len = self.title_text.chars().count();
            let mut st = egui::TextEdit::load_state(&ctx, id).unwrap_or_default();
            st.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(len),
            )));
            st.store(&ctx, id);
        }
        let mut resp = None;
        let enabled = !self.dialog_open();
        ui.add_space(4.0);
        ui.add_enabled_ui(enabled, |ui| {
            resp = Some(
                ui.add(
                    egui::TextEdit::singleline(&mut self.title_text)
                        .id(id)
                        .frame(egui::Frame::NONE)
                        .font(FontId::proportional(22.0))
                        .hint_text("Title")
                        .desired_width(f32::INFINITY)
                        .margin(egui::Margin::symmetric(16, 2)),
                ),
            );
        });
        ui.add_space(4.0);
        let Some(resp) = resp else { return };
        if resp.has_focus() {
            self.focus_reached(Focus::Title);
        }
        if resp.lost_focus() && !self.dialog_open() {
            let (enter, escape) =
                ctx.input(|i| (i.key_pressed(Key::Enter), i.key_pressed(Key::Escape)));
            if self.commit_title(&ctx) {
                if enter {
                    self.set_focus(Focus::Editor);
                } else if escape {
                    self.set_focus(Focus::Tree);
                }
            }
        }
    }
}

fn menu_entries(ui: &mut egui::Ui, on: Option<NodeId>, branch: bool, cmds: &mut Vec<Cmd>) {
    let mut pick = |ui: &mut egui::Ui, label: &str, hint: &str, enabled: bool, cmd: Cmd| {
        let r = ui.add_enabled(enabled, egui::Button::new(label).shortcut_text(hint));
        if r.clicked() {
            cmds.push(cmd);
            ui.close();
        }
    };
    pick(ui, "New note", "Ctrl+N", true, Cmd::NewRoot);
    pick(ui, "New child", "Ctrl+Shift+N", on.is_some(), Cmd::NewChild);
    ui.separator();
    let id = on.unwrap_or(NodeId::from_raw(0));
    pick(
        ui,
        "Expand subtree",
        "Ctrl+Right",
        branch,
        Cmd::ExpandSubtree(id, true),
    );
    pick(
        ui,
        "Collapse subtree",
        "Ctrl+Left",
        branch,
        Cmd::ExpandSubtree(id, false),
    );
    pick(ui, "Expand all", "", true, Cmd::ExpandAll(true));
    pick(ui, "Collapse all", "", true, Cmd::ExpandAll(false));
    ui.separator();
    pick(ui, "Move to Trash", "Delete", on.is_some(), Cmd::Delete);
}
