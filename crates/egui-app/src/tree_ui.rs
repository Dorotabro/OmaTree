//! Drawing and keyboard handling of the tree pane and the editable title.
//!
//! The tree is one focusable widget (`Host::tree_id`); rows are painted by hand and only the rows
//! in the viewport are touched, so a frame costs the same for 50 notes or 50,000. Keys act only
//! while the tree has the keyboard, which keeps them away from text being edited; the commands
//! that are application-wide (new note, rename, move) are handled in `Host::handle_shortcuts`.

use super::drag::{self, Drag, Kind, Target, Verdict};
use super::*;
use egui::{Color32, FontId, Key, Pos2, Rect, Sense, Shape, Stroke, Vec2};

use super::layout::{
    BAR_H, GUTTER, INDENT, PAD, ROW_H, TITLE_NOTE_H, TITLE_PX, TITLE_TOP, TREE_LEFT as LEFT,
    TREE_TOP,
};
/// A pointer resting this long on a collapsed note while dragging opens it.
const HOVER_OPEN: f64 = 0.6;
/// Dragging within this distance of the top or bottom of the tree scrolls it, up to `SCROLL_MAX`.
const SCROLL_EDGE: f32 = 28.0;
const SCROLL_MAX: f32 = 1500.0;
/// How far the pointer must move from where it was pressed for a press to become a drag.
const DRAG_START: f32 = 6.0;

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
    /// The drag ended (dropped, cancelled or lost): close what hovering opened.
    EndDrag(Vec<NodeId>),
    DragOpen(NodeId),
    Drop(NodeId, Option<NodeId>, usize),
}

fn with_alpha(c: Color32, a: f32) -> Color32 {
    c.gamma_multiply(a)
}

impl Host {
    pub(crate) fn tree_pane(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut cmds: Vec<Cmd> = Vec::new();

        // The command strip: quiet, fixed commands (as the Qt footer). Flat text actions that only
        // show a fill under the pointer or the keyboard, on the same bar height as the status bar.
        let chrome = *self.theme.chrome();
        let bar = egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(PAD as i8 - 2, 0))
            .fill(theme::colour(self.theme.palette().surface));
        egui::Panel::bottom("tree-footer")
            .frame(bar)
            .exact_size(BAR_H)
            .show_separator_line(false)
            .show(ui, |ui| {
                paint_hairline(ui, chrome.hairline, PAD - 2.0);
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    if flat_action(ui, "+ note", true, &chrome)
                        .on_hover_text("New note   Ctrl+N")
                        .clicked()
                    {
                        cmds.push(Cmd::NewRoot);
                    }
                    if flat_action(ui, "+ child", self.selected.is_some(), &chrome)
                        .on_hover_text("New child note   Ctrl+Shift+N")
                        .clicked()
                    {
                        cmds.push(Cmd::NewChild);
                    }
                });
            });

        if self.tree.rows().is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(24.0);
                ui.weak("No notes yet");
                if flat_action(ui, "+ create the first note", true, &chrome).clicked() {
                    cmds.push(Cmd::NewRoot);
                }
            });
        } else {
            ui.add_space(TREE_TOP);
            self.tree_rows(ui, &mut cmds);
        }
        self.apply(&ctx, cmds);
    }

    fn tree_rows(&mut self, ui: &mut egui::Ui, cmds: &mut Vec<Cmd>) {
        let ctx = ui.ctx().clone();
        let tree_id = Host::tree_id();
        let n = self.tree.len_visible();
        // Every colour here is derived from the one palette (see `theme::Chrome`).
        let ch = *self.theme.chrome();
        let (text_color, muted, accent, guide) = (ch.text, ch.muted, ch.accent, ch.guide);
        let danger = ch.danger_text;
        let has_focus = ctx.memory(|m| m.has_focus(tree_id));
        let font = FontId::proportional(14.5);
        let bold = self.bold.clone().map(|f| FontId::new(14.5, f));

        // Escape closes a menu that is open (egui's own handling leaves the keyboard-opened one).
        if egui::Popup::is_any_open(&ctx)
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape))
        {
            egui::Popup::close_all(&ctx);
        }
        if has_focus
            && !self.dialog_open()
            && !egui::Popup::is_any_open(&ctx)
            && self.drag.is_none()
        {
            self.tree_keys(&ctx, cmds);
        }
        let want_menu = std::mem::take(&mut self.menu_request);
        let scroll_to = self
            .scroll_to
            .take()
            .and_then(|id| self.tree.visible_position(id));
        let focus_now = self.wants_focus(Focus::Tree);

        let view = ui.available_rect_before_wrap();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_viewport(ui, |ui, viewport| {
                self.page_rows = (viewport.height() / ROW_H).floor().max(1.0) as usize;
                let width = ui.available_width().max(60.0);
                let (area, _) = ui.allocate_exact_size(
                    Vec2::new(width, (n as f32 * ROW_H).max(viewport.height())),
                    Sense::hover(),
                );
                self.tree_geom = (area.min, view);
                self.drag_frame(ui, &ctx, view, area.min, cmds);
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
                    menu_entries(ui, None, false, &ch, cmds);
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
                    let text_color = if current {
                        ch.selected_text
                    } else {
                        text_color
                    };
                    let text_color = if self.drag.as_ref().is_some_and(|d| d.id == r.id) {
                        with_alpha(text_color, 0.5)
                    } else {
                        text_color
                    };
                    let open =
                        r.has_children && self.doc.as_ref().is_some_and(|d| d.is_expanded(r.id));
                    let resp = ui.interact(
                        rect,
                        ui.make_persistent_id(("omatree-row", r.id.get())),
                        Sense::click_and_drag(),
                    );
                    // Only a drag that began on a row (past egui's click distance) is a tree drag.
                    // (A press held in place also counts as a drag to egui; it is not one here.)
                    let moved = ctx.input(|i| {
                        i.pointer
                            .press_origin()
                            .zip(i.pointer.latest_pos())
                            .is_some_and(|(a, b)| a.distance(b) >= DRAG_START)
                    });
                    if resp.drag_started_by(egui::PointerButton::Primary)
                        && moved
                        && self.drag.is_none()
                        && !self.dialog_open()
                    {
                        self.drag = Some(Drag::new(&self.tree, r.id));
                        ctx.memory_mut(|m| m.request_focus(tree_id));
                    }
                    let marker_left = rect.left() + LEFT + r.depth as f32 * INDENT;
                    let marker = Rect::from_min_size(
                        Pos2::new(marker_left, rect.top()),
                        Vec2::new(INDENT, ROW_H),
                    );

                    if current {
                        let (fill, bar) = if has_focus {
                            (ch.selected_focus, accent)
                        } else {
                            (ch.selected_blur, muted)
                        };
                        painter.rect_filled(rect, 0.0, fill);
                        painter.rect_filled(
                            Rect::from_min_size(rect.min, Vec2::new(2.0, ROW_H)),
                            0.0,
                            bar,
                        );
                    } else if resp.hovered() {
                        painter.rect_filled(rect, 0.0, ch.hover_fill);
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
                    if galley.elided && resp.hovered() && self.drag.is_none() {
                        resp.show_tooltip_text(&r.title);
                    }
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
                    popup.show(|ui| menu_entries(ui, Some(r.id), r.has_children, &ch, cmds));
                }
                if let Some(t) = self.drag.as_ref().and_then(|d| d.target) {
                    let accent_line = |y: f32, colour: Color32| {
                        painter.rect_filled(
                            Rect::from_min_size(
                                Pos2::new(area.left(), y - 1.0),
                                Vec2::new(width, 2.0),
                            ),
                            0.0,
                            colour,
                        );
                    };
                    let row_top = area.min.y + t.row as f32 * ROW_H;
                    let outline = |colour: Color32| {
                        painter.rect_stroke(
                            Rect::from_min_size(
                                Pos2::new(area.left(), row_top),
                                Vec2::new(width, ROW_H),
                            )
                            .shrink(1.0),
                            0.0,
                            Stroke::new(2.0, colour),
                            egui::StrokeKind::Inside,
                        );
                    };
                    match (t.verdict, t.kind) {
                        // Nothing would change: nothing is shown.
                        (Verdict::NoOp, _) => {}
                        (Verdict::Refused, Kind::End) => accent_line(row_top, danger),
                        (Verdict::Refused, _) => outline(danger),
                        (Verdict::Move { .. }, Kind::Child) => outline(accent),
                        (Verdict::Move { .. }, Kind::Before) => accent_line(row_top, accent),
                        (Verdict::Move { .. }, Kind::After) => {
                            accent_line(row_top + ROW_H, accent);
                        }
                        (Verdict::Move { .. }, Kind::End) => accent_line(row_top, accent),
                    }
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

    /// One frame of a drag: cancel on Escape, follow the pointer to a target, scroll near the
    /// edges, open a collapsed note the pointer rests on, and finish when the button is released.
    /// Nothing here touches the notebook; structural changes are returned as commands.
    fn drag_frame(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        view: Rect,
        origin: Pos2,
        cmds: &mut Vec<Cmd>,
    ) {
        let Some(mut drag) = self.drag.take() else {
            return;
        };
        let (released, down, pos, now, dt, escape) = ctx.input_mut(|i| {
            (
                i.pointer.primary_released(),
                i.pointer.primary_down(),
                i.pointer.latest_pos(),
                i.time,
                i.unstable_dt.min(0.1),
                i.consume_key(egui::Modifiers::NONE, Key::Escape),
            )
        });
        if escape {
            drag.cancelled = true;
        }
        if drag.cancelled {
            drag.target = None;
            drag.hover = None;
        } else {
            let pos = pos.filter(|p| view.contains(*p));
            drag.target = match (pos, self.doc.as_ref()) {
                (Some(p), Some(doc)) => {
                    drag::locate(&self.tree, doc, drag.id, p.y - origin.y, ROW_H, drag.target)
                }
                _ => None,
            };
            if let Some(p) = pos {
                let speed =
                    drag::scroll_speed(p.y, view.top(), view.bottom(), SCROLL_EDGE, SCROLL_MAX);
                if speed != 0.0 {
                    ui.scroll_with_delta(Vec2::new(0.0, -speed * dt));
                }
            }
            // A collapsed note the pointer rests on (its middle zone) opens after a pause.
            let resting = drag.target.and_then(|t| match (t.kind, t.over) {
                (Kind::Child, Some(o))
                    if !drag.subtree.contains(&o)
                        && self.tree.row(o).is_some_and(|r| r.has_children)
                        && !self.doc.as_ref().is_some_and(|d| d.is_expanded(o)) =>
                {
                    Some(o)
                }
                _ => None,
            });
            drag.hover = match (resting, drag.hover) {
                (Some(o), Some((h, since))) if o == h => {
                    if now - since >= HOVER_OPEN {
                        cmds.push(Cmd::DragOpen(o));
                        None
                    } else {
                        Some((h, since))
                    }
                }
                (Some(o), _) => Some((o, now)),
                (None, _) => None,
            };
        }
        ctx.request_repaint();
        let refused = drag.target.is_some_and(|t| t.verdict == Verdict::Refused);
        ctx.set_cursor_icon(if refused {
            egui::CursorIcon::NotAllowed
        } else {
            egui::CursorIcon::Grabbing
        });
        if released || !down {
            let landing = drag.target.filter(|_| released && !drag.cancelled);
            cmds.push(Cmd::EndDrag(std::mem::take(&mut drag.opened)));
            if let Some(Target {
                verdict: Verdict::Move { parent, position },
                ..
            }) = landing
            {
                cmds.push(Cmd::Drop(drag.id, parent, position));
            }
            return;
        }
        self.drag = Some(drag);
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
                Cmd::EndDrag(opened) => self.end_drag(opened),
                Cmd::DragOpen(id) => {
                    self.set_expanded(ctx, id, true);
                    if let Some(d) = self.drag.as_mut() {
                        d.opened.push(id);
                    }
                }
                Cmd::Drop(id, parent, position) => self.drop_note(ctx, id, parent, position),
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
        let ch = *self.theme.chrome();
        let font = self.bold.clone().map_or_else(
            || FontId::proportional(TITLE_PX),
            |f| FontId::new(TITLE_PX, f),
        );
        let mut resp = None;
        let enabled = !self.dialog_open();
        ui.add_space(TITLE_TOP);
        ui.add_enabled_ui(enabled, |ui| {
            // The inset is a frame around the field: a text edit without its own frame ignores
            // its margin, and this is what puts the first letter on the editor's axis.
            egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(GUTTER as i8, 0))
                .show(ui, |ui| {
                    resp = Some(
                        ui.add(
                            egui::TextEdit::singleline(&mut self.title_text)
                                .id(id)
                                .frame(egui::Frame::NONE)
                                .font(font)
                                .text_color(ch.text)
                                .hint_text(egui::RichText::new("Title").color(ch.muted))
                                .desired_width(f32::INFINITY)
                                .margin(egui::Margin::ZERO),
                        ),
                    );
                });
        });
        // One line for the validation message of this title, then the rule: the message stays
        // beside the field it is about, and the editor below never moves when it appears.
        let (note, note_resp) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), TITLE_NOTE_H),
            Sense::hover(),
        );
        let Some(resp) = resp else { return };
        let invalid = self
            .title_error
            .as_deref()
            .filter(|_| self.title_text.trim() != self.loaded_title);
        if let Some(msg) = invalid {
            // One line, clipped to its row: the whole message is in the tooltip below.
            let painter = ui.painter_at(note);
            let galley =
                painter.layout_no_wrap(msg.to_owned(), FontId::proportional(12.5), ch.danger_text);
            painter.galley(
                Pos2::new(note.left() + GUTTER, note.top()),
                galley,
                ch.danger_text,
            );
            note_resp.on_hover_text(msg);
        }
        let rule = if invalid.is_some() {
            ch.danger_text
        } else if resp.has_focus() {
            ch.accent
        } else {
            ch.hairline
        };
        paint_inset_rule(ui, note.bottom(), rule);
        if resp.changed() {
            self.title_error = None;
        }
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

/// A flat command in a bar: text only, a fill under the pointer or keyboard, never a button frame.
/// Disabled, it is plain muted text that takes neither clicks nor focus.
pub(crate) fn flat_action(
    ui: &mut egui::Ui,
    label: &str,
    enabled: bool,
    ch: &theme::Chrome,
) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        FontId::proportional(12.5),
        Color32::PLACEHOLDER,
    );
    let size = Vec2::new(galley.size().x + 16.0, BAR_H - 8.0);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, r) = ui.allocate_exact_size(size, sense);
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
    let colour = if !enabled {
        ch.muted
    } else if r.hovered() || r.has_focus() {
        ch.text
    } else {
        ch.secondary
    };
    if enabled && (r.hovered() || r.has_focus()) {
        ui.painter().rect_filled(rect, 4.0, ch.hover_fill);
    }
    if enabled && r.has_focus() {
        ui.painter().rect_stroke(
            rect,
            4.0,
            Stroke::new(1.0, ch.accent),
            egui::StrokeKind::Inside,
        );
    }
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, colour);
    r
}

/// A one-pixel line along the top of a bar, `bleed` wider than its content on both sides so it
/// runs the full width of the panel (the bars inset their content, the line is not inset).
pub(crate) fn paint_hairline(ui: &egui::Ui, colour: Color32, bleed: f32) {
    let r = ui.max_rect().expand2(Vec2::new(bleed, 0.0));
    ui.painter()
        .hline(r.x_range(), r.top() + 0.5, Stroke::new(1.0, colour));
}

/// A one-pixel rule at `y`, inset by the gutter on both sides (the title's axis).
fn paint_inset_rule(ui: &egui::Ui, y: f32, colour: Color32) {
    let r = ui.max_rect();
    ui.painter().hline(
        (r.left() + GUTTER)..=(r.right() - GUTTER),
        (y - 1.0).floor() + 0.5,
        Stroke::new(1.0, colour),
    );
}

fn menu_entries(
    ui: &mut egui::Ui,
    on: Option<NodeId>,
    branch: bool,
    ch: &theme::Chrome,
    cmds: &mut Vec<Cmd>,
) {
    ui.set_min_width(220.0);
    ui.spacing_mut().button_padding = Vec2::new(10.0, 4.0);
    ui.spacing_mut().item_spacing.y = 2.0;
    let danger = ch.danger_text;
    let mut pick = |ui: &mut egui::Ui, label: &str, hint: &str, enabled: bool, cmd: Cmd| {
        let mut text = egui::RichText::new(label);
        if matches!(cmd, Cmd::Delete) {
            text = text.color(danger);
        }
        let r = ui.add_enabled(enabled, egui::Button::new(text).shortcut_text(hint));
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
