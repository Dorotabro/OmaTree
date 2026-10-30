//! Spacing rules of the host's chrome (ticket E6b). Everything the host draws around the editor
//! takes its measurements from here, so the title, the tree and the bars share one rhythm.
//!
//! The editor keeps its own 16-point document padding (`DOC_PAD` in the editor library); the host
//! does not change it. [`GUTTER`] is that same distance, used as the one horizontal axis the note
//! title (and its rule) hang on, so the title's first letter sits exactly above the body's.

/// The shared left axis of the editor column: the distance from the pane's left edge to the first
/// letter of the title and of the Markdown body.
pub(crate) const GUTTER: f32 = 16.0;
/// A small gap: panel insets, the space between related controls.
pub(crate) const PAD: f32 = 8.0;
/// Height of the two bottom bars (the tree's command strip and the status bar). Equal heights put
/// their top edges on one line across the window.
pub(crate) const BAR_H: f32 = 32.0;
/// Space above the first tree row, under the search field's rule.
pub(crate) const TREE_TOP: f32 = 6.0;
/// Type scale of the chrome: the tree, the search field and a result's title use `UI_PX`; the
/// status bar, the command strip, counts and secondary lines use `SMALL_PX`. The note title is
/// `TITLE_PX`; the Markdown body has its own scale in the editor.
pub(crate) const UI_PX: f32 = 15.0;
pub(crate) const SMALL_PX: f32 = 13.0;
/// Height of one tree row (also the drag and drop-zone unit).
pub(crate) const ROW_H: f32 = 28.0;
/// Horizontal step per nesting level.
pub(crate) const INDENT: f32 = 16.0;
/// Left inset of the tree's first column.
pub(crate) const TREE_LEFT: f32 = 8.0;
/// The note title's size and the room around it.
pub(crate) const TITLE_PX: f32 = 21.0;
pub(crate) const TITLE_TOP: f32 = 10.0;
/// Height of the title's text row; the search field above the tree sits in a row of the same
/// height and the same distance from the top, so the two share a centre line.
pub(crate) const TITLE_ROW_H: f32 = 26.0;
/// One line under the title reserved for its validation message.
pub(crate) const TITLE_NOTE_H: f32 = 15.0;
/// Height of the header on both sides of the window: the search field over the tree and the
/// note title over the editor. Their rules are drawn on its bottom edge, so they form one line.
pub(crate) const HEADER_H: f32 = TITLE_TOP + TITLE_ROW_H + TITLE_NOTE_H;
/// The note header's colour picker: a clickable cell per swatch, the dot drawn in it (radius), the
/// gap between cells. The picker's region is six cells, five gaps and the right gutter; a row
/// narrower than `HEADER_FULL_MIN` (the region plus a title that can still be read and edited)
/// gets one `HEADER_SWATCH` cell instead.
pub(crate) const HEADER_SWATCH: f32 = 22.0;
pub(crate) const HEADER_DOT: f32 = 6.5;
pub(crate) const HEADER_SWATCH_GAP: f32 = 2.0;
pub(crate) const HEADER_PICKER_W: f32 = 6.0 * HEADER_SWATCH + 5.0 * HEADER_SWATCH_GAP + GUTTER;
pub(crate) const HEADER_COMPACT_W: f32 = HEADER_SWATCH + GUTTER;
pub(crate) const HEADER_FULL_MIN: f32 = HEADER_PICKER_W + 2.0 * GUTTER + 150.0;
/// The tree pane may be dragged between these widths; the default is its starting width in a
/// window wide enough for it.
pub(crate) const TREE_MIN: f32 = 170.0;
pub(crate) const TREE_DEFAULT: f32 = 300.0;
/// A window narrower than this lets the tree take no more than `TREE_MAX_SHARE` of it.
pub(crate) const TREE_MAX_SHARE: f32 = 0.6;
/// In a small window the tree starts at this share of it instead (never below `TREE_MIN`).
pub(crate) const TREE_START_SHARE: f32 = 0.4;
/// How long a status message about an action stays in the bar, and how long a failure does.
pub(crate) const FEEDBACK_SECS: f64 = 6.0;
pub(crate) const PROBLEM_SECS: f64 = 15.0;
/// Search results: text sizes, the room around a result and between its lines. A result is two
/// lines (title, snippet) or three when it lives inside a branch (title, path, snippet).
pub(crate) const RESULT_TITLE_PX: f32 = UI_PX;
pub(crate) const RESULT_SUB_PX: f32 = SMALL_PX;
pub(crate) const RESULT_PAD_Y: f32 = 6.0;
pub(crate) const RESULT_TITLE_H: f32 = 19.0;
pub(crate) const RESULT_SUB_H: f32 = 16.0;
/// An edit made while Search is open refreshes the results after this long (one scan, however
/// many keys were typed meanwhile).
pub(crate) const SEARCH_DEBOUNCE: f64 = 0.12;
/// A scan that takes longer than this (seconds) makes typing wait for a pause before searching.
pub(crate) const SLOW_SCAN: f64 = 0.008;
