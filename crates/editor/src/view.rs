//! [`EditorView`]: the embeddable editor component. See the crate docs for the host/editor split.

use crate::assets;
use crate::attachments::AttachmentStore;
use crate::edit::{self, Edit};
use crate::emoji;
use crate::geom::{Geo, Offsets};
use crate::image_size;
use crate::keys::{Action, EditorKeys};
use crate::layout::{self, ImageSlot};
use crate::links;
use crate::markdown::{self, Span};
use crate::table;
use crate::theme::Palette;
use egui::{
    text::CCursor, text::CCursorRange, Color32, Context, FontFamily, Id, Key, Modifiers, Pos2,
    Rect, TextureHandle, Vec2,
};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

// Test-only timing of the per-frame stages (`prof!("name", expr)`); compiled out of real builds.
#[cfg(test)]
thread_local! {
    pub(crate) static PROF: RefCell<HashMap<&'static str, (u128, u32)>> = RefCell::new(HashMap::new());
}
#[cfg(test)]
macro_rules! prof {
    ($name:expr, $e:expr) => {{
        let t = std::time::Instant::now();
        let r = $e;
        PROF.with(|p| {
            let mut p = p.borrow_mut();
            let e = p.entry($name).or_insert((0, 0));
            e.0 += t.elapsed().as_nanos();
            e.1 += 1;
        });
        r
    }};
}
#[cfg(not(test))]
macro_rules! prof {
    ($name:expr, $e:expr) => {
        $e
    };
}

/// Identity of the note an editor is showing: the notebook it lives in and the note within it.
///
/// Everything egui remembers for the editor (the `TextEdit`'s caret, selection and undo history,
/// popups, task checkboxes, image controls) is keyed from [`NoteKey::widget_id`], and the image
/// texture cache is scoped by `notebook`, so two notes (or two notebooks) never share state by
/// accident. The host chooses the numbers; they only have to be stable and unique per note.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NoteKey {
    pub notebook: u64,
    pub note: u64,
}

impl NoteKey {
    /// The single document of the standalone editor.
    pub const STANDALONE: NoteKey = NoteKey::new(0, 0);

    pub const fn new(notebook: u64, note: u64) -> Self {
        NoteKey { notebook, note }
    }

    /// The egui id of this note's `TextEdit`; every other widget id derives from it.
    pub fn widget_id(self) -> Id {
        Id::unique(("omatree-editor", self.notebook, self.note))
    }
}

/// Whether the editor should take keyboard focus this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FocusRequest {
    #[default]
    None,
    /// Focus the text surface (for example after a note was selected).
    Request,
}

/// Everything the host lends to the editor for one frame.
pub struct EditorInput<'a> {
    /// Which note this is (see [`NoteKey`]).
    pub key: NoteKey,
    /// The Markdown source: the one authoritative text. The editor edits it in place.
    pub text: &'a mut String,
    /// Where images come from. The editor never touches files itself.
    pub attachments: &'a dyn AttachmentStore,
    /// Semantic colours. A different palette between frames restyles the editor.
    pub palette: &'a Palette,
    /// Shortcuts the editor may consume.
    pub keys: &'a EditorKeys,
    /// The bold font family if the host installed one (see [`crate::fonts::install_bold`]).
    pub bold: Option<&'a FontFamily>,
    pub focus: FocusRequest,
}

/// What happened during a frame.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EditorOutput {
    /// The text differs from what it was at the start of the frame (typing, a command, a paste, a
    /// checkbox or image-width edit). The editor never saves; the host decides.
    pub changed: bool,
    /// Selection as UTF-8 byte offsets (anchor, caret).
    pub selection: (usize, usize),
    pub has_focus: bool,
    /// Escape was pressed while the editor had focus and nothing inside the editor used it (no popup
    /// or image selection was open). Escape in a text edit has already surrendered focus.
    pub escape: bool,
    /// The configured export shortcut was pressed ([`EditorKeys::export`]).
    pub export_requested: bool,
    /// A one-line notice for a status bar ("Pasted 3x4 image", "Opening ...", a failure).
    pub status: Option<String>,
}

const IMAGE_PAD: f32 = image_size::PAD;
/// Horizontal inset of the document area, applied by the container (never by source whitespace).
const DOC_PAD: f32 = 16.0;
const BASE_SIZE: f32 = 16.0;

/// Table layouts keyed by (table source, wrap width bits, pixels-per-point bits).
type TableCache = HashMap<(String, u32, u32), Rc<table::TableLayout>>;
/// Full table layouts kept (only the tables near the viewport are drawn).
const MAX_LAYOUTS: usize = 256;

/// Table heights, which the text layout needs for every table in the note on every frame (they set
/// the height of the rows reserved for the rendered tables). Keyed by a hash of the table source,
/// the wrap width, the scale and whether the bold family is in use; the entry keeps the source so a
/// hash collision is detected rather than trusted. Cleared when the fonts change, bounded in size.
type HeightCache = HashMap<(u64, u32, u32, bool), (Box<str>, f32)>;
const MAX_HEIGHTS: usize = 8192;

fn table_height(
    heights: &RefCell<HeightCache>,
    layouts: &RefCell<TableCache>,
    ctx: &Context,
    src: &str,
    t: &markdown::Table,
    wrap: f32,
    bold: Option<&FontFamily>,
) -> f32 {
    use std::hash::{Hash, Hasher};
    let body = &src[t.source_range()];
    let mut h = std::collections::hash_map::DefaultHasher::new();
    body.hash(&mut h);
    let key = (
        h.finish(),
        wrap.to_bits(),
        ctx.pixels_per_point().to_bits(),
        bold.is_some(),
    );
    if let Some((text, height)) = heights.borrow().get(&key) {
        if &**text == body {
            return *height;
        }
    }
    let height = table_layout(layouts, ctx, src, t, wrap, bold).height;
    let mut c = heights.borrow_mut();
    if c.len() >= MAX_HEIGHTS {
        c.clear();
    }
    c.insert(key, (body.into(), height));
    height
}

/// Widths of fixed probe strings in the fonts the tables use. Any change of font definitions or
/// metrics changes them, which is how cached table measurements notice (egui offers no event).
fn font_probe(ctx: &Context, bold: Option<&FontFamily>) -> [u32; 3] {
    ctx.fonts_mut(|f| {
        let mut w = |family: FontFamily| {
            f.layout_no_wrap(
                "Příliš žluťoučký Hgxy 🦀".to_owned(),
                egui::FontId::new(BASE_SIZE, family),
                Color32::WHITE,
            )
            .size()
        };
        let p = w(FontFamily::Proportional);
        let m = w(FontFamily::Monospace);
        let b = w(bold.cloned().unwrap_or(FontFamily::Proportional));
        [
            (p.x + 1000.0 * p.y).to_bits(),
            (m.x + 1000.0 * m.y).to_bits(),
            (b.x + 1000.0 * b.y).to_bits(),
        ]
    })
}

fn table_layout(
    cache: &RefCell<TableCache>,
    ctx: &Context,
    src: &str,
    t: &markdown::Table,
    wrap: f32,
    bold: Option<&FontFamily>,
) -> Rc<table::TableLayout> {
    let r = t.source_range();
    let key = (
        src[r].to_owned(),
        wrap.to_bits(),
        ctx.pixels_per_point().to_bits(),
    );
    let mut c = cache.borrow_mut();
    if c.len() > MAX_LAYOUTS && !c.contains_key(&key) {
        c.clear();
    }
    c.entry(key)
        .or_insert_with(|| Rc::new(table::layout(ctx, src, t, wrap, BASE_SIZE, bold)))
        .clone()
}

struct Slash {
    trigger: usize,
    selected: usize,
    picker: Option<Picker>,
}

/// Emoji picker state; the document is untouched until an emoji is chosen.
struct Picker {
    /// End of the typed `/emoji` text when the picker opened (replaced on insert).
    caret: usize,
    query: String,
    selected: usize,
    pos: Pos2,
    focus: bool,
    moved: bool,
    /// Search results for `hits_for`; recomputed only when the query changes.
    hits: Vec<emoji::Emoji>,
    hits_for: Option<String>,
    scroll: f32,
}

impl Picker {
    fn new(caret: usize, pos: Pos2) -> Self {
        Picker {
            caret,
            query: String::new(),
            selected: 0,
            pos,
            focus: true,
            moved: false,
            hits: Vec::new(),
            hits_for: None,
            scroll: 0.0,
        }
    }

    fn refresh(&mut self) {
        if self.hits_for.as_deref() != Some(self.query.as_str()) {
            self.hits = emoji::search(&self.query);
            self.hits_for = Some(self.query.clone());
        }
    }
}

const EMOJI_COLS: usize = 8;
const EMOJI_CELL: f32 = 28.0;
const EMOJI_VIEW_H: f32 = 170.0;
/// Explicit popup widths. An egui `Area` remembers its last size and uses it as the next frame's
/// maximum, so content-sized popups that once shrank (e.g. a one-item filtered menu) stay narrow.
const SLASH_WIDTH: f32 = 170.0;
const EMOJI_WIDTH: f32 = EMOJI_COLS as f32 * 34.0;
struct Tex {
    handle: TextureHandle,
    size: Vec2,
}

/// Decoded images, keyed by (notebook, reference): a reference only means something within its
/// notebook. A missing or undecodable image is remembered as `None`.
type Textures = HashMap<(u64, String), Option<Tex>>;

/// The interactive state and render caches of one editor. Create one per editor on screen and keep
/// it between frames. It holds no document text: that is lent by the host every frame.
///
/// State that belongs to a note (slash menu, emoji picker, image selection, a pending width) is reset
/// whenever [`EditorInput::key`] differs from the previous frame's. Caret, selection and undo
/// history live in egui memory under [`NoteKey::widget_id`], so they survive switching away and
/// back until [`EditorView::forget_note`].
pub struct EditorView {
    /// The note the transient state below belongs to.
    scope: Option<NoteKey>,
    slash: Option<Slash>,
    caret_pos: Pos2,
    slash_rect: Rect,
    /// Link under a Ctrl/Cmd+press, opened when the button comes up without a drag.
    link_press: Option<String>,
    /// Image (byte offset of its line) whose width controls are shown.
    selected_image: Option<usize>,
    /// Width being dragged/typed on the slider, not yet written to the source: (image, percent).
    /// The layout uses it, so the reserved row height follows the slider immediately.
    live_width: Cell<Option<(usize, u16)>>,
    /// The pending width came from the keyboard: it is committed when the slider loses focus.
    slider_keys: bool,
    slider_id: Option<Id>,
    /// Screen rects of the controls and the selected image, for dismissal on an outside press.
    controls_rect: Rect,
    // Render caches. Interior mutability: the `TextEdit` layouter only gets shared access.
    spans: RefCell<(String, Rc<Vec<Span>>, Rc<Offsets>)>,
    textures: RefCell<Textures>,
    images: RefCell<Vec<ImageSlot>>,
    tables: RefCell<TableCache>,
    heights: RefCell<HeightCache>,
    /// Font probe the table caches were built under (see `font_probe`).
    fonts: Option<[u32; 3]>,
    /// Selection (byte range) the last layout was built with.
    layout_sel: RefCell<(usize, usize)>,
    /// The palette the caches were built under.
    palette: Option<Palette>,
    /// Offer `/strike` (from the host's key configuration, refreshed every frame).
    strike_command: bool,
    /// This frame is the first one for the current note after a switch (or after `forget_note`).
    entered: bool,
    /// The text edit had focus at the end of the previous frame. egui drops focus on Escape before
    /// the frame starts, so this is how Escape-while-focused is recognised.
    was_focused: bool,
    // Per-frame results.
    changed: bool,
    export_requested: bool,
    status: Option<String>,
    /// Test probes: where the last frame put the text and the task checkboxes (screen space).
    #[cfg(test)]
    probe: Probe,
}

#[cfg(test)]
#[derive(Default)]
struct Probe {
    galley: Option<(std::sync::Arc<egui::Galley>, Pos2)>,
    task_rects: Vec<(usize, Rect)>,
    image_rects: Vec<(usize, Rect)>,
    /// Slider, Reset and Source rects of the width controls.
    controls: Vec<(&'static str, Rect)>,
    /// Start offsets of the tables painted as grids in the last frame.
    tables: Vec<usize>,
}

impl Default for EditorView {
    fn default() -> Self {
        Self::new()
    }
}
fn image_height(
    textures: &RefCell<Textures>,
    notebook: u64,
    store: &dyn AttachmentStore,
    ctx: &Context,
    url: &str,
    wrap_width: f32,
    width: Option<u16>,
) -> Option<f32> {
    let mut cache = textures.borrow_mut();
    let entry = cache.entry((notebook, url.to_owned())).or_insert_with(|| {
        // Decoded once per reference; a missing or undecodable image is remembered as such.
        let img = assets::decode(&store.get(url).ok()?).ok()?;
        let size = Vec2::new(img.size[0] as f32, img.size[1] as f32);
        let handle = ctx.load_texture(format!("img:{url}"), img, egui::TextureOptions::LINEAR);
        Some(Tex { handle, size })
    });
    let tex = entry.as_ref()?;
    Some(image_size::display(tex.size, wrap_width, width).y + IMAGE_PAD)
}

/// Top and bottom y (galley space) of the rows covering `r`.
fn rows_y(geo: &Geo, r: &std::ops::Range<usize>) -> (f32, f32) {
    let end = r.start + geo.text[r.clone()].trim_end_matches(['\n', '\r']).len();
    (geo.pos(r.start).min.y, geo.pos(end).max.y)
}

/// The vertical span of the document that is on screen (plus a margin), in galley space.
fn view_span(ui: &egui::Ui, out: &egui::text_edit::TextEditOutput) -> (f32, f32) {
    const MARGIN: f32 = 300.0;
    let clip = ui.clip_rect();
    (
        clip.min.y - out.galley_pos.y - MARGIN,
        clip.max.y - out.galley_pos.y + MARGIN,
    )
}

/// Rule lines, blockquote borders and code surfaces for the current layout (colour/geometry only).
fn decor_shapes(
    decor: &[layout::Decor],
    geo: &Geo,
    origin: Pos2,
    width: f32,
    pal: &Palette,
) -> Vec<egui::Shape> {
    let text = geo.text;
    let valid = |r: &std::ops::Range<usize>| {
        r.start <= r.end
            && r.end <= text.len()
            && text.is_char_boundary(r.start)
            && text.is_char_boundary(r.end)
    };
    let mut out = Vec::new();
    for d in decor {
        match d {
            layout::Decor::Rule { at }
                if *at < text.len()
                    && text.is_char_boundary(*at)
                    && geo.is_visible(&(*at..*at + 1)) =>
            {
                let r = geo.pos(*at);
                let y = origin.y + r.center().y;
                out.push(egui::Shape::line_segment(
                    [Pos2::new(origin.x, y), Pos2::new(origin.x + width, y)],
                    egui::Stroke::new(1.5, pal.rule),
                ));
            }
            layout::Decor::Quote { range, depth } if valid(range) && geo.is_visible(range) => {
                let (top, bottom) = rows_y(geo, range);
                let x = origin.x - 7.0 - 5.0 * f32::from(*depth);
                let rect = Rect::from_min_max(
                    Pos2::new(x, origin.y + top),
                    Pos2::new(x + 3.0, origin.y + bottom),
                );
                out.push(egui::Shape::rect_filled(rect, 1.5, pal.quote_border));
            }
            layout::Decor::CodeBlock { range } if valid(range) && geo.is_visible(range) => {
                let (top, bottom) = rows_y(geo, range);
                let rect = Rect::from_min_max(
                    Pos2::new(origin.x - 6.0, origin.y + top - 1.0),
                    Pos2::new(origin.x + width + 6.0, origin.y + bottom + 1.0),
                );
                out.push(egui::Shape::rect_filled(rect, 4.0, pal.code_block_bg));
            }
            _ => {}
        }
    }
    out
}

impl EditorView {
    pub fn new() -> Self {
        EditorView {
            scope: None,
            slash: None,
            caret_pos: Pos2::ZERO,
            slash_rect: Rect::NOTHING,
            link_press: None,
            selected_image: None,
            live_width: Cell::new(None),
            slider_keys: false,
            slider_id: None,
            controls_rect: Rect::NOTHING,
            spans: RefCell::new((
                String::new(),
                Rc::new(Vec::new()),
                Rc::new(Offsets::build("")),
            )),
            textures: RefCell::new(HashMap::new()),
            images: RefCell::new(Vec::new()),
            tables: RefCell::new(HashMap::new()),
            heights: RefCell::new(HashMap::new()),
            fonts: None,
            layout_sel: RefCell::new((0, 0)),
            palette: None,
            strike_command: false,
            was_focused: false,
            entered: false,
            changed: false,
            export_requested: false,
            status: None,
            #[cfg(test)]
            probe: Probe::default(),
        }
    }

    /// Selection of `key`'s text edit as UTF-8 byte offsets (anchor, caret).
    pub fn selection(ctx: &Context, key: NoteKey, text: &str) -> (usize, usize) {
        Self::selection_bytes(ctx, key.widget_id(), text)
    }

    /// Whether `key`'s text edit has keyboard focus.
    pub fn has_focus(ctx: &Context, key: NoteKey) -> bool {
        ctx.memory(|m| m.has_focus(key.widget_id()))
    }

    /// Write a pending image width (a slider drag or key adjustment still in progress) into `text`
    /// as one undo step. Hosts call this before saving or switching notes. Does nothing for a note
    /// the view is not showing. Returns whether `text` changed.
    pub fn commit_pending(&mut self, ctx: &Context, key: NoteKey, text: &mut String) -> bool {
        if self.scope != Some(key) {
            return false;
        }
        self.changed = false;
        self.commit_width(ctx, key.widget_id(), text);
        std::mem::take(&mut self.changed)
    }

    /// Drop everything remembered for a note that was deleted or whose text was replaced behind the
    /// editor's back: its caret, selection and undo history (egui memory) and, if it is the note on
    /// screen, the slash menu, emoji picker, image selection and any pending width. Image
    /// textures are kept; they belong to the notebook (see [`EditorView::forget_images`]).
    pub fn forget_note(&mut self, ctx: &Context, key: NoteKey) {
        let id = key.widget_id();
        ctx.data_mut(|d| d.remove::<egui::text_edit::TextEditState>(id));
        ctx.memory_mut(|m| {
            if m.has_focus(id) {
                m.surrender_focus(id);
            }
        });
        if self.scope == Some(key) {
            self.reset_transient();
            self.scope = None;
        }
    }

    /// Drop the decoded images of a notebook (it was closed, or an attachment was replaced).
    pub fn forget_images(&mut self, notebook: u64) {
        self.textures
            .borrow_mut()
            .retain(|(nb, _), _| *nb != notebook);
    }

    fn reset_transient(&mut self) {
        self.slash = None;
        self.link_press = None;
        self.selected_image = None;
        self.live_width.set(None);
        self.slider_keys = false;
        self.slider_id = None;
        self.controls_rect = Rect::NOTHING;
        self.slash_rect = Rect::NOTHING;
    }

    /// Switch the transient state to `key`'s note.
    fn enter(&mut self, key: NoteKey) {
        if self.scope == Some(key) {
            return;
        }
        self.reset_transient();
        self.was_focused = false;
        self.entered = true;
        // Textures of other notebooks are not needed any more; bound the cache.
        self.textures
            .borrow_mut()
            .retain(|(nb, _), _| *nb == key.notebook);
        self.scope = Some(key);
    }

    fn selection_bytes(ctx: &Context, id: Id, text: &str) -> (usize, usize) {
        let Some(range) = egui::TextEdit::load_state(ctx, id).and_then(|s| s.cursor.char_range())
        else {
            return (0, 0);
        };
        let (a, b) = (range.primary.index.0, range.secondary.index.0);
        let (a, b) = (edit::char_to_byte(text, a), edit::char_to_byte(text, b));
        (a, b)
    }

    fn paste_image(
        &mut self,
        ctx: &Context,
        id: Id,
        img: &egui::ColorImage,
        text: &mut String,
        store: &dyn AttachmentStore,
    ) {
        // Store first; the document only gains a reference once the attachment exists.
        let stored = assets::encode_png(img)
            .and_then(|png| store.put(&png, "png").map_err(|e| e.to_string()));
        match stored {
            Ok(url) => {
                let (a, b) = Self::selection_bytes(ctx, id, text);
                let at = a.max(b);
                let e = edit::insert_image_ref(text, at, &url);
                // Through `apply_edit`, like every other source edit, so the frame reports `changed`.
                self.apply_edit(ctx, id, &e, false, text);
                self.status = Some(format!(
                    "Pasted {}x{} image -> {url}",
                    img.size[0], img.size[1]
                ));
            }
            Err(e) => self.status = Some(format!("Image paste failed: {e}")),
        }
    }

    /// Keys for the open emoji picker; consumed before any widget (incl. the editor) sees them.
    fn emoji_keys(&mut self, ctx: &Context, id: Id, text: &mut String) {
        let Some(s) = self.slash.as_mut() else { return };
        let Some(p) = s.picker.as_mut() else { return };
        p.refresh();
        let n = p.hits.len();
        let (down, up, left, right, enter, esc) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::ArrowLeft),
                i.consume_key(Modifiers::NONE, Key::ArrowRight),
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        let last = n.saturating_sub(1);
        let old = p.selected;
        if down {
            p.selected = (p.selected + EMOJI_COLS).min(last);
        }
        if up {
            p.selected = p.selected.saturating_sub(EMOJI_COLS);
        }
        if right {
            p.selected = (p.selected + 1).min(last);
        }
        if left {
            p.selected = p.selected.saturating_sub(1);
        }
        p.moved |= p.selected != old;
        if esc {
            // Back to the slash menu; the document and caret were never touched.
            s.picker = None;
            ctx.memory_mut(|m| m.request_focus(id));
        } else if enter {
            if let Some(g) = p.hits.get(p.selected).map(|e| e.glyph) {
                self.insert_emoji(ctx, id, g, text);
            }
        }
    }

    fn insert_emoji(&mut self, ctx: &Context, id: Id, glyph: &str, text: &mut String) {
        let Some(s) = self.slash.take() else { return };
        let Some(p) = s.picker else { return };
        let e = edit::apply_emoji(s.trigger, p.caret, glyph);
        apply_edit_to(ctx, id, text, &e, false);
        ctx.memory_mut(|m| m.request_focus(id));
    }

    fn emoji_popup(&mut self, ctx: &Context, id: Id, text: &mut String) {
        let Some(p) = self.slash.as_mut().and_then(|s| s.picker.as_mut()) else {
            return;
        };
        let mut chosen: Option<&'static str> = None;
        egui::Area::new(id.with("emoji"))
            .order(egui::Order::Foreground)
            .fixed_pos(p.pos)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(EMOJI_WIDTH);
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut p.query)
                            .hint_text("Search emoji")
                            .desired_width(f32::INFINITY),
                    );
                    if p.focus {
                        r.request_focus();
                        p.focus = false;
                    }
                    if r.changed() {
                        p.selected = 0;
                        p.moved = true;
                    }
                    p.refresh();
                    let hits = std::mem::take(&mut p.hits);
                    p.selected = p.selected.min(hits.len().saturating_sub(1));
                    // Virtualised grid: the full catalogue can match thousands of emoji.
                    let row_h = EMOJI_CELL + ui.spacing().item_spacing.y;
                    let rows = hits.len().div_ceil(EMOJI_COLS);
                    let mut area = egui::ScrollArea::vertical().max_height(EMOJI_VIEW_H);
                    if p.moved {
                        let top = (p.selected / EMOJI_COLS) as f32 * row_h;
                        let view = EMOJI_VIEW_H.min(rows as f32 * row_h);
                        let off = if top < p.scroll {
                            top
                        } else if top + row_h > p.scroll + view {
                            top + row_h - view
                        } else {
                            p.scroll
                        };
                        area = area.vertical_scroll_offset(off);
                    }
                    let out = area.show_rows(ui, row_h, rows, |ui, range| {
                        for row in range {
                            ui.horizontal(|ui| {
                                for col in 0..EMOJI_COLS {
                                    let i = row * EMOJI_COLS + col;
                                    let Some(em) = hits.get(i) else { break };
                                    let b = ui.add_sized(
                                        [EMOJI_CELL, EMOJI_CELL],
                                        egui::Button::selectable(
                                            i == p.selected,
                                            egui::RichText::new(em.glyph).size(18.0),
                                        ),
                                    );
                                    if b.has_focus() {
                                        b.surrender_focus();
                                    }
                                    if b.clicked() {
                                        chosen = Some(em.glyph);
                                    }
                                    b.on_hover_text(em.name);
                                }
                            });
                        }
                        if hits.is_empty() {
                            ui.weak("No matches");
                        }
                    });
                    p.scroll = out.state.clamped_offset().y;
                    p.moved = false;
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(hits.get(p.selected).map_or("", |e| e.name)).weak(),
                        )
                        .truncate(),
                    );
                    p.hits = hits;
                });
            });
        if let Some(g) = chosen {
            self.insert_emoji(ctx, id, g, text);
        }
    }

    /// Run a slash command on the typed `/query` ending at `caret`.
    fn run_slash(
        &mut self,
        ctx: &Context,
        id: Id,
        cmd: &edit::SlashCommand,
        caret: usize,
        text: &mut String,
    ) {
        let Some(s) = self.slash.as_mut() else { return };
        match edit::apply_slash(text, s.trigger, caret, cmd) {
            None => s.picker = Some(Picker::new(caret, self.caret_pos)),
            Some(e) => {
                self.slash = None;
                self.apply_edit(ctx, id, &e, false, text);
                ctx.memory_mut(|m| m.request_focus(id));
            }
        }
    }

    /// Apply a source edit and set the caret/selection it asks for. `rev`: the caret end is the
    /// start of the selection (selection made right-to-left).
    fn apply_edit(&mut self, ctx: &Context, id: Id, e: &Edit, rev: bool, text: &mut String) {
        self.changed = true;
        apply_edit_to(ctx, id, text, e, rev);
    }

    /// Formatting shortcuts (as configured in `keys`) and list editing keys, consumed before the
    /// `TextEdit` sees them. Enter / Backspace inside list items are fixed (never in code blocks).
    fn editing_keys(
        &mut self,
        ctx: &Context,
        id: Id,
        text: &mut String,
        keys: &EditorKeys,
        strike: bool,
    ) {
        let (p, q) = Self::selection_bytes(ctx, id, text);
        let (a, b) = (p.min(q), p.max(q));
        let rev = p < q;
        let toggle = Self::toggle_task_key(ctx, text, a, keys);
        let pressed = keys.consume(
            ctx,
            &[
                Action::Bold,
                Action::Italic,
                Action::Code,
                Action::Bullets,
                Action::Numbers,
            ],
        );
        let has = |a: Action| pressed.contains(&a);
        let (bold, italic, code) = (has(Action::Bold), has(Action::Italic), has(Action::Code));
        let (bullets, numbers) = (has(Action::Bullets), has(Action::Numbers));
        let delim = [(bold, "**"), (italic, "*"), (code, "`"), (strike, "~~")]
            .into_iter()
            .find_map(|(on, d)| on.then_some(d));
        let e = if let Some(start) = toggle {
            // Ctrl+Enter on a task line: flip the box, keep the selection where it was.
            edit::toggle_task(text, start).map(|e| keep_selection(e, p, q))
        } else if let Some(d) = delim {
            Some(edit::format_inline(text, a, b, d))
        } else if bullets || numbers {
            Some(edit::list_selection(text, a, b, numbers))
        } else if a == b {
            let in_block = |at: usize| in_code_block(&self.spans.borrow().1, at);
            let enter = !in_block(a.saturating_sub(1))
                && edit::list_enter(text, a).is_some()
                && ctx.input(|i| i.key_pressed(Key::Enter) && i.modifiers.is_none());
            let back = !in_block(a)
                && edit::list_backspace(text, a).is_some()
                && ctx.input(|i| i.key_pressed(Key::Backspace) && i.modifiers.is_none());
            if enter && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
                edit::list_enter(text, a)
            } else if back && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Backspace)) {
                edit::list_backspace(text, a)
            } else {
                None
            }
        } else {
            None
        };
        if let Some(e) = e {
            self.apply_edit(ctx, id, &e, rev && toggle.is_none(), text);
        }
    }

    /// The task-toggle shortcut with the caret on a task item: consume it and return the box's start byte.
    fn toggle_task_key(ctx: &Context, text: &str, at: usize, keys: &EditorKeys) -> Option<usize> {
        if !keys.pressed(ctx, Action::ToggleTask) {
            return None;
        }
        let line = markdown::line_range(text, at.min(text.len()));
        let start = markdown::parse(text)
            .into_iter()
            .find_map(|sp| match sp.kind {
                markdown::Kind::Task { .. } if line.contains(&sp.range.start) => {
                    Some(sp.range.start)
                }
                _ => None,
            })?;
        keys.consume_one(ctx, Action::ToggleTask);
        Some(start)
    }

    /// Checkbox widgets over the (invisible) task boxes, link opening, and the matching pointer
    /// cursors. Runs after the `TextEdit`, so its click areas sit above the text.
    fn task_and_link_interaction(
        &mut self,
        ui: &egui::Ui,
        id: Id,
        out: &egui::text_edit::TextEditOutput,
        pal: &Palette,
        text: &mut String,
    ) {
        let ctx = ui.ctx();
        let (a, b) = *self.layout_sel.borrow();
        let (spans, offsets) = {
            let cache = self.spans.borrow();
            if cache.0 != *text {
                return;
            }
            (cache.1.clone(), cache.2.clone())
        };
        #[cfg(test)]
        {
            self.probe.galley = Some((out.galley.clone(), out.galley_pos));
            self.probe.task_rects.clear();
        }
        let geo = Geo::new(text, &offsets, &out.galley, view_span(ui, out));
        let boxes = task_boxes(&geo, &spans, &(a.min(b)..a.max(b)));
        for tb in boxes {
            let rect = tb.rect.translate(out.galley_pos.to_vec2());
            #[cfg(test)]
            self.probe.task_rects.push((tb.start, rect));
            paint_checkbox(ui.painter(), rect, tb.checked, pal);
            let r = ui.interact(
                rect.expand(2.0),
                id.with(("task", tb.start)),
                // Claim the drag as well, or the text edit underneath still starts a selection.
                egui::Sense::click_and_drag(),
            );
            if r.hovered() {
                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if r.is_pointer_button_down_on() || r.clicked() {
                // The press blurs the text edit; the editor keeps keyboard focus.
                ctx.memory_mut(|m| m.request_focus(id));
            }
            if r.clicked() {
                if let Some(e) = edit::toggle_task(text, tb.start) {
                    let (p, q) = Self::selection_bytes(ctx, id, text);
                    let e = keep_selection(e, p, q);
                    self.apply_edit(ctx, id, &e, false, text);
                }
            }
        }

        // Links: only Ctrl/Cmd+click opens one; a plain click just places the caret. The link is
        // looked up when the button goes down: the press moves the caret into the link, which
        // reveals its delimiters and shifts the text before the button comes up.
        let (mods, hover, pressed, clicked, released) = ctx.input(|i| {
            (
                i.modifiers,
                i.pointer.hover_pos(),
                i.pointer
                    .primary_pressed()
                    .then(|| i.pointer.interact_pos())
                    .flatten(),
                i.pointer
                    .primary_clicked()
                    .then(|| i.pointer.interact_pos())
                    .flatten(),
                i.pointer.primary_released(),
            )
        });
        let link_at = |pos: Pos2| {
            if !out.response.rect.contains(pos) {
                return None;
            }
            let rel = pos - out.galley_pos;
            let c = out.galley.cursor_from_pos(rel);
            // Past the end of a line the cursor clamps to the line end: that is not on the link.
            let on_row = out.galley.rows.iter().any(|row| {
                let r = row.rect();
                rel.y >= r.min.y && rel.y <= r.max.y && rel.x <= r.max.x
            });
            if !on_row {
                return None;
            }
            let byte = edit::char_to_byte(text, c.index.0);
            spans.iter().rev().find_map(|sp| match &sp.kind {
                markdown::Kind::Link { url } if sp.range.start <= byte && byte < sp.range.end => {
                    Some(url.clone())
                }
                _ => None,
            })
        };
        let open_mod = links::opens_link(mods);
        if open_mod && hover.and_then(link_at).is_some() {
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if let Some(pos) = pressed {
            self.link_press = open_mod.then(|| link_at(pos)).flatten();
        }
        let url = clicked.and_then(|pos| {
            self.link_press
                .take()
                .or_else(|| open_mod.then(|| link_at(pos)).flatten())
        });
        if released {
            self.link_press = None;
        }
        if let Some(url) = url {
            match links::safe_http_url(&url) {
                Some(u) => {
                    ctx.open_url(egui::OpenUrl::new_tab(u));
                    self.status = Some(format!("Opening {u}"));
                }
                None => {
                    self.status = Some(format!("Not opened (only http/https links open): {url}"))
                }
            }
        }
    }

    /// Annotated width of the image whose line starts at `start`, from a fresh parse.
    fn image_annotation(text: &str, start: usize) -> Option<(Option<u16>, std::ops::Range<usize>)> {
        markdown::parse(text)
            .into_iter()
            .find_map(|sp| match sp.kind {
                markdown::Kind::Image { width, ann, .. } if sp.range.start == start => {
                    Some((width, ann))
                }
                _ => None,
            })
    }

    /// Write `pct` (or no annotation) into the image line at `start` as one undoable source edit.
    /// The caret and selection keep their place in the text. No-op if nothing would change.
    fn set_image_width(
        &mut self,
        ctx: &Context,
        id: Id,
        text: &mut String,
        start: usize,
        pct: Option<u16>,
    ) {
        let Some((width, ann)) = Self::image_annotation(text, start) else {
            return;
        };
        if width == pct {
            return;
        }
        let mut e = edit::image_width_edit(ann, pct);
        let (p, q) = Self::selection_bytes(ctx, id, text);
        e.caret = edit::map_offset(&e, p);
        e.anchor = (p != q).then(|| edit::map_offset(&e, q));
        self.apply_edit(ctx, id, &e, false, text);
    }

    /// Write the slider's pending width to the source (one undo step), if there is one.
    fn commit_width(&mut self, ctx: &Context, id: Id, text: &mut String) {
        self.slider_keys = false;
        if let Some((start, pct)) = self.live_width.take() {
            self.set_image_width(ctx, id, text, start, Some(pct));
        }
    }

    /// Hide the width controls. `commit`: write a pending width first; otherwise drop it.
    fn dismiss_image(&mut self, ctx: &Context, id: Id, text: &mut String, commit: bool) {
        if commit {
            self.commit_width(ctx, id, text);
        } else {
            self.live_width.set(None);
        }
        self.selected_image = None;
        self.slider_keys = false;
        self.slider_id = None;
        self.controls_rect = Rect::NOTHING;
        ctx.memory_mut(|m| m.request_focus(id));
    }

    /// Escape / Enter while an image is selected: Escape dismisses (committing a pending width);
    /// Enter puts the caret into the image's source line, which reveals the raw Markdown.
    fn image_keys(&mut self, ctx: &Context, id: Id, text: &mut String) {
        let Some(start) = self.selected_image else {
            return;
        };
        // Undo/redo with the slider focused: finish the adjustment (it becomes the step to undo)
        // and let the editor handle the key.
        let slider_focused = self
            .slider_id
            .is_some_and(|sid| ctx.memory(|m| m.has_focus(sid)));
        if slider_focused
            && ctx
                .input(|i| i.modifiers.command && (i.key_pressed(Key::Z) || i.key_pressed(Key::Y)))
        {
            self.commit_width(ctx, id, text);
            ctx.memory_mut(|m| m.request_focus(id));
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.dismiss_image(ctx, id, text, true);
        } else if ctx.memory(|m| m.has_focus(id))
            && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter))
        {
            self.dismiss_image(ctx, id, text, true);
            Self::edit_image_source(ctx, id, text, start);
        }
    }

    fn edit_image_source(ctx: &Context, id: Id, text: &str, start: usize) {
        let at = (start + 2).min(text.len());
        let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
        let c = CCursor::new(edit::byte_to_char(text, at));
        state.cursor.set_char_range(Some(CCursorRange::one(c)));
        state.store(ctx, id);
        ctx.request_repaint();
    }

    /// Selection of rendered images (a click keeps the image visible and shows the width slider).
    /// The click areas sit above the `TextEdit`, so a click does not move the caret into the line.
    fn image_interaction(
        &mut self,
        ui: &egui::Ui,
        id: Id,
        out: &egui::text_edit::TextEditOutput,
        pal: &Palette,
        rects: &[(usize, Rect)],
        text: &mut String,
    ) {
        let ctx = ui.ctx();
        #[cfg(test)]
        {
            self.probe.image_rects = rects.to_vec();
        }
        if let Some(sel) = self.selected_image {
            // Typing, undo or a caret on the line (raw source shown) ends the selection.
            if out.response.changed() {
                self.dismiss_image(ctx, id, text, false);
            } else if !rects.iter().any(|(s, _)| *s == sel) {
                self.dismiss_image(ctx, id, text, true);
            }
        }
        let pressed = ctx.input(|i| {
            i.pointer
                .primary_pressed()
                .then(|| i.pointer.interact_pos())
                .flatten()
        });
        if let (Some(sel), Some(pos)) = (self.selected_image, pressed) {
            let on_image = rects.iter().any(|(s, r)| *s == sel && r.contains(pos));
            if !on_image && !self.controls_rect.contains(pos) {
                self.dismiss_image(ctx, id, text, true);
            }
        }
        for (start, rect) in rects {
            let r = ui.interact(
                *rect,
                id.with(("image", *start)),
                egui::Sense::click_and_drag(),
            );
            if r.hovered() {
                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if r.is_pointer_button_down_on() || r.clicked() {
                ctx.memory_mut(|m| m.request_focus(id));
            }
            if r.clicked() && self.selected_image != Some(*start) {
                self.commit_width(ctx, id, text);
                self.selected_image = Some(*start);
            }
        }
        let Some(sel) = self.selected_image else {
            return;
        };
        let Some(rect) = rects.iter().find(|(s, _)| *s == sel).map(|(_, r)| *r) else {
            return;
        };
        ui.painter().rect_stroke(
            rect.expand(2.0),
            2.0,
            egui::Stroke::new(2.0, pal.link),
            egui::StrokeKind::Outside,
        );
        self.image_controls(ctx, id, text, sel, rect, out.galley.job.wrap.max_width);
    }

    /// The width slider, Reset and Source buttons next to the selected image.
    fn image_controls(
        &mut self,
        ctx: &Context,
        id: Id,
        text: &mut String,
        sel: usize,
        rect: Rect,
        content_w: f32,
    ) {
        // The palette of this frame (set at the top of `show`).
        let pal = self.palette.unwrap_or_else(Palette::fallback);
        let Some((annotated, _)) = Self::image_annotation(text, sel) else {
            return;
        };
        let natural = {
            let url = self
                .images
                .borrow()
                .iter()
                .find_map(|s| (s.byte_start == sel).then(|| s.url.clone()));
            let t = self.textures.borrow();
            let nb = self.scope.map_or(0, |k| k.notebook);
            url.and_then(|u| t.get(&(nb, u)).and_then(|t| t.as_ref().map(|t| t.size)))
        };
        let Some(natural) = natural else { return };
        let live = self
            .live_width
            .get()
            .filter(|(s, _)| *s == sel)
            .map(|(_, p)| p);
        let mut pct = f32::from(image_size::effective_pct(
            natural,
            content_w,
            live.or(annotated),
        ));
        let before = pct;
        // Arrow keys while the slider has focus: 1 point, Shift+arrow 10. Consumed here so the
        // slider's own (range-relative) key step does not also fire.
        if self
            .slider_id
            .is_some_and(|sid| ctx.memory(|m| m.has_focus(sid)))
        {
            let (steps, big) = ctx.input_mut(|i| {
                let mut n = 0i32;
                for (k, d) in [
                    (Key::ArrowRight, 1),
                    (Key::ArrowUp, 1),
                    (Key::ArrowLeft, -1),
                    (Key::ArrowDown, -1),
                ] {
                    while i.consume_key(Modifiers::NONE, k) {
                        n += d;
                    }
                }
                (n, i.modifiers.shift)
            });
            pct += (steps * if big { 10 } else { 1 }) as f32;
        }
        let screen = ctx.content_rect();
        let h = 40.0;
        let below = rect.bottom() + 8.0;
        let y = if below + h <= screen.bottom() {
            below
        } else if rect.top() - 8.0 - h >= screen.top() {
            rect.top() - 8.0 - h
        } else {
            (screen.bottom() - h - 6.0).max(screen.top())
        };
        let x = rect.left().clamp(
            screen.left() + 4.0,
            (screen.right() - 330.0).max(screen.left() + 4.0),
        );
        let mut reset = false;
        let mut source = false;
        let (mut drag_over, mut down, mut focused) = (false, false, false);
        let mut slider_id = None;
        let out = egui::Area::new(id.with("image-controls"))
            .order(egui::Order::Foreground)
            .fixed_pos(Pos2::new(x, y))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().slider_width = 140.0;
                        // Theme colours: a visible rail, the accent for the filled part.
                        let v = ui.visuals_mut();
                        v.widgets.inactive.bg_fill = pal.separator;
                        v.selection.bg_fill = pal.link;
                        let r = ui.add(
                            egui::Slider::new(&mut pct, 20.0..=100.0)
                                .step_by(1.0)
                                .fixed_decimals(0)
                                .trailing_fill(true)
                                .suffix("%")
                                .show_value(true),
                        );
                        slider_id = Some(r.id);
                        // A slider does not take keyboard focus from a click by itself.
                        if r.is_pointer_button_down_on() || r.dragged() {
                            r.request_focus();
                        }
                        #[cfg(test)]
                        {
                            self.probe.controls.clear();
                            self.probe.controls.push(("slider", r.rect));
                        }
                        drag_over = r.dragged();
                        down = r.is_pointer_button_down_on();
                        focused = r.has_focus();
                        let rb = ui.add_enabled(
                            annotated.is_some() || live.is_some(),
                            egui::Button::new("Reset"),
                        );
                        reset = rb.clicked();
                        let sb = ui.button("Source");
                        source = sb.clicked();
                        #[cfg(test)]
                        {
                            self.probe.controls.push(("reset", rb.rect));
                            self.probe.controls.push(("source", sb.rect));
                        }
                    });
                });
            });
        self.controls_rect = out.response.rect;
        self.slider_id = slider_id;
        let keyed = (pct - before).abs() > 0.0 && !drag_over && !down;
        if (pct - before).abs() > 0.0 {
            let v = pct.round().clamp(20.0, 100.0) as u16;
            self.live_width.set(Some((sel, v)));
            self.slider_keys |= keyed;
            ctx.request_repaint();
        }
        // Mouse: one edit when the button comes up. Keyboard: one edit when focus leaves.
        if self.live_width.get().is_some() {
            if self.slider_keys {
                if !focused {
                    self.commit_width(ctx, id, text);
                }
            } else if !drag_over && !down {
                self.commit_width(ctx, id, text);
            }
        }
        if reset {
            self.live_width.set(None);
            self.set_image_width(ctx, id, text, sel, None);
            // The button press blurred the editor; give it back so Ctrl+Z works at once.
            ctx.memory_mut(|m| m.request_focus(id));
        }
        if source {
            self.dismiss_image(ctx, id, text, true);
            Self::edit_image_source(ctx, id, text, sel);
        }
    }

    fn slash_items(&self, text: &str, caret: usize) -> Vec<edit::SlashCommand> {
        let Some(s) = &self.slash else { return vec![] };
        if caret < s.trigger + 1 || !text.is_char_boundary(s.trigger + 1) {
            return vec![];
        }
        edit::slash_filter_with(&text[s.trigger + 1..caret], self.strike_command)
    }
    /// Draw the editor into `ui` and handle its input for one frame.
    ///
    /// The editor fills the available space (inside its own vertical `ScrollArea`); the host owns the
    /// surrounding panel, margins and window. Nothing is saved: edits are made in `input.text` and
    /// reported in [`EditorOutput::changed`].
    pub fn show(&mut self, ui: &mut egui::Ui, input: EditorInput<'_>) -> EditorOutput {
        let EditorInput {
            key,
            text,
            attachments: store,
            palette,
            keys,
            bold,
            focus,
        } = input;
        let ctx = &ui.ctx().clone();
        let id = key.widget_id();
        self.enter(key);
        self.changed = false;
        self.export_requested = false;
        self.status = None;
        self.strike_command = keys.strike_command;
        let pal = *palette;
        let bold = bold.cloned();
        let probe = font_probe(ctx, bold.as_ref());
        if self.fonts != Some(probe) {
            // Cached table measurements depend on the fonts' metrics.
            self.tables.borrow_mut().clear();
            self.heights.borrow_mut().clear();
            self.fonts = Some(probe);
        }
        if self.palette != Some(pal) {
            // Cached table galleys point into the font atlas; egui rebuilds it when the
            // light/dark glyph gamma changes, so drop them on every theme change.
            self.tables.borrow_mut().clear();
            self.palette = Some(pal);
        }

        // The export shortcut is consumed before Ctrl+E (inline code) can match it.
        if keys.consume_one(ctx, Action::Export) {
            self.export_requested = true;
        }
        // Strikethrough is consumed even without focus, ahead of any host Ctrl+S: egui ignores an
        // extra Shift when matching.
        let strike = keys.consume_one(ctx, Action::Strike);
        let pasted: Vec<_> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| {
                    if let egui::Event::PasteImage(img) = e {
                        Some(img.clone())
                    } else {
                        None
                    }
                })
                .collect()
        });
        for img in pasted {
            self.paste_image(ctx, id, &img, text, store);
        }

        // Slash menu navigation must be consumed before the TextEdit sees the keys.
        let (sel_a, sel_b) = Self::selection_bytes(ctx, id, text);
        let caret = sel_a;
        self.emoji_keys(ctx, id, text);
        let menu_open = self.slash.as_ref().is_some_and(|s| s.picker.is_none());
        let items = if sel_a == sel_b && menu_open {
            self.slash_items(text, caret)
        } else {
            vec![]
        };
        if menu_open && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            // Close the menu, keep the typed text, and keep typing: the editor stays focused.
            self.slash = None;
            ctx.memory_mut(|m| m.request_focus(id));
        }
        let menu_open = self.slash.as_ref().is_some_and(|s| s.picker.is_none());
        if menu_open && !items.is_empty() {
            let (down, up, enter) = ctx.input_mut(|i| {
                (
                    i.consume_key(Modifiers::NONE, Key::ArrowDown),
                    i.consume_key(Modifiers::NONE, Key::ArrowUp),
                    i.consume_key(Modifiers::NONE, Key::Enter)
                        | i.consume_key(Modifiers::NONE, Key::Tab),
                )
            });
            let s = self.slash.as_mut().unwrap();
            if down {
                s.selected = (s.selected + 1) % items.len();
            }
            if up {
                s.selected = (s.selected + items.len() - 1) % items.len();
            }
            if enter {
                let cmd = items[s.selected.min(items.len() - 1)];
                self.run_slash(ctx, id, &cmd, caret, text);
            }
        }
        if self.slash.is_none() {
            self.image_keys(ctx, id, text);
        }
        if self.slash.is_none() && ctx.memory(|m| m.has_focus(id)) {
            self.editing_keys(ctx, id, text, keys, strike);
        }

        let escape_left = ctx.input(|i| {
            i.events.iter().any(|e| {
                matches!(
                    e,
                    egui::Event::Key {
                        key: Key::Escape,
                        pressed: true,
                        ..
                    }
                )
            })
        });
        let escape = self.was_focused && escape_left;

        let avail = ui.available_size();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // The document inset lives here, in the container: the text column, its wrap
                // width, the image width and every decoration measure from the padded area.
                egui::Frame::NONE
                    .inner_margin(egui::Margin::symmetric(DOC_PAD as i8, 0))
                    .show(ui, |ui| {
                        let width = ui.available_width().min(780.0);
                        ui.add_space(8.0);
                        let sel_now = Self::selection_bytes(ctx, id, text);
                        *self.layout_sel.borrow_mut() = sel_now;
                        let live_c = &self.live_width;
                        let heights_c = &self.heights;
                        let (spans_c, sel_c, textures_c, images_c, store_c, bold_c, tables_c) = (
                            &self.spans,
                            &self.layout_sel,
                            &self.textures,
                            &self.images,
                            store,
                            &bold,
                            &self.tables,
                        );
                        let mut layouter = |ui: &egui::Ui,
                                            buf: &dyn egui::TextBuffer,
                                            wrap: f32| {
                            let src = buf.as_str();
                            let mut cache = spans_c.borrow_mut();
                            if cache.0 != src {
                                *cache = (
                                    src.to_owned(),
                                    Rc::new(markdown::parse(src)),
                                    Rc::new(Offsets::build(src)),
                                );
                            }
                            let (a, b) = *sel_c.borrow();
                            let (a, b) = (a.min(src.len()), b.min(src.len()));
                            let height = |start: usize, url: &str, width: Option<u16>| {
                                let width = match live_c.get() {
                                    Some((s, p)) if s == start => Some(p),
                                    _ => width,
                                };
                                image_height(
                                    textures_c,
                                    key.notebook,
                                    store_c,
                                    ui.ctx(),
                                    url,
                                    wrap,
                                    width,
                                )
                            };
                            let table_h = |t: &markdown::Table| {
                                table_height(
                                    heights_c,
                                    tables_c,
                                    ui.ctx(),
                                    src,
                                    t,
                                    wrap,
                                    bold_c.as_ref(),
                                ) + table::TABLE_PAD
                            };
                            let params = layout::Params {
                                base_size: BASE_SIZE,
                                text_color: pal.text,
                                heading_colors: pal.headings,
                                quote_color: pal.quote_text,
                                code_color: pal.code_text,
                                dim_color: pal.reveal,
                                accent: pal.link,
                                code_bg: pal.code_bg,
                                bold_family: bold_c.clone(),
                                selection: a.min(b)..a.max(b),
                                image_height: &height,
                                table_height: &table_h,
                            };
                            let (job, slots) =
                                prof!("build_job", layout::build_job(src, &cache.1, &params, wrap));
                            *images_c.borrow_mut() = slots;
                            prof!("layout_job", ui.ctx().fonts_mut(|f| f.layout_job(job)))
                        };
                        // egui collapses a range selection to its caret whenever a `TextEdit` is drawn
                        // without owning input, which is the first frame after switching back to this
                        // note. Remember the range now and put it back after the widget has run.
                        let keep_range = if self.entered {
                            egui::TextEdit::load_state(ctx, id)
                                .and_then(|s| s.cursor.char_range())
                                .filter(|r| r.primary != r.secondary)
                        } else {
                            None
                        };
                        // Reserved slot so block decorations are drawn *behind* the text.
                        let under = ui.painter().add(egui::Shape::Noop);
                        let out = prof!(
                            "textedit.show (incl. layouter)",
                            egui::TextEdit::multiline(&mut *text)
                                .id(id)
                                .frame(egui::Frame::NONE)
                                .desired_width(width)
                                .min_size(Vec2::new(width, (avail.y - 24.0).max(100.0)))
                                .lock_focus(true)
                                .layouter(&mut layouter)
                                .show(ui)
                        );
                        if let Some(r) = keep_range {
                            let n = text.chars().count();
                            let c = |i: CCursor| CCursor::new(i.index.0.min(n));
                            let mut st = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
                            st.cursor.set_char_range(Some(CCursorRange::two(
                                c(r.secondary),
                                c(r.primary),
                            )));
                            st.store(ctx, id);
                            ctx.request_repaint();
                        }
                        if focus == FocusRequest::Request {
                            out.response.request_focus();
                        }
                        self.changed |= out.response.changed();

                        // Positions of source offsets are looked up through one `Geo` per frame, and
                        // only for blocks near the viewport (see `geom`).
                        let (cache_ok, offsets, spans_now) = {
                            let cache = self.spans.borrow();
                            (cache.0 == *text, cache.2.clone(), cache.1.clone())
                        };
                        let view = view_span(ui, &out);
                        prof!("decor_shapes", {
                            if cache_ok {
                                let geo = Geo::new(text, &offsets, &out.galley, view);
                                let (a, b) = *self.layout_sel.borrow();
                                let decor = layout::decorations(&spans_now, &(a.min(b)..a.max(b)));
                                let shapes = decor_shapes(
                                    &decor,
                                    &geo,
                                    out.galley_pos,
                                    out.response.rect.width(),
                                    &pal,
                                );
                                ui.painter().set(under, egui::Shape::Vec(shapes));
                            }
                        });

                        // Paint images into the rows reserved for them by the layout.
                        let image_rects: Vec<(usize, Rect)> = prof!("images", {
                            let geo = Geo::new(text, &offsets, &out.galley, view);
                            let mut image_rects: Vec<(usize, Rect)> = Vec::new();
                            for slot in self.images.borrow().iter().filter(|s| !s.active) {
                                let tex_guard = self.textures.borrow();
                                let Some(Some(tex)) =
                                    tex_guard.get(&(key.notebook, slot.url.clone()))
                                else {
                                    continue;
                                };
                                let r = geo.pos(slot.byte_start.min(text.len()));
                                let h = slot.height - IMAGE_PAD;
                                let w = tex.size.x * h / tex.size.y;
                                let min =
                                    out.galley_pos + Vec2::new(r.min.x, r.center().y - h / 2.0);
                                let rect = Rect::from_min_size(min, Vec2::new(w, h));
                                image_rects.push((slot.byte_start, rect));
                                ui.painter().rect_filled(rect, 0.0, pal.image_bg);
                                ui.painter().image(
                                    tex.handle.id(),
                                    rect,
                                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                                    Color32::WHITE,
                                );
                                ui.painter().rect_stroke(
                                    rect,
                                    0.0,
                                    egui::Stroke::new(1.0, pal.image_border),
                                    egui::StrokeKind::Outside,
                                );
                            }
                            image_rects
                        });

                        // Paint inactive tables into the (concealed) rows reserved for them.
                        prof!("tables", {
                            if cache_ok {
                                let geo = Geo::new(text, &offsets, &out.galley, view);
                                #[cfg(test)]
                                self.probe.tables.clear();
                                let (a, b) = *self.layout_sel.borrow();
                                let sel = a.min(b)..a.max(b);
                                let colors = table::Colors {
                                    text: pal.text,
                                    header_bg: pal.code_block_bg,
                                    border: pal.separator,
                                    header_rule: pal.rule,
                                };
                                for sp in spans_now.iter() {
                                    let markdown::Kind::Table(t) = &sp.kind else {
                                        continue;
                                    };
                                    if !geo.is_visible(&sp.range)
                                        || layout::touches(&sp.reveal, &sel)
                                    {
                                        continue;
                                    }
                                    let wrap = out.galley.job.wrap.max_width;
                                    let l = table_layout(
                                        &self.tables,
                                        ctx,
                                        text,
                                        t,
                                        wrap,
                                        bold.as_ref(),
                                    );
                                    let r = geo.pos(t.header.range.start);
                                    let top = out.galley_pos.y + r.center().y - l.height / 2.0;
                                    table::paint(
                                        ui.painter(),
                                        Pos2::new(out.galley_pos.x, top),
                                        &l,
                                        t,
                                        text,
                                        BASE_SIZE,
                                        bold.as_ref(),
                                        &colors,
                                    );
                                    #[cfg(test)]
                                    self.probe.tables.push(sp.range.start);
                                }
                            }
                        });

                        prof!(
                            "task_and_link_interaction",
                            self.task_and_link_interaction(ui, id, &out, &pal, text)
                        );
                        prof!(
                            "image_interaction",
                            self.image_interaction(ui, id, &out, &pal, &image_rects, text)
                        );

                        if let Some(range) = out.cursor_range {
                            let c = out.galley.pos_from_cursor(range.primary);
                            let caret_rect = c.translate(out.galley_pos.to_vec2());
                            self.caret_pos = caret_rect.left_bottom() + Vec2::new(0.0, 4.0);
                            // Layout used the selection from the previous frame; re-layout if it moved.
                            let now = (
                                edit::char_to_byte(text, range.primary.index.0),
                                edit::char_to_byte(text, range.secondary.index.0),
                            );
                            if now != *self.layout_sel.borrow() {
                                ctx.request_repaint();
                            }
                            // Slash trigger: the user just typed `/` at a valid position.
                            if out.response.changed() && range.is_empty() && self.slash.is_none() {
                                let at = now.0;
                                if at > 0
                                    && !in_code(&self.spans.borrow().1, at - 1)
                                    && text.is_char_boundary(at - 1)
                                    && text[..at].ends_with('/')
                                {
                                    let t = at - 1;
                                    if edit::is_slash_trigger(text, t) {
                                        self.slash = Some(Slash {
                                            trigger: t,
                                            selected: 0,
                                            picker: None,
                                        });
                                    }
                                }
                            }
                        }
                    });
            });

        // Close the menu when the caret leaves the `/query` or nothing matches.
        let (a, b) = Self::selection_bytes(ctx, id, text);
        if let Some(s) = self.slash.as_ref().filter(|s| s.picker.is_none()) {
            let valid = a == b
                && a > s.trigger
                && text
                    .get(s.trigger..a)
                    .is_some_and(|q| q.starts_with('/') && !q.contains(char::is_whitespace));
            if !valid || self.slash_items(text, a).is_empty() {
                self.slash = None;
            }
        }
        // A press on the popup takes focus from the editor; keep drawing the menu (at the last known
        // caret position) so the release lands on an item. A press anywhere else closes it.
        if self.slash.as_ref().is_some_and(|s| s.picker.is_none())
            && !ctx.memory(|m| m.has_focus(id))
        {
            let pressed_outside = ctx.input(|i| {
                i.pointer.any_pressed()
                    && i.pointer
                        .interact_pos()
                        .is_some_and(|p| !self.slash_rect.contains(p))
            });
            if pressed_outside {
                self.slash = None;
            }
        }
        if let Some(s) = self.slash.as_ref().filter(|s| s.picker.is_none()) {
            let items = self.slash_items(text, a);
            let selected = s.selected;
            let (rect, clicked) = slash_popup(ctx, id, self.caret_pos, &items, selected);
            self.slash_rect = rect;
            if let Some(cmd) = clicked.and_then(|i| items.get(i)) {
                self.run_slash(ctx, id, cmd, a, text);
            }
        }
        self.emoji_popup(ctx, id, text);
        let has_focus = ctx.memory(|m| m.has_focus(id));
        self.was_focused = has_focus;
        self.entered = false;
        EditorOutput {
            changed: self.changed,
            selection: Self::selection_bytes(ctx, id, text),
            has_focus,
            escape,
            export_requested: self.export_requested,
            status: self.status.take(),
        }
    }
}

/// A task box that is currently drawn as a checkbox, in galley space.
struct TaskBox {
    start: usize,
    checked: bool,
    rect: Rect,
}

/// Square slots for every task box not touched by the selection (those show their raw `[ ]`).
fn task_boxes(geo: &Geo, spans: &[Span], sel: &std::ops::Range<usize>) -> Vec<TaskBox> {
    let mut out = Vec::new();
    for sp in spans {
        let markdown::Kind::Task { checked } = sp.kind else {
            continue;
        };
        if sp.range.end > geo.text.len()
            || !geo.is_visible(&sp.range)
            || layout::touches(&sp.reveal, sel)
        {
            continue;
        }
        let (r0, r1) = (geo.pos(sp.range.start), geo.pos(sp.range.end));
        if r1.min.x <= r0.min.x {
            continue; // wrapped across rows: leave it as text
        }
        let side = (BASE_SIZE * 0.78).min(r1.min.x - r0.min.x);
        let c = Pos2::new((r0.min.x + r1.min.x) / 2.0, r0.center().y);
        out.push(TaskBox {
            start: sp.range.start,
            checked,
            rect: Rect::from_center_size(c, Vec2::splat(side)),
        });
    }
    out
}

fn paint_checkbox(painter: &egui::Painter, rect: Rect, checked: bool, pal: &Palette) {
    let r = 3.0;
    if checked {
        painter.rect_filled(rect, r, pal.link);
        let (w, h) = (rect.width(), rect.height());
        let p = |x: f32, y: f32| rect.min + Vec2::new(w * x, h * y);
        painter.add(egui::Shape::line(
            vec![p(0.24, 0.52), p(0.43, 0.72), p(0.77, 0.30)],
            egui::Stroke::new(1.8, pal.canvas),
        ));
    } else {
        painter.rect_stroke(
            rect,
            r,
            egui::Stroke::new(1.4, pal.muted),
            egui::StrokeKind::Inside,
        );
    }
}

/// Make `e` (same length as what it replaces) leave the selection `primary`/`secondary` as it was.
fn keep_selection(mut e: Edit, primary: usize, secondary: usize) -> Edit {
    e.caret = primary;
    e.anchor = (primary != secondary).then_some(secondary);
    e
}

/// Apply `e` to `text` and store the caret/selection it asks for in the editor's `TextEditState`.
///
/// The edit is its own undo step. egui's undoer only closes a step after the text has been stable
/// for a while, so without explicit points a command and the typing that follows (or the typing
/// that preceded it) would merge into one step. We record the state before and after the edit
/// with `Undoer::add_undo`, which keeps egui's native undo/redo.
fn apply_edit_to(ctx: &Context, id: Id, text: &mut String, e: &Edit, rev: bool) {
    let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
    let before_cursor = state
        .cursor
        .char_range()
        .unwrap_or_else(|| CCursorRange::one(CCursor::new(edit::byte_to_char(text, e.range.end))));
    let mut undoer = state.undoer();
    undoer.add_undo(&(before_cursor, text.clone()));
    e.apply(text);
    let c = |b: usize| CCursor::new(edit::byte_to_char(text, b.min(text.len())));
    let range = match e.anchor {
        Some(a) if rev => CCursorRange::two(c(e.caret), c(a)),
        Some(a) => CCursorRange::two(c(a), c(e.caret)),
        None => CCursorRange::one(c(e.caret)),
    };
    undoer.add_undo(&(range, text.clone()));
    state.set_undoer(undoer);
    state.cursor.set_char_range(Some(range));
    state.store(ctx, id);
    // Show the edit now, not on the next input event.
    ctx.request_repaint();
}

/// Slash command popup; returns the area's rect.
fn slash_popup(
    ctx: &Context,
    id: Id,
    pos: Pos2,
    items: &[edit::SlashCommand],
    selected: usize,
) -> (Rect, Option<usize>) {
    let out = egui::Area::new(id.with("slash"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            let mut clicked = None;
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_width(SLASH_WIDTH);
                for (i, it) in items.iter().enumerate() {
                    let sel = i == selected.min(items.len().saturating_sub(1));
                    let r = ui.selectable_label(sel, it.title);
                    // Items are mouse targets only: a focused item would swallow Enter/Space.
                    if r.has_focus() {
                        r.surrender_focus();
                    }
                    if r.clicked() {
                        clicked = Some(i);
                    }
                }
            });
            clicked
        });
    (out.response.rect, out.inner)
}

/// Is byte `pos` inside a fenced or indented code block?
fn in_code_block(spans: &[Span], pos: usize) -> bool {
    spans
        .iter()
        .any(|sp| matches!(sp.kind, markdown::Kind::CodeBlock { .. }) && sp.range.contains(&pos))
}

/// Is byte `pos` inside/indented code block or inline code span?
fn in_code(spans: &[Span], pos: usize) -> bool {
    spans.iter().any(|sp| {
        matches!(
            sp.kind,
            markdown::Kind::CodeBlock { .. } | markdown::Kind::Code
        ) && sp.range.contains(&pos)
    })
}
#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "multinote_host.rs"]
mod multinote_host;

#[cfg(test)]
#[path = "multinote_tests.rs"]
mod multinote_tests;
