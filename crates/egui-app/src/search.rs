//! Notebook-wide Search in the navigation pane.
//!
//! The searching is `omatree_core::search::search` over the in-memory `Document` (so unsaved
//! edits are found); this module only asks, keeps the answer, draws it and reports which note
//! was chosen. Results are a throwaway list: they are replaced whenever the query or the
//! notebook's content changes and are never trusted when one is activated (the note is looked
//! up again by its `NodeId`).
//!
//! Invalidation: a changed (trimmed) query searches at once; a structural change (create, rename,
//! move, delete, open) marks the results stale for the next frame; a body edit marks them stale
//! for a short debounce. Nothing is scanned on a frame where none of these happened.

use super::layout::{
    BAR_H, GUTTER, PAD, RESULT_PAD_Y, RESULT_SUB_H, RESULT_SUB_PX, RESULT_TITLE_H, RESULT_TITLE_PX,
    SEARCH_DEBOUNCE, SLOW_SCAN,
};
use super::*;
use egui::{FontId, Pos2, Rect, Sense, Stroke, Vec2};
use omatree_core::search::{self, SearchResult};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Stale {
    #[default]
    No,
    /// A body was edited: refresh after the debounce.
    Soon,
    /// The structure or a title changed: refresh on the next frame.
    Now,
}

#[derive(Default)]
pub(crate) struct Search {
    /// The search field is showing.
    pub(crate) open: bool,
    pub(crate) query: String,
    results: Vec<SearchResult>,
    /// Top edge of every result, and the total height as the last entry.
    tops: Vec<f32>,
    current: usize,
    /// The trimmed query `results` answers.
    answered: String,
    stale: Stale,
    deadline: Option<f64>,
    /// The query the pending deadline was started for (typing restarts the wait).
    deadline_query: String,
    /// How long the last scan took, in seconds. A notebook that is slow to scan waits for a
    /// pause in the typing before searching; a fast one searches on every key.
    pub(crate) last_scan: f64,
    /// How many scans of the notebook were made (tests assert none are made when idle).
    scans: u64,
    /// The field has had the keyboard since it opened (an empty field closes when it leaves).
    seen_focus: bool,
    scroll_to_current: bool,
    /// The next time the field is given the keyboard, leave its text alone (not select it all).
    keep_query: bool,
}

impl Search {
    /// A body was edited.
    pub(crate) fn body_changed(&mut self) {
        if self.stale == Stale::No {
            self.stale = Stale::Soon;
        }
    }

    /// The tree's structure or a title changed, or a notebook was opened.
    pub(crate) fn structure_changed(&mut self) {
        self.stale = Stale::Now;
    }

    fn clear_results(&mut self) {
        self.results.clear();
        self.tops.clear();
        self.answered.clear();
        self.current = 0;
        self.stale = Stale::No;
        self.deadline = None;
    }

    /// Whether a scan is expensive enough that typing should be waited out.
    fn slow(&self) -> bool {
        self.last_scan > SLOW_SCAN
    }

    fn step(&mut self, delta: isize) {
        if self.results.is_empty() {
            return;
        }
        let last = self.results.len() - 1;
        self.current = (self.current as isize + delta).clamp(0, last as isize) as usize;
        self.scroll_to_current = true;
    }

    fn showing_results(&self) -> bool {
        self.open && !self.query.trim().is_empty()
    }

    fn result_height(r: &SearchResult) -> f32 {
        let lines = if r.breadcrumb.is_empty() { 1.0 } else { 2.0 };
        RESULT_PAD_Y * 2.0 + RESULT_TITLE_H + RESULT_SUB_H * lines
    }
}

impl Host {
    pub(crate) fn search_id() -> egui::Id {
        egui::Id::unique("omatree-search")
    }

    /// Ctrl+F: show the field, with the last query selected, and give it the keyboard.
    pub fn start_search(&mut self) {
        if self.doc.is_none() {
            return;
        }
        self.search.open = true;
        self.search.seen_focus = false;
        self.search.keep_query = false;
        self.set_focus(Focus::Search);
    }

    /// Dismiss Search: the field goes, the query and every result with it. Touches no selection.
    pub fn stop_search(&mut self) {
        self.search.open = false;
        self.search.query.clear();
        self.search.clear_results();
    }

    pub fn search_open(&self) -> bool {
        self.search.open
    }

    pub fn search_query(&self) -> &str {
        &self.search.query
    }

    /// The results as last computed (empty while the query is empty).
    pub fn search_results(&self) -> &[SearchResult] {
        &self.search.results
    }

    /// The highlighted result, if there are any.
    pub fn search_current(&self) -> Option<usize> {
        (!self.search.results.is_empty()).then_some(self.search.current)
    }

    /// How many times the notebook has been scanned for a query.
    pub fn search_scans(&self) -> u64 {
        self.search.scans
    }

    /// Choose result `index`: reveal its note in the tree, show it, and give the tree the
    /// keyboard. The note is looked up again now; a result that has gone stale selects nothing and
    /// refreshes the list. A pending title the notebook refuses keeps Search open and the
    /// selection where it is.
    pub fn activate_result(&mut self, ctx: &egui::Context, index: usize) {
        let Some(id) = self.search.results.get(index).map(|r| r.id) else {
            return;
        };
        let alive = self.tree.contains(id)
            && self
                .doc
                .as_ref()
                .is_some_and(|d| d.notebook().get(id).is_some());
        if !alive {
            self.search.structure_changed();
            return;
        }
        if self.select(ctx, id) {
            self.stop_search();
            self.reveal(id);
            self.set_focus(Focus::Tree);
        }
    }

    /// Bring the results up to date with the query and the notebook (see the module comment).
    fn search_refresh(&mut self, now: f64, ctx: &egui::Context, force: bool) {
        if !self.search.showing_results() {
            if !self.search.results.is_empty() || self.search.stale != Stale::No {
                self.search.clear_results();
            }
            return;
        }
        let fresh = self.search.query.trim() != self.search.answered;
        // Wait: a body edit always waits a moment; a new query does only when scanning is slow
        // (each key then restarts the wait, as in the Qt app). Choosing a result never waits.
        let wait = if fresh {
            self.search.slow()
        } else {
            match self.search.stale {
                Stale::No => return,
                Stale::Now => false,
                Stale::Soon => true,
            }
        };
        let query = self.search.query.trim().to_owned();
        if wait && !force {
            let s = &mut self.search;
            let restart = fresh && s.deadline_query != query;
            if restart || s.deadline.is_none() {
                s.deadline = Some(now + SEARCH_DEBOUNCE);
                s.deadline_query.clone_from(&query);
            }
            let due = s.deadline.unwrap_or(now);
            if now < due {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(due - now + 0.01));
                return;
            }
        }
        let Some(doc) = self.doc.as_ref() else { return };
        let started = std::time::Instant::now();
        let results = search::search(doc.notebook(), &query);
        let took = started.elapsed().as_secs_f64();
        let s = &mut self.search;
        s.scans += 1;
        s.last_scan = took;
        // The highlight stays on the same note when it is still a result; otherwise it keeps its
        // place in the list, and a new query starts at the top.
        let keep = (!fresh)
            .then(|| s.results.get(s.current).map(|r| r.id))
            .flatten();
        s.current = match keep.and_then(|id| results.iter().position(|r| r.id == id)) {
            Some(i) => i,
            None if fresh => 0,
            None => s.current.min(results.len().saturating_sub(1)),
        };
        let mut tops = Vec::with_capacity(results.len() + 1);
        let mut y = 0.0;
        for r in &results {
            tops.push(y);
            y += Search::result_height(r);
        }
        tops.push(y);
        s.tops = tops;
        s.answered = query;
        s.results = results;
        s.stale = Stale::No;
        s.deadline = None;
        s.scroll_to_current = true;
    }

    /// The field at the top of the pane (only while Search is open). Up and Down move the
    /// highlight, Enter chooses, Escape dismisses; Tab and everything else is left alone.
    pub(crate) fn search_header(&mut self, ui: &mut egui::Ui, ch: &theme::Chrome) {
        let ctx = ui.ctx().clone();
        let id = Host::search_id();
        // The tree is not showing, so a request for it (Escape from the editor, a closed
        // dialog) goes to the results' own field instead.
        if self.focus == Some(Focus::Tree) && self.search.showing_results() {
            self.search.keep_query = true;
            self.set_focus(Focus::Search);
        }
        let frame = egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(PAD as i8 + 2, 0))
            .fill(theme::colour(self.theme.palette().surface));
        let mut leave_to_tree = false;
        let mut chosen = false;
        egui::Panel::top("search-bar")
            .frame(frame)
            .exact_size(BAR_H)
            .show_separator_line(false)
            .show(ui, |ui| {
                if self.wants_focus(Focus::Search) {
                    ctx.memory_mut(|m| m.request_focus(id));
                    if !std::mem::take(&mut self.search.keep_query) {
                        let len = self.search.query.chars().count();
                        let mut st = egui::TextEdit::load_state(&ctx, id).unwrap_or_default();
                        st.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                            egui::text::CCursor::new(0),
                            egui::text::CCursor::new(len),
                        )));
                        st.store(&ctx, id);
                    }
                }
                let focused = ctx.memory(|m| m.has_focus(id));
                if focused && !self.dialog_open() {
                    let none = egui::Modifiers::NONE;
                    let (down, up, enter, escape) = ctx.input_mut(|i| {
                        (
                            i.consume_key(none, Key::ArrowDown),
                            i.consume_key(none, Key::ArrowUp),
                            i.consume_key(none, Key::Enter),
                            i.consume_key(none, Key::Escape),
                        )
                    });
                    if down {
                        self.search.step(1);
                    }
                    if up {
                        self.search.step(-1);
                    }
                    chosen = enter;
                    if escape {
                        leave_to_tree = true;
                    }
                }
                let enabled = !self.dialog_open();
                ui.add_enabled_ui(enabled, |ui| {
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let prompt = if focused { ch.accent } else { ch.muted };
                        ui.label(
                            egui::RichText::new("/")
                                .monospace()
                                .size(15.0)
                                .color(prompt),
                        );
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut self.search.query)
                                .id(id)
                                .frame(egui::Frame::NONE)
                                .font(FontId::proportional(14.5))
                                .text_color(ch.text)
                                .hint_text(egui::RichText::new("search notes").color(ch.muted))
                                .desired_width(f32::INFINITY)
                                .margin(egui::Margin::ZERO),
                        );
                        if resp.has_focus() {
                            // Up, Down and Escape are this field's: egui must not use them to
                            // move the keyboard to another widget.
                            ctx.memory_mut(|m| {
                                m.set_focus_lock_filter(
                                    id,
                                    egui::EventFilter {
                                        horizontal_arrows: true,
                                        vertical_arrows: true,
                                        tab: false,
                                        escape: true,
                                    },
                                );
                            });
                            self.focus_reached(Focus::Search);
                            self.search.seen_focus = true;
                        }
                    });
                });
                // The rule under the field, quietly accent while it has the keyboard.
                let rule = if focused { ch.accent } else { ch.hairline };
                let r = ui.max_rect().expand2(Vec2::new(PAD + 2.0, 0.0));
                ui.painter()
                    .hline(r.x_range(), r.bottom() - 0.5, Stroke::new(1.0, rule));
            });
        if chosen && !leave_to_tree {
            // What was typed this frame counts: look again before choosing.
            let now = ctx.input(|i| i.time);
            self.search_refresh(now, &ctx, true);
            if !self.search.results.is_empty() {
                let at = self.search.current;
                self.activate_result(&ctx, at);
            }
        }
        if leave_to_tree {
            self.stop_search();
            self.set_focus(Focus::Tree);
        } else if self.search.open
            && self.search.seen_focus
            && !ctx.memory(|m| m.has_focus(id))
            && self.focus != Some(Focus::Search)
            && self.search.query.trim().is_empty()
            && !self.dialog_open()
        {
            // The field was left with nothing typed: it is not a toolbar, so it goes.
            self.search.open = false;
        }
    }

    /// The results, in place of the tree. A click calls `activate_result`.
    pub(crate) fn search_results_ui(&mut self, ui: &mut egui::Ui, ch: &theme::Chrome) {
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        self.search_refresh(now, &ctx, false);
        if self.search.results.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(24.0);
                ui.label(egui::RichText::new("No matching notes").color(ch.muted));
            });
            return;
        }
        let field_focused = ctx.memory(|m| m.has_focus(Host::search_id()));
        let want_visible = std::mem::take(&mut self.search.scroll_to_current);
        let mut clicked = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_viewport(ui, |ui, viewport| {
                let total = *self.search.tops.last().unwrap_or(&0.0);
                let width = ui.available_width().max(60.0);
                let (area, _) = ui.allocate_exact_size(Vec2::new(width, total), Sense::hover());
                let tops = &self.search.tops;
                let first = tops
                    .partition_point(|t| *t <= viewport.min.y)
                    .saturating_sub(1);
                let painter = ui.painter_at(ui.clip_rect());
                let text_left = area.left() + GUTTER;
                let right = area.right() - GUTTER + 4.0;
                let single = |text: &str, size: f32, colour: egui::Color32, family| {
                    let mut job = egui::text::LayoutJob::single_section(
                        text.to_owned(),
                        egui::TextFormat::simple(FontId::new(size, family), colour),
                    );
                    job.wrap = egui::text::TextWrapping {
                        max_width: (right - text_left).max(10.0),
                        max_rows: 1,
                        break_anywhere: true,
                        overflow_character: Some('\u{2026}'),
                    };
                    painter.layout_job(job)
                };
                let proportional = egui::FontFamily::Proportional;
                for i in first..self.search.results.len() {
                    let top = area.top() + tops[i];
                    if top > area.top() + viewport.max.y {
                        break;
                    }
                    let r = &self.search.results[i];
                    let height = tops[i + 1] - tops[i];
                    let rect =
                        Rect::from_min_size(Pos2::new(area.left(), top), Vec2::new(width, height));
                    let current = i == self.search.current;
                    let resp = ui.interact(
                        rect,
                        ui.make_persistent_id(("omatree-result", i)),
                        Sense::CLICK,
                    );
                    resp.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::Role::ListItem, true, r.title.as_str())
                    });
                    if current {
                        let (fill, bar) = if field_focused {
                            (ch.selected_focus, ch.accent)
                        } else {
                            (ch.selected_blur, ch.muted)
                        };
                        painter.rect_filled(rect, 0.0, fill);
                        painter.rect_filled(
                            Rect::from_min_size(rect.min, Vec2::new(2.0, height)),
                            0.0,
                            bar,
                        );
                    } else if resp.hovered() {
                        painter.rect_filled(rect, 0.0, ch.hover_fill);
                    }
                    let title_colour = if current { ch.selected_text } else { ch.text };
                    let family = if current {
                        self.bold.clone().unwrap_or_else(|| proportional.clone())
                    } else {
                        proportional.clone()
                    };
                    let mut y = rect.top() + RESULT_PAD_Y;
                    let galley = single(&r.title, RESULT_TITLE_PX, title_colour, family);
                    let elided = galley.elided;
                    painter.galley(
                        Pos2::new(text_left, y + (RESULT_TITLE_H - galley.size().y) / 2.0),
                        galley,
                        title_colour,
                    );
                    y += RESULT_TITLE_H;
                    if !r.breadcrumb.is_empty() {
                        // A path that does not fit loses its outermost notes first: the nearest
                        // ancestors are what tell two notes of the same name apart.
                        let mut g =
                            single(&r.breadcrumb, RESULT_SUB_PX, ch.muted, proportional.clone());
                        if g.elided {
                            let parts: Vec<&str> = r.breadcrumb.split(" \u{203a} ").collect();
                            for skip in 1..parts.len() {
                                let shorter = format!(
                                    "\u{2026} \u{203a} {}",
                                    parts[skip..].join(" \u{203a} ")
                                );
                                g = single(&shorter, RESULT_SUB_PX, ch.muted, proportional.clone());
                                if !g.elided {
                                    break;
                                }
                            }
                        }
                        painter.galley(
                            Pos2::new(text_left, y + (RESULT_SUB_H - g.size().y) / 2.0),
                            g,
                            ch.muted,
                        );
                        y += RESULT_SUB_H;
                    }
                    if !r.snippet.is_empty() {
                        let g = single(
                            &r.snippet,
                            RESULT_SUB_PX,
                            ch.secondary,
                            proportional.clone(),
                        );
                        painter.galley(
                            Pos2::new(text_left, y + (RESULT_SUB_H - g.size().y) / 2.0),
                            g,
                            ch.secondary,
                        );
                    }
                    if elided && resp.hovered() {
                        resp.show_tooltip_text(&r.title);
                    }
                    if resp.clicked() {
                        clicked = Some(i);
                    }
                }
                if want_visible && self.search.current < self.search.results.len() {
                    let at = self.search.current;
                    let rect = Rect::from_min_size(
                        Pos2::new(area.left(), area.top() + self.search.tops[at]),
                        Vec2::new(width, self.search.tops[at + 1] - self.search.tops[at]),
                    );
                    ui.scroll_to_rect(rect, None);
                }
            });
        if let Some(i) = clicked {
            self.search.current = i;
            self.activate_result(&ctx, i);
        }
    }

    pub(crate) fn search_showing_results(&self) -> bool {
        self.search.showing_results()
    }
}
