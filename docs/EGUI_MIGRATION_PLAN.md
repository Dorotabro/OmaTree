# OmaTree: Qt/QML → egui migration plan

Status: **plan, revised by ticket E0.** The maintainer's decisions are recorded in `docs/EGUI_DECISIONS.md` (D1–D9) and applied below; tickets, risks and the parity checklist were revised accordingly. This is the integration-branch (`egui-integration`) copy. No application code has changed; the Qt application is still the only implementation.

Evidence conventions used throughout:

- **[Confirmed]** was read in the code of `OmaTree` (`4efb1b4`, v0.1.2) or `omatree-egui-editor-spike` (no git history; the PDF-export state, 169 tests).
- **[Proposed]** is a design recommendation, not something that exists.
- **[Unverified]** is an assumption that must be tested before it is relied on.

I did **not** build or run OmaTree's test suite (it needs the Qt toolchain, and the task forbids changes). Test counts below are `#[test]` counts by `grep`, not pass results.

---

## 0. Decisions (resolved in E0)

All three items below are now decided; see `docs/EGUI_DECISIONS.md`. The original text is kept for the reasoning.

| Item | Outcome |
|---|---|
| Stack rule | D1: Qt is replaced by egui; the Qt app stays buildable until cut-over. `AGENTS.md`, `ARCHITECTURE.md`, `VISION.md` amended in E0. |
| Code sharing | Library crate for the editor (D3). Path vs git dependency still open (E2/E4). |
| Image scope | D8: notebook-owned attachments only; no remote or arbitrary filesystem images. |
| Shortcuts / Preview | D4: no Edit/Preview toggle. D5: Ctrl+E is inline code. D6: Ctrl+Shift+S is Save As; strikethrough gets another binding or a slash command. |
| Attachment ids | D7: random 128-bit, `assets/<32-hex-id>.<ext>`. |
| Cleanup | D9: pruning and Compact Notebook are required (§5.4, E9). |

1. **The stack rule.** `AGENTS.md` lists Qt 6 / QML / CXX-Qt as the approved stack and says not to replace it "without explicit instruction". [Confirmed] This task is that instruction, but `AGENTS.md`, `ARCHITECTURE.md` and `VISION.md` all describe Qt, and `ARCHITECTURE.md` says "the editor is a plain-text editor … Do not introduce a rich-text document model". The egui editor is a source-backed live-styled editor (still plain text underneath), so the intent holds, but the documents must be amended on the integration branch (ticket E0).
2. **How the two repositories share code** (see §3.4). Recommendation: turn the spike into a library crate plus its existing demo binary, and have OmaTree depend on it by path during integration.
3. **Image scope.** OmaTree's Preview "never loads local or remote resources" and has a test for it (`tests/qml/resources.qml`). [Confirmed] Embedded attachments are a *new* capability that must be reconciled with that principle (§5.5): only attachments stored inside the notebook may load, never files or URLs.

---

## 1. Current architecture map

### 1.1 OmaTree (Qt) [Confirmed]

```
QML (≈3,000 lines, 12 files)           tests/qml (24 scenarios + Base/runner)
  main.qml  TreePane  EditorPane  SearchPane  RecoveryDialog  ThemedDialog …
        │  Qt.styleHints, FileDialog, Shortcut, TreeView, TextArea, TextEdit(RichText preview)
        ▼
CXX-Qt bridges (QObjects)              C++ shims (cpp/)
  tree_model.rs   NotebookModel          markdown_render.cpp  (Qt's Markdown → rich text)
  theme_model.rs  Theme (singleton)      app_identity / controls_style /
  markdown.rs     Markdown (singleton)   keyboard_policy / file_open (macOS Finder events)
        ▼
Pure Rust core (no Qt)
  notebook.rs  document.rs  storage.rs  recovery.rs  search.rs  theme.rs
        ▼
rusqlite (bundled SQLite), toml
```

Sizes: `document.rs` 3,104 lines, `tree_model.rs` 1,397, `storage.rs` 1,388, `notebook.rs` 1,324, `theme.rs` 964, `recovery.rs` 527, `search.rs` 410, `theme_model.rs` 189, `markdown.rs` 138. Direct dependencies: `cxx`, `cxx-qt`, `cxx-qt-lib`, `rusqlite` (bundled), `toml`.

### 1.2 Notebook model [Confirmed: `notebook.rs`]

- `NodeId(u64)`: stable, never reused within a notebook (a persisted counter in `notebook_meta.next_node_id`). `Node { id, parent_id, position, title, body }`. Every node is a note; no separate folder type.
- `Notebook` owns `BTreeMap<NodeId, Node>` and `next_id`. Nodes are read-only to callers; invariants (contiguous sibling positions, no cycles) are enforced inside `Notebook`.
- Operations: `create_root/child`, `rename` (empty and case-insensitive sibling-duplicate titles refused), `set_body`, `delete` (subtree), `plan_move/move_node` (cycle check), `restore_subtree`, `depth_first`, `breadcrumb`.
- Many helpers are `pub(crate)` (23 in `notebook.rs`, 10 in `recovery.rs`, 5 in `storage.rs`). That matters for a crate split (§6, E1).

### 1.3 Document lifecycle, dirty state, save/load [Confirmed: `document.rs`]

`Document` = `Notebook` + `Recovery` + `Option<Storage>` + `path` + dirty flags. Three independent dirty flags:

| Flag | Set by | Counts as unsaved content |
|---|---|---|
| `active_dirty` | create/rename/`set_body`/move/delete | yes |
| `recovery_dirty` | `create_checkpoint`, delete, restore | yes |
| `view_dirty` | expand/collapse | **no** (written with the next save, never prompts) |

- `set_body(id, body)` returns whether the text actually changed (O(n) comparison) and only then sets `active_dirty`.
- `save()` writes the minimum: nothing if clean; `save_active_with_view` (nodes + counter + expanded) for ordinary edits; the full atomic `save_document_with_view` only if Trash or checkpoints changed or the file is older than the current schema.
- `save_as(path, overwrite)` is transactional: a new file is created and removed again on failure; an existing file is refused unless confirmed *and* a valid notebook.
- Conflict guard: `Storage` records `PRAGMA data_version` when it opens the file and refuses to write (`ExternallyModified`) if **another connection** has committed since. [Confirmed: `begin_checked_write`.] This is important for attachments (§5.4).
- Untitled notebooks live only in memory (`storage: None`; `save` → `NoPath`).
- **There is no separate "editor buffer"**: QML's `onTextChanged` calls `notebook.setBody(index, text)` on every change (`EditorPane.qml`), so the `Notebook` is always current. Autosave is a QML `Timer` restarted by the `documentMutated` signal (`main.qml`). Close/new/open go through `whenSafeToLeave` continuations that save or ask.

### 1.4 SQLite schema [Confirmed: `storage.rs`]

`user_version` = 3 (`SCHEMA_VERSION`); versions 1–3 readable (`OLDEST_READABLE_VERSION`), older files migrated only by a successful save.

```sql
-- v1
nodes(id INTEGER PRIMARY KEY, parent_id REFERENCES nodes(id) ON DELETE CASCADE,
      position INTEGER NOT NULL CHECK(position>=0), title TEXT NOT NULL, body TEXT NOT NULL)
-- v2
notebook_meta(id=1 CHECK, next_node_id)
trash_entries(id, deleted_at, root_node_id)
trash_nodes(entry_id→trash_entries CASCADE, node_id, parent_id, position, title, body, PK(entry_id,node_id))
checkpoints(id, created_at, reason)
checkpoint_nodes(checkpoint_id→checkpoints CASCADE, node_id, parent_id, position, title, body, PK(...))
-- v3
expanded_nodes(node_id PRIMARY KEY REFERENCES nodes(id) ON DELETE CASCADE)
```

Notable facts for the migration:

- Both save paths do `DELETE FROM nodes` then re-insert every node (`write_nodes`). Any table with a foreign key to `nodes ON DELETE CASCADE` is **emptied by every save** (that is why `expanded_nodes` is explicitly rewritten).
- Checkpoints and trash are full copies of bodies, capped at 100 checkpoints / 16 MiB (`MAX_CHECKPOINTS`, `MAX_CHECKPOINT_BYTES`).
- `save_document_with_view` runs `VIEW_SCHEMA` whenever `from < SCHEMA_VERSION`. Bumping `SCHEMA_VERSION` without restructuring that block would re-run `CREATE TABLE expanded_nodes` on a v3 file and fail.

### 1.5 Search [Confirmed: `search.rs`]

Linear scan over the in-memory `Notebook`: trimmed, Unicode-lowercased substring in title or body; title matches first, then body-only, each in tree order; snippet with 20 characters of context, 80 maximum. No index, no SQL. Pure Rust; 16 tests.

### 1.6 Qt-specific parts

| Piece | Where | What it does |
|---|---|---|
| `NotebookModel` | `tree_model.rs` (157 Qt references) | A `QAbstractItemModel` over `Notebook`: indexes carry `NodeId`; `beginInsertRows`/`beginMoveRows`/reset; ~50 invokables for QML (create/rename/move/drop/trash/checkpoints/search/expand/save/open/save-as). All domain logic it calls is in `Document`. |
| `Theme` singleton | `theme_model.rs` | Exposes the `Palette` to QML, polls Omarchy via `OmarchyWatcher`, takes `Qt.styleHints.colorScheme`. |
| `Markdown` singleton | `markdown.rs` + `cpp/markdown_render.cpp` | **Only `is_safe_external_link` is pure Rust.** The Preview renderer is C++ using Qt's Markdown → rich text. |
| QML views | `qml/*.qml` | `TreeView` (+ custom drag and drop, guide lines, context menu), `TextArea` editor, `TextEdit` RichText preview, dialogs, shortcuts, Keymap. |
| Platform shims | `cpp/*.cpp` | Window icon, Windows AppUserModelID, Basic controls style, macOS keyboard policy, macOS Finder open-document events. |

### 1.7 Unexpected Qt dependencies [Confirmed]

- `markdown.rs` mixes one pure function with a Qt bridge in the same file; the pure function needs moving out.
- `theme.rs` is Qt-free, **but** `Scheme::from_qt(i32)` encodes `Qt::ColorScheme` values, and `tests/qml/systemtheme.qml` tests the system-scheme behaviour through Qt.
- `src/integration_tests.rs` (626 lines) spawns the Qt app (`OMATREE_TEST_QML`) and is in the same crate as the core, so core tests and Qt tests are not separable today (a separate `tests/` crate "cannot link the CXX-Qt objects", per its own header).
- `main.rs` has a debug-only `OMATREE_TEST_QML` hook.
- Packaging for Linux (AppImage, deb, Arch), macOS (dmg), and Windows (Inno Setup) is built around dynamic Qt, and `THIRD_PARTY.md` is mainly a Qt licence inventory.

### 1.8 Existing tests [Confirmed by grep; not run]

| Module | `#[test]` | Depends on Qt? |
|---|---|---|
| `document.rs` | 120 | no |
| `notebook.rs` | 41 | no |
| `storage.rs` | 34 | no |
| `theme.rs` | 27 | no |
| `search.rs` | 16 | no |
| `recovery.rs` | 15 | no |
| `tree_model.rs` | 15 | yes (QObject model) |
| `integration_tests.rs` | 26 | yes (spawns the real app) |
| `markdown.rs` | 2 | only for the part that needs the bridge |
| QML scenarios (`tests/qml`) | 24 files | yes (QtTest, offscreen) |

**Roughly 250 of 296 Rust tests are pure and portable; the Qt-bound ones are the executable specification of the UI behaviour that egui must reproduce** (§8).

---

## 2. The egui editor audit

### 2.1 Module inventory [Confirmed]

| Module | Lines | egui-coupled? | Reusable as is? |
|---|---|---|---|
| `markdown.rs` (spans, tables, `image_references`) | 990 | no (pulldown-cmark only) | **yes** |
| `edit.rs` (edits, slash commands, lists, formatting) | 1,143 | no | **yes** |
| `attachments.rs` (`AttachmentStore`, `FsStore`, `normalize`, `referenced`) | 502 | no | **yes** (trait); `FsStore` is spike-only |
| `export.rs` (plan, folder, ZIP, atomic write) | 731 | no | **yes** (takes a store) |
| `pdf.rs` (krilla + rustybuzz layout) | 2,353 | no | **yes** (takes markdown + store + path) |
| `emoji.rs`, `links.rs`, `image_size.rs` | 392 / 98 / 109 | thin (`Vec2`, `Modifiers`) | yes |
| `layout.rs` (`LayoutJob` builder, concealment) | 859 | yes, but a pure function of text, spans, selection, params | **yes** |
| `table.rs` (table galley) | 331 | yes (fonts) | yes |
| `theme.rs` (spike `Palette`, `ThemeWatcher`) | 510 | yes (`Color32`, `Visuals`) | **no, see §2.5** |
| `fonts.rs`, `assets.rs` | 64 / 44 | yes | mostly |
| **`app.rs` (`EditorApp`)** | 3,665 (≈1,900 production, rest tests) | everything | **no, this is the part to split** |

The dependency set (eframe 0.37 with glow, wayland, x11, clipboard, links; `pulldown-cmark`, `image` (PNG only), `toml`, `emojis`, `zip`, `krilla`, `rustybuzz`) adds about 7 MB for PDF (spike `FINDINGS.md`). Fonts bundled: Liberation, about 1.9 MB.

### 2.2 What `EditorApp` mixes together

`EditorApp` (`app.rs`) holds, in one struct:

| Concern | Fields | Belongs to |
|---|---|---|
| Document | `text: String`, `saved: String`, `path`, `save_blocked`, `status` | host (OmaTree's `Document`) |
| Persistence | `save()` writes `path` with `std::fs::write`; `new()` reads it | host |
| Attachments | `store: Box<dyn AttachmentStore>` built as `FsStore(dir of path)` | host injects |
| Theme | `theme: ThemeWatcher`, `palette` | host |
| Editing state | `slash`, `selected_image`, `live_width`, `slider_*`, `controls_rect`, `link_press`, `caret_pos` | editor |
| Render caches | `spans`, `textures`, `images`, `tables`, `layout_sel` (all `RefCell`) | editor |
| Export dialog | `export`, `run_export` | editor or host (see §3.6) |
| Frame loop | `impl eframe::App` with `CentralPanel` | host |

Filesystem and process assumptions to remove: `std::fs` read/write of the document; `FsStore(dir)`; `active_palette_path()` reading `$HOME/.local/state/omarchy/...` (OmaTree's own `default_omarchy_colors_path` honours `XDG_STATE_HOME`, the spike's does not); `main.rs` window setup; the default export path derived from the document's file stem.

### 2.3 Ownership of the document `String`, editor state, undo

- **Document text:** one `String` (`EditorApp.text`), handed as `&mut` to `egui::TextEdit::multiline`. The spike's rule "one authoritative buffer" holds because nothing else stores text. [Confirmed]
- **Caret, selection, undo history:** these are **not** in `EditorApp`; they live in egui's `TextEditState` in `ctx` memory, keyed by the widget `Id` (`Id::unique("editor")`, a *fixed* id). `apply_edit_to` mutates that state through `TextEdit::load_state` / `Undoer::add_undo` / `store`. [Confirmed]
- **Consequence for a multi-note app:** with a fixed id, switching the text to another note would apply the previous note's cursor and undo stack to the new note's text. The id must be per note (`Id::unique(("note", node_id))`). `Id::unique` hashes with a fixed seed, so ids are stable. [Confirmed in egui 0.37 source]
- **Undo state is in-memory only** (egui memory); it is not persisted and is lost on restart. Same as today's QML `TextArea`.
- **Tested boundary behaviour** to preserve: programmatic edits (slash commands, emoji, image paste, width slider) are separate undo steps via the two `add_undo` calls; this relies on egui's `Undoer` public API, a coupling that should be pinned by tests.

### 2.4 Image texture/cache ownership [Confirmed]

`textures: RefCell<HashMap<String, Option<Tex>>>` keyed by the **reference string**, decoded once per reference through `store.get`, failures cached as `None`. Textures are created with `ctx.load_texture`, so egui owns the GPU handle; the map owns the `TextureHandle`.

Problems when hosting: the cache key must be scoped to the notebook (a reference like `assets/x.png` means different things in two notebooks); the cache is never invalidated if an attachment is replaced or a notebook is closed (fine for immutable content-addressed ids, wrong for filesystem paths); the `image` crate is built with PNG only, so JPEG attachments display as missing in the editor while the PDF path (krilla) accepts them. [Confirmed in `Cargo.toml` and `assets.rs`]

### 2.5 Theme handling and fallback [Confirmed]

Two independent theme systems exist:

| | OmaTree `theme.rs` | Spike `theme.rs` |
|---|---|---|
| Output | `Palette` of 14 semantic roles (`Rgb`) | `Palette` of ~20 roles (`Color32`, including `headings[6]`, quote, code, link, reveal, image, menu) |
| Source | Omarchy `colors.toml` → else OmaTree Dark/Light by **system colour scheme**, live | Omarchy `colors.toml` → else a built-in dark palette |
| Watching | `OmarchyWatcher` (confirms a change on two consecutive polls) | `ThemeWatcher` (1 s poll, keeps last valid) |
| Path | honours `XDG_STATE_HOME`; Linux only | `$HOME/.local/state/…`, any OS |

**Proposed:** OmaTree's `theme.rs` stays the single source of truth (it has 27 tests and defines the product's behaviour). The editor takes a *palette trait or struct supplied by the host*; the spike's role derivation (headings, quote, code, …) moves into a `EditorPalette::from_semantic(&theme::Palette) -> EditorPalette` function. The system colour scheme comes from egui (`ctx.system_theme()` / viewport info) instead of `Qt.styleHints`. [Unverified: that egui 0.37 reports system theme changes live on all three platforms; test in ticket E2/E6.]

### 2.6 Requirements for hosting the editor in a parent egui app

1. It must render into a caller-provided `&mut Ui` instead of owning a `CentralPanel`.
2. The text buffer and attachment store must be injected, with no file access.
3. Widget `Id`s derived from an `id_salt` or note id, so two editors, or one editor over several notes, do not collide. (The spike uses `Id::unique(("task", start))` and `("image", start)` for sub-widgets: these are global and byte-offset-based, so they must include the editor id.)
4. Keyboard shortcuts it consumes (Ctrl+E inline code, Ctrl+Shift+S strikethrough, Ctrl+Shift+E export, Ctrl+B/I, Ctrl+Enter) must not collide with OmaTree's (`Keymap.qml`: Ctrl+E is *Edit/Preview toggle*, Ctrl+Shift+S is *Save As*, Ctrl+N/Shift+N/O/S/F, Alt+Up/Down, F2, F1). **Confirmed conflicts: Ctrl+E and Ctrl+Shift+S. Resolved (D5, D6):** Ctrl+E is inline code and the Edit/Preview toggle is dropped; Ctrl+Shift+S is Save As and strikethrough moves to another shortcut or a slash command. The editor takes its bindings from the host (`EditorKeys`); the standalone editor may keep different ones.
5. Focus handoff with the tree and title field (Escape returns to the tree, Enter in the title focuses the body).
6. Caches (`spans`, `tables`, `images`) keyed by text content must survive or be cheaply rebuilt on note switch.

### 2.7 Smallest practical refactoring [Proposed]

Make the spike a library crate and extract a widget with this shape; `EditorApp` becomes a thin demo host around it:

```
omatree_editor (lib)
  EditorView   — render caches + editing state (slash, image selection, link press); no text, no store
  EditorView::show(&mut self, ui, EditorInput) -> EditorOutput
  (markdown, edit, layout, table, attachments (trait only), export, pdf: unchanged)
omatree-egui-editor-spike (bin)
  EditorApp   — file text + FsStore + ThemeWatcher + save, calls EditorView::show
```

Mechanically this moves about 20 fields out of `EditorApp` and replaces `self.text`/`self.store` reads inside the ~1,200-line `ui()` body with the `EditorInput` references. The 60 or so tests in `app.rs` (`AppHarness`-driven) can keep running against `EditorApp` unchanged, which is exactly what makes this safe.

---

## 3. Integration boundaries (proposed Rust APIs)

Principle: **`omatree_core::Document` stays the only owner of note content.** The editor works on a transient `String` that is synchronised with the document every frame it changes. There is no second store.

### 3.1 Crate layout [Proposed]

```
OmaTree/                       (Cargo workspace on the integration branch)
  crates/core/     omatree-core    notebook, document, storage, recovery, search, theme, link safety   (no Qt, no egui)
  crates/editor/   omatree-editor  from the spike: markdown, edit, layout, table, attachments (trait), export, pdf, EditorView
  crates/qt-app/   omatree         today's Qt app, depends on core; unchanged behaviour
  crates/egui-app/ omatree-egui    new app, depends on core + editor
```

Only `core` is shared by both apps; `editor` is egui-only. The Qt app and its packaging stay intact until parity.

### 3.2 Editor component

```rust
pub struct EditorView { /* caches, slash/emoji/image/link state, per-note UI state */ }

pub struct EditorInput<'a> {
    pub note: NoteKey,                 // identifies the note; salts widget ids
    pub text: &'a mut String,          // working buffer (see 3.4)
    pub attachments: &'a dyn AttachmentStore,
    pub palette: &'a EditorPalette,
    pub keys: &'a EditorKeys,          // which shortcuts the editor may consume
    pub focus: FocusRequest,           // None | Request
}

pub struct EditorOutput {
    pub changed: bool,                 // text bytes differ from the start of the frame
    pub focus_lost_to_host: bool,      // Escape pressed
    pub export_requested: Option<ExportKind>,
    pub status: Option<String>,        // one-line notices ("Image pasted", errors)
}

impl EditorView {
    pub fn new() -> Self;
    pub fn show(&mut self, ui: &mut egui::Ui, input: EditorInput<'_>) -> EditorOutput;
    /// Forget everything cached/stored for a note (it was deleted, or its body was replaced from outside).
    pub fn forget_note(&mut self, ctx: &egui::Context, note: NoteKey);
    pub fn caret_and_selection(&self, ctx: &egui::Context, note: NoteKey) -> (usize, usize); // bytes
}
```

`NoteKey` = `(notebook_uid, NodeId)`; it salts the `TextEdit` id and every sub-widget id, and scopes the texture cache.

### 3.3 Creating an editor / loading or switching the active note

The host (`egui-app`) owns `Document` and `selected: Option<NodeId>`. Switching is:

1. `commit_pending_title()`: the existing rule from `EditorPane.qml`/`titles.qml`: a title being typed is committed against *its own* note before leaving; a refused commit blocks the switch.
2. The buffer is *not* saved separately: `Document.set_body` was already called on the last change (3.4), so the notebook is current.
3. `buffer = document.notebook().get(id).body().to_owned()` (one clone, once per switch).
4. Because the `TextEdit` id includes the `NodeId`, egui restores that note's caret, selection and undo history from memory if the note was visited before. A first visit starts at the top, which matches `EditorPane.load()`.

### 3.4 Retrieving edited content and reporting changes

```rust
// Host, once per frame, after EditorView::show:
if out.changed {
    document.set_body(id, &buffer)?;        // O(n) once per *changed* frame; returns whether it really changed
    autosave.restart();                      // same rule as QML's documentMutated
}
```

- This is exactly what `onTextChanged → notebook.setBody(...)` does today, so the observable semantics (dirty on any actual change, Trash/checkpoint untouched, `documentMutated` equivalent) are unchanged. [Confirmed equivalence]
- **Invariant** (assert in debug builds and in a test): after the sync, `document.notebook().get(id).body() == buffer`.
- The cost is two `String` copies per changed frame for large notes. Acceptable for note-sized text; the spike has not measured large documents (its own `FINDINGS.md` says so), so ticket E3 includes a 200 KB typing-latency measurement.
- Alternative [rejected for now]: `Notebook::body_mut` so `TextEdit` edits the node's own `String`. It would remove the copy but weaken `Notebook`'s "read-only to callers" invariant and bypass `set_body`'s dirty check.

### 3.5 Caret, selection and undo per note

- Stored by egui in `TextEditState` per `Id` (§2.3), so per-note state needs no new structure, only per-note ids.
- **Invalidation rule [Proposed, must be tested]:** when a note's body changes *behind the editor's back* (restore from Trash, restore checkpoint, Save As reload, external reopen) the stored cursor and undo stack no longer describe the text. The host calls `forget_note` for every affected id; for a whole-notebook replace it calls `EditorView::reset()`.
- Rename, move and expand/collapse do not touch the body and need no invalidation.
- Memory: egui keeps state for each visited note until forgotten. Bound it (LRU of, say, 64 notes) or accept it; a note's undo history is ephemeral anyway.
- Undo of a body edit never touches the notebook structure; structural operations stay in OmaTree's checkpoint/Trash recovery system. This separation is already how Qt behaves (the `TextArea` undo is per-document-instance and reset on selection change) and is **not** an improvement to promise.

### 3.6 `AttachmentStore` for SQLite

```rust
pub struct NotebookAttachments { /* shared handle to the Document's Storage + pending map */ }
impl AttachmentStore for NotebookAttachments { fn put/get/info }
```

Hard constraint from §1.4: **the store must use the same SQLite connection as the `Document`'s `Storage`**, not a second connection, otherwise every attachment commit changes `data_version` for the main connection, and the next note save fails with `ExternallyModified`. So the store is a view onto `Document` (e.g. `Rc<RefCell<Document>>` shared by the app, or `Document` exposing `attachments(&self) -> &dyn AttachmentStore`). The `AttachmentStore` trait takes `&self`, so `Document` needs interior mutability for the pending map. Details in §5.

Untitled (in-memory) notebooks: attachments stay in the in-memory pending map until the first Save As writes them.

### 3.7 Triggering exports for the active note

Reuse `export::plan/write_dir/write_zip` and `pdf::export_pdf` unchanged:

```rust
let body = &buffer;                          // the live buffer, so unsaved edits are included
pdf::export_pdf(body, &*attachments, &title, &dest)
export::plan(body, &*attachments, &note_file_name)   // folder / ZIP
```

The title for the PDF and the Markdown file name come from the node title (`export::note_name` already sanitises names). The exporters read only the `AttachmentStore`, so they are backend-independent (this is already tested with a non-filesystem store in the spike). OmaTree has no export feature today; the popup (Ctrl+Shift+E) becomes a host dialog because the destination picker is a native file dialog, not a text field.

### 3.8 Save coordination and close confirmation

Port, do not redesign, the logic in `main.qml`, which `tests/qml/{autosave,conflict,replacement,titleclose,lifecycle}.qml` pin down:

```rust
enum Leave { Quit, NewNotebook, OpenNotebook(PathBuf) }
fn when_safe_to_leave(&mut self, then: Leave)   // flush title, save if path, else Save As / ask Discard
```

- `Document::save` is already the synchronous, atomic unit. The egui app calls it from the frame loop on a timer (`ctx.request_repaint_after`) and from `ViewportCommand::CancelClose` handling for close requests (`ctx.input(|i| i.viewport().close_requested())`). [Unverified: close interception on every platform and under macOS Cmd+Q.]
- The editor never saves and never prompts; the "unsaved" decision depends only on `Document::is_dirty()`.

---

## 4. Avoiding a second document store; unsaved edits across node switches

- **Authority:** `Document` (via `Notebook`) holds every note's body at all times. The editor buffer is a working copy for the *one* active note.
- **Because of the write-through in §3.4 an unsaved edit is "retained" the moment it is typed**: it is in `Notebook`, marks the document dirty, and is what Save, close-confirmation, search, export and checkpoints will see.
- Switching away discards the buffer, not the data; switching back reloads the body and restores caret and undo from egui memory.
- Notes without a path are retained exactly as today (in memory, flagged dirty, "Save As…" on close).
- **Not retained** by design: undo history across a restart, and across `forget_note` invalidations. Say so in the UI docs.
- Search results and the title being edited are the other pieces of ephemeral state that must be resolved before a switch (title: §3.3; search: derived, recomputed from the notebook, as today).
- Edge cases that need explicit tests: edit → switch → Ctrl+Z (undo applies to the right note and does not resurrect the other note's text); edit → delete the *same* note (forget state; checkpoint taken before delete includes the latest body); edit → restore checkpoint → return to the note (no stale undo).

---

## 5. SQLite attachments: compatible extension (proposal, nothing implemented)

### 5.1 Requirements derived from the code

- References live in Markdown as `![alt](REF)`. The spike's `attachments::normalize` accepts only relative, `/`-separated paths with no `..`, no `:` in the first segment and no `://`, and its exporters write the reference path as-is into folder/ZIP exports. A reference should therefore **look like a relative path** so that Markdown exported to a folder or ZIP keeps working in any other Markdown tool.
- Trash and checkpoints copy bodies, so a body in Trash or an old checkpoint can still reference an attachment. Deleting a note must not make its attachments unrecoverable.
- Saves rewrite `nodes` completely, so the attachment table must not have a cascading foreign key to `nodes` (§1.4).
- Savepoint semantics: notes are "saved" only by `Document::save`; an image pasted and then undone must not be a permanent row forever, but also must not disappear if the body that references it is saved.

### 5.2 Reference format and identifiers [Proposed]

`![shot](assets/<id>.<ext>)` where `<id>` is 128 random bits in lowercase hex (32 chars) and `<ext>` is derived from sniffed media type (`png`, `jpg`), e.g. `assets/3f9c…e1.png`.

- Stable (never reused, independent of node ids and positions); survives copying a note between notebooks as "image missing" rather than a *wrong* image (a counter id would silently resolve to a different picture).
- Passes the existing `normalize`; exports as a normal relative path; `FsStore`-style tools can read an exported folder.
- Open question: content hash (SHA-256, deduplication) instead of random. It needs a new dependency and gives dedupe for repeated pastes; recommend random ids first and revisit.

### 5.3 Schema, version 4 [Proposed]

```sql
CREATE TABLE attachments (
    id          TEXT PRIMARY KEY CHECK (length(id) = 32),   -- the <id> in the reference
    media_type  TEXT NOT NULL,                               -- 'image/png' | 'image/jpeg'
    created_at  INTEGER NOT NULL,
    byte_len    INTEGER NOT NULL CHECK (byte_len > 0),
    data        BLOB NOT NULL
) WITHOUT ROWID;
```

- **No foreign key to `nodes`, trash or checkpoints.** What an attachment "belongs to" is derived from bodies (a scan for `assets/<32 hex>.`). That is cheap and avoids double bookkeeping that can drift.
- **Lazy migration:** files stay at version 3 until the first attachment is saved; only then `user_version` becomes 4 and the table is created inside the same transaction as the body that references it. Consequences: notebooks that never use images remain readable by the Qt app and by v0.1.x; a v4 file is refused by an older OmaTree with the existing `UnsupportedSchemaVersion` message ("not a notebook this version can open … left untouched"), which is a safe failure. [Confirmed from `Storage::open`: only 1..=3 accepted.]
- `Storage` changes: accept version 4; restructure `save_document_with_view` into per-version steps (`if from < 2 {RECOVERY} if from < 3 {VIEW} if from < 4 {ATTACHMENTS}`) because today's `from < SCHEMA_VERSION` block would re-run `CREATE TABLE expanded_nodes` (§1.4). Keep `is_current_schema` meaning "has recovery + view tables" (≥ 3) so `save_active_with_view` keeps working for v3 and v4.
- **Size guard:** reuse `MAX_ATTACHMENT_BYTES` (64 MiB) per blob as in the spike; consider a per-notebook soft limit with a visible warning. SQLite incremental blob I/O (`rusqlite::blob`) is an option for large images; unnecessary for screenshots.

### 5.4 Write path, unsaved state, failure recovery

- `put` stores bytes in an in-memory `pending` map and returns the reference immediately; nothing touches the file. This keeps "all unsaved changes live in memory and are committed by one transaction" true for images, and keeps the *same connection* rule.
- `Document::save*` writes pending attachments first, **in the same transaction** as `nodes`, `expanded_nodes` and the counter; failure rolls everything back and `pending` is kept, exactly like dirty flags are kept today.
- `get`/`info`: pending first, then `SELECT … WHERE id = ?`. Never reads outside the DB; ids that don't match the 32-hex pattern are `Invalid`; unknown ids are `NotFound`.
- **Cleanup and compaction (D9, required; replaces the earlier "GC on full save" idea).** Nothing deletes an attachment during autosave, ordinary save, Save As, note switching, restore or close. Orphans accumulate until the user runs **Compact Notebook**:
  - *Reference-aware pruning.* The referenced set is the union of ids found (plain-text scan of `assets/<32-hex>.` references, not a Markdown parse) in `nodes` bodies, `trash_nodes` bodies, `checkpoint_nodes` bodies and the pending map. Only attachments outside that set are prunable. It is computed from the whole stored state, never from visible or selected notes. An attachment that Trash or a retained checkpoint can restore is never removed; it becomes prunable only after that entry is gone (deleted from Trash, or dropped from the checkpoints by the 100 / 16 MiB limits).
  - *Compact Notebook operation.* A deliberate command, never automatic. Step 1 is a dry run that reports: unreferenced attachments (count, bytes), attachments kept because Trash or checkpoints reference them (count, bytes), and the expected file-size change. Step 2 asks for confirmation. Step 3 prunes in one transaction using the checked-write guard (`ExternallyModified` aborts with nothing deleted). Step 4 reports the result: removed, kept, bytes freed.
  - *Space reclamation is separate.* Deleting rows does not shrink the file. `VACUUM` (or an equivalent) is a second, separately reported step, offered after pruning and also usable on its own, with the file size before and after. It needs free disk about the size of the database and no other open connection; failure leaves the original file intact and is reported, not hidden. The exact strategy (`VACUUM`, `VACUUM INTO` plus replace, or incremental vacuum) is chosen in E9.
  - *Reporting.* Counts and sizes in plain words, before and after.
- **Save As:** `save_document_with_view` into a new file must also copy referenced attachments from the old store; if copying fails, the new file is removed (the existing behaviour for failed Save As).
- **Recovery:** because blobs are committed with the notes that reference them in one transaction, a crash leaves either the old or the new file state. The conflict guard (`ExternallyModified`) applies to attachment writes for free, since they are part of the same checked transaction.
- **Checkpoints** are *not* extended with blobs: they would blow the 16 MiB budget with one screenshot. Restoring a checkpoint whose body references an attachment shows the image, because pruning never removes an attachment a retained checkpoint references; "image missing" can only occur after the checkpoint itself has been dropped and the attachment pruned.

### 5.5 Exporting a note with its attachments

Unchanged exporters: `export::plan(markdown, store, name)` calls `attachments::referenced` and gets bytes from the SQLite-backed store; folder/ZIP exports write `assets/<id>.<ext>` and the Markdown **byte-for-byte** (the spike's rule), so references stay valid in the exported folder. PDF embeds them. Missing attachments are reported by name and the export fails rather than silently dropping the image (spike behaviour).

### 5.6 Import/compat with existing Markdown

Existing notes contain no image syntax handled by the app; Qt Preview strips images. In egui, an existing `![x](https://…)` stays source text with a "remote images are not loaded" placeholder; `![x](assets/…)` that does not resolve in the notebook shows as missing; any other relative path (`../x.png`, `/etc/…`) is not loaded either (D8). **No file or network access ever.** `FsStore` must not be linked into the OmaTree app (it reads arbitrary relative paths next to a document file). This carries the principle of `resources.qml` into egui; that test itself is Preview-specific and is retired with Preview (D4), replaced by an attachment-loading test (E10).

---

## 6. Migration strategy and tickets

Rules for the whole sequence:

- Work on an **integration branch or a git worktree** of OmaTree (`git worktree add ../OmaTree-egui egui-integration`), not on `main`; Qt app builds and passes all existing tests at every ticket until the cut-over ticket (E13).
- The spike repository stays a runnable standalone editor until ticket E4 produces an equivalent library; after that the spike is a thin demo over the library.
- No SQLite migration ships before E9.

### Tickets

**E0: Decision record and branch**
- Objective: record the stack change and create the integration branch/worktree.
- Status: **done in E0** (documentation only).
- Affected: new branch; `AGENTS.md`, `ARCHITECTURE.md`, `VISION.md` (amended on the branch only); a short `docs/EGUI_DECISIONS.md` listing §0 answers.
- Behaviour: none.
- Tests: none. GUI: none.
- Acceptance: maintainer answers §0 items and the key-conflict question (§2.6): given, see `docs/EGUI_DECISIONS.md`.
- Risks: none.

**E1: Extract `omatree-core` (Qt-free library crate)**
- **Status: implemented and verified on `egui-integration` (not committed).** See "E1 implementation record" below this ticket list. Deviations from the original ticket text are listed there.
- Objective: separate `notebook`, `document`, `storage`, `recovery`, `search`, `theme` and the pure `is_safe_external_link` into a workspace crate used by the Qt app.
- Affected: Cargo workspace; module moves; visibility (`pub(crate)` → `pub` for the ~39 items counted in §1.2 that `tree_model.rs`/tests touch: `plan_move`, `plan_trash_restore`, `Checkpoint`, `TrashEntry`, etc.); `theme.rs::Scheme::from_qt` moved to `theme_model.rs`; `integration_tests.rs` stays with the Qt crate.
- Behaviour: none; identical app.
- Tests: the ~250 pure Rust tests pass in `omatree-core` with **no Qt installed** (verify with Qt env vars unset or in a container without Qt); all 296 Rust tests and 24 QML scenarios pass unchanged in the Qt app.
- GUI: smoke run of the Qt app (open, edit, save, Trash, search).
- Acceptance: `cargo test -p omatree-core` builds without Qt; the Qt packaging scripts still produce a working package; diff is moves and visibility only.
- Risks: packaging scripts reference `target/release/omatree` and `Cargo.toml` metadata; `build.rs`/cxx-qt `.files([...])` paths change. Mitigation: keep the Qt crate's binary name and path stable.

**E2: Embeddable editor in the spike repository**
- Objective: split `EditorApp` into `EditorView` (library) and `EditorApp` (demo), per §2.7 and §3.2. Also: key bindings come from an `EditorKeys` map supplied by the host (D5, D6): the OmaTree profile has Ctrl+E inline code, no Ctrl+Shift+S, and strikethrough on another binding or the slash command; the demo binary may keep its current bindings. (This ticket changes the standalone editor's repository; E0 does not.)
- Affected (spike repo): `src/app.rs` split; new `lib.rs`; Cargo `[lib]`+`[[bin]]`; `EditorInput`/`EditorOutput`; widget ids salted by `NoteKey`.
- Behaviour: standalone editor works exactly as before.
- Tests: all 169 existing tests pass unmodified; new tests: two `EditorView`s in one `Context` do not share caret/undo; same view with two `NoteKey`s keeps independent undo/caret; sub-widget ids (task boxes, image controls) don't collide across notes.
- GUI: spike on Wayland: the same acceptance script as for the previous tasks (typing, tasks, links, images, slash menu, export).
- Acceptance: no `std::fs`, `std::env` or `eframe::App` in the library; `EditorView::show` renders into a caller `Ui`.
- Risks: `Id::unique(("task", start))`-style ids; focus handling (`first_frame` request_focus must move to the host); the `Probe` test plumbing is inside `EditorApp`.

**E3: Per-note state, switching and undo semantics (still in the spike)**
- Objective: prove §3.3–§3.5: a test host with a `Vec<String>` of notes and two or more notes, write-through sync, per-note undo/caret, `forget_note`.
- Behaviour: edit note A, switch to B, edit, return to A: text, caret, undo intact; replacing a body from outside resets state.
- Tests: the edge cases at the end of §4; invariant `buffer == stored body` after every frame; 200 KB typing-latency measurement (release, headless) with a recorded budget.
- GUI: Wayland: scripted note switching with typing, undo across switches.
- Acceptance: no undo step ever applies to another note's text; latency budget recorded in `FINDINGS.md`.
- Risks: egui's `Undoer` cannot represent a body replaced behind its back (hence `forget_note`); memory growth of retained states.

**E4: Host skeleton: `omatree-egui` app opening a notebook**
- **Status: implemented and verified on `egui-integration` (not committed).** See "E4 implementation record". Deviations: the tree is flat but indented (depth-first), selectable; the editor edits through `Document::set_body` write-through.
- Objective: new workspace crate with an `eframe` window, opens a notebook path via `Document`, shows a **flat read-only tree** and a single `EditorView` for the selected note; `set_body` write-through; Ctrl+S.
- Affected: `crates/egui-app`; depends on `core` and `editor` (path dependency; this is where the code-sharing decision from §0 is exercised).
- Behaviour: open an existing `.omatree`, select a node, edit its body, save, reopen: content and tree unchanged; the Qt app opens the same file.
- Tests: headless tests with `egui_kittest`-style or the spike's frame harness [Unverified which; the spike's `AppHarness` is custom]: edit → `Document` body equals buffer → `save` → reopen → equal. A schema v3 notebook written by the Qt app round-trips byte-for-byte in `nodes`.
- GUI: open a copy of a real notebook (not the original); compare tree and bodies with the Qt app; confirm `cmp` on untouched notes.
- Acceptance: no data loss on open/edit/save/reopen with fixture notebooks produced by the Qt app; the file remains openable by Qt.
- Risks: first time the shared `Document` is driven from egui; copy fixture, never the user's real notebook.

**E5: Tree pane parity**
- **Status:** split into E5a (hierarchy, keyboard, node operations, persisted expansion: **implemented**, see "E5a implementation record") and E5b (drag-and-drop: **implemented**, see "E5b implementation record").
- Objective: replace `TreePane.qml`: expand/collapse (persisted via `Document::set_expanded`), selection, keyboard navigation (Up/Down/Left/Right/Ctrl+Left/Right/Enter/F2/Delete/Alt+Up/Down), guide lines, context menu (Shift+F10/Menu key), create root/child, rename (with the existing refusal messages), delete → Trash with the confirmation dialog, move up/down, drag-and-drop with `can_drop`/`drop_node` and drop indicators.
- Implementation notes: a hand-written flat, virtualised list over `depth_first` + expanded set is recommended; egui has no stock hierarchical tree view with drag-drop and this repo's rules are bespoke. Evaluating third-party tree crates is optional and **unverified** against egui 0.37.
- Tests: port `expansion`, `moveexpanded`, `newnote`, `titles`, `titleclose`, `keyboard`, `escape` scenarios as Rust tests over the egui host harness.
- GUI: keyboard-only walkthrough; mouse drag across nested, collapsed, expanded; the Wayland and X11 sessions.
- Acceptance: scenario-by-scenario parity (§8).
- Risks: drag-and-drop and keyboard focus are the largest UX unknown; large-notebook scrolling performance not measured.

**E6: Theme and palette**
- Objective: drive the editor and the host chrome from `omatree-core::theme::Palette`, including live Omarchy changes and the system light/dark scheme.
- Behaviour: same colour semantics as the Qt app; fallback to OmaTree Dark/Light; spike's derived roles produced by one function.
- Tests: reuse the 27 `theme.rs` tests; new tests: `EditorPalette::from_semantic` contrast checks (the spike's `contrast >= 4.5` assertions), live swap keeps the document byte-identical.
- GUI: switch an isolated Omarchy fixture dark↔light while a note with every construct is open; confirm egui reports system theme changes on Linux; mark macOS/Windows NOT TESTED if they are.
- Risks: the spike notes a glyph-gamma/atlas rebuild on theme change (it clears the table cache); egui system-theme reporting across platforms is **unverified**.

**E7: Search, Trash, checkpoints, dialogs**
- Objective: port `SearchPane`, `RecoveryDialog` (Trash and checkpoints tabs, keyboard-selectable), themed modal dialogs (unsaved changes, Move to Trash, errors, Save As overwrite, keyboard reference F1, listing the final egui bindings: Ctrl+E inline code, Ctrl+Shift+S Save As, and the strikethrough binding). The Trash and checkpoints views also show how many attachments each entry keeps alive, since they decide what Compact Notebook may remove (D9).
- Tests: `search`, `searchheader`, `recovery`, `recoverykeys`, `dialogs` ported; restore flows call `forget_note` (§3.5).
- GUI: restore a deleted subtree and a checkpoint while editing; verify undo does not resurrect old text.
- Risks: dialog keyboard model (N / Y / Esc / arrows) is custom and must be reimplemented.

**E8: File lifecycle: open, new, save as, autosave, close, conflict**
- Objective: port `main.qml` flow: native file dialogs, `.omatree` default extension, autosave timer, `when_safe_to_leave`, close interception, conflict message, command-line path argument, single `Document`.
- Dependency decision: a native file dialog crate (commonly `rfd`) [Unverified: version, portal behaviour on Wayland, licence]; log in the dependency list.
- Tests: `autosave`, `conflict`, `replacement`, `lifecycle`, `paths` (spaces, accents, Czech) scenarios ported; failure injection via `storage.rs` write guards (they are in core and reusable).
- GUI: open/save as/overwrite confirmation, close with unsaved changes; macOS Finder open-document events and Windows taskbar identity are separate follow-ups (§7).
- Risks: close interception under platform quit paths; dialog threading on macOS.

**E9: Attachments in SQLite (core, schema v4)**
- Objective: implement §5 in `omatree-core`: `attachments` table, lazy v4 migration, pending map, same-connection store, Save As copy, `NotebookAttachments: AttachmentStore`, **and the required cleanup and compaction (D9)**: a reference-aware prune and a deliberate Compact Notebook operation with dry-run report, confirmation and result report, with `VACUUM`-style space reclamation as a separate step. Ticket E9 is not complete without these.
- Behaviour: per §5. v3 files without images are never upgraded.
- Tests (all in core, no UI): put/get/info; invalid and unknown ids; save → reopen round-trip; v3 → v4 migration keeps all tables; migration failure leaves the old file intact (use the existing `install_write_guards`); a second connection committing causes `ExternallyModified` (and a paste does **not**); Save As copies blobs; **cleanup and compaction:** blobs referenced only from active notes, only from a Trash entry, only from a retained checkpoint, and only from pending state all survive pruning; true orphans are removed; nothing is ever deleted by autosave, save, Save As, restore or close; pruning decided from the whole stored state gives the same result whichever note is selected; the dry-run report matches what pruning then does (counts and bytes); a failure part-way (injected with the write guards) rolls back with every attachment still present; an interrupted prune or vacuum leaves a valid notebook that opens; an external modification between dry run and confirmation aborts with nothing deleted (`ExternallyModified`); an image deleted with its note is shown again after Trash restore, and after checkpoint restore, following a prune; a notebook with no attachments is byte-unchanged by Compact; `VACUUM` is reported separately, with file size before and after, and a refusal (open connection, low disk) is reported without data loss; an old binary refusal for v4 (open with the Qt build's `Storage` still at 1..=3).
- GUI: none beyond E10.
- Acceptance: the schema change is invisible to notebooks without images; no write path uses a second connection.
- Risks: this is the first schema change in the project after the recovery/view tables; pruning is the only place that deletes user bytes: it runs only from Compact Notebook, only on confirmation, only through the checked-write guard, and is tested hardest. `VACUUM` needs free disk and exclusive access; its failure modes need explicit handling.

**E10: Images in the egui app**
- Objective: wire `NotebookAttachments` into `EditorView`: paste, display, width annotation, slider, selection.
- Tests: the spike's image tests with the SQLite store; undo of a paste leaves an orphan that stays until Compact Notebook (no automatic removal); remote and non-attachment references are not loaded (D8); a closed notebook does not leak textures (texture cache scoped by notebook).
- GUI: paste a screenshot, resize, save, reopen, Save As, delete the note then restore from Trash (image returns); a note with `![x](https://…)` and `![x](/etc/…)` shows no image and performs no network or file access.
- Risks: PNG-only `image` feature set in the editor vs JPEG accepted by PDF (decide: enable `jpeg` in `image`, or restrict `put` to PNG and convert).

**E11: Export and Compact Notebook UI**
- Objective: (a) Markdown / folder / ZIP / PDF export of the active note via a host dialog with a native save dialog. (b) The user-facing **Compact Notebook** command: dry-run report, confirmation, result report with attachment counts and file sizes, and the separate space-reclamation step, on top of the core operations from E9. There is no Preview mode to port (D4).
- Tests: spike export tests re-run against the SQLite store; export of unsaved text; folder/ZIP byte-identity of the Markdown; PDF text extraction.
- GUI: export a note with images, tables, Czech text to each format; open the PDF. Compact Notebook on a fixture copy with orphaned, Trash-referenced and checkpoint-referenced images: the report counts and sizes match the file afterwards; cancelling changes nothing; keyboard-only operation.
- Risks: the +7 MB binary and ~135 extra `cargo tree` lines for PDF; consider a Cargo feature. The command must fit OmaTree's "no permanent toolbar" rule (menu or command, not a button strip).

**E12: Parity hardening and packaging prototype**
- Objective: confirm attachment cleanup and Compact Notebook were delivered and tested (E9, E11); run the full parity checklist (§8) on Linux (Wayland and X11), then Windows and macOS; accessibility and IME review; large-notebook performance; replace Qt packaging scripts with eframe equivalents on the branch (AppImage/deb/Arch/Windows installer/macOS bundle); update `THIRD_PARTY.md`.
- Acceptance: every §8 row is PASS or has an accepted written deviation; the Qt version is still buildable.
- Risks: see §7; macOS/Windows are untested ground for the editor.

**E13: Cut-over decision (maintainer's)**
- Objective: tag, switch the default build/package to egui, move the Qt app to `legacy/` or remove it in a follow-up. Only after E12.
- Not to be done automatically.

### E1 implementation record (what was actually done)

**Layout.** The repository root stays the Qt package `omatree` (same `[package]`, same `build.rs`, same `target/release/omatree`, same `Cargo.lock` location) and also becomes the workspace root: `[workspace] members = [".", "crates/core"]`, `default-members = [".", "crates/core"]`. The new library is `crates/core` (`omatree-core`, direct dependencies `rusqlite` (bundled) and `toml` only). *Deviation from §3.1:* the Qt app was **not** moved to `crates/qt-app`; every packaging script reads the root `Cargo.toml` version, builds with `cargo build --release --locked` at the root and stages `target/release/omatree`, and the Arch/deb/AppImage builders take their source from `git ls-files`. Keeping the root as the Qt package changes none of that. `default-members` keeps the documented bare `cargo fmt/clippy/test` covering the core as well.

**Moved to `omatree-core` (git sees them as renames; `storage.rs`, `recovery.rs`, `search.rs` and `document.rs` are byte-identical apart from one visibility keyword in `document.rs`):** `notebook`, `document`, `storage`, `recovery`, `search`, `theme`, plus a new `link` module holding `is_safe_external_link` and its two tests, which were split out of `markdown.rs`. The Qt `markdown.rs` keeps only the CXX-Qt bridge and calls `omatree_core::link::is_safe_external_link`.

**Stayed in the Qt crate:** `main.rs`, `app.rs`, `tree_model.rs`, `theme_model.rs`, `markdown.rs` (bridge), `integration_tests.rs`, `build.rs`, `qml/`, `cpp/`, `tests/qml/`, `packaging/`.

**Qt-specific logic moved out of the core:** `Scheme::from_qt` became the private function `scheme_from_qt` in `theme_model.rs`, with the same mapping (1 light, 2 dark, anything else unknown). Its assertions moved to a new test there. The core's `Scheme` enum stays (a plain light/dark/unknown value) and its doc comment no longer mentions Qt. The test `qml_contains_no_literal_ui_colours` reads `qml/*.qml`, so it moved to the Qt crate's `theme_model.rs` unchanged.

**Public surface added (everything else stayed `pub(crate)` or private; no new mutable access to notebook internals):**

| Item | Why the Qt adapter needs it |
|---|---|
| `NodeId::from_raw` | `tree_model.rs` carries a `NodeId` through `QModelIndex`'s internal id and re-validates it with `Notebook::get` on every use; doc comment now says it is an unvalidated name |
| `Document::plan_move`, `MovePlan` (struct, fields already `pub`) | the drag-and-drop move path |
| `Notebook::can_reparent` | `can_drop` |
| `Notebook::sibling_title_conflicts` | the duplicate-title warning |
| `Notebook::move_node` | a Qt-crate unit test builds notebooks directly; the method is a validated operation (cycle check, contiguous positions), so it does not bypass invariants |
| modules `document`, `link`, `notebook`, `recovery`, `search`, `storage`, `theme` | Rust visibility of the paths the bridges name (`omatree_core::recovery::TrashEntry` etc.) |

Not made public: `Node::from_parts`, `Notebook::from_nodes*`, `snapshot_nodes`, `subtree`, `restore_*`, `Checkpoint::of/from_stored`, `Recovery::from_parts/push_trash/take_trash/add_checkpoint`, the storage test helpers, and `set_body` remains the only way to change a body.

**Qt crate dependencies:** `toml` removed (no longer used there); `rusqlite` moved to `[dev-dependencies]` (only `integration_tests.rs` opens notebooks directly); `omatree-core` added. `Cargo.lock` gained one `omatree-core` package entry and the dependency edges that moved; no crate version changed.

#### Verification evidence (this machine: Arch/Omarchy, Qt 6.12.0, Hyprland/Wayland, release build)

| Check | Baseline (before E1, same tree) | After E1 |
|---|---|---|
| `cargo fmt --all -- --check` | pass | pass |
| `cargo clippy --all-targets -- -D warnings` | pass | pass (and `cargo clippy -p omatree-core --all-targets -- -D warnings` passes) |
| `cargo test` | 294 passed, 1 ignored, 0 failed | core 253 passed + 1 ignored; Qt crate 42 passed; total 295 passed + 1 ignored, 0 failed |
| `cargo build --release` / `--locked` | pass | pass, binary `target/release/omatree` (6.52 MB before, 6.56 MB after) |
| `cargo build -p omatree-core`, `cargo test -p omatree-core` | n/a | pass; **also pass in an environment with `PATH` reduced to cargo/rustc/rustdoc, the C compiler and linker, and no `qmake6`/Qt tools** (`env -i`, separate target dir). `cargo tree -p omatree-core` has 25 lines (rusqlite, toml and their dependencies) and no Qt, cxx, egui or eframe crate |

*Test accounting.* The baseline listed 295 tests (294 + 1 ignored). After the change there are 296 listed: the same 295, plus one new test (`theme_model::tests::qt_colour_scheme_values_map_to_schemes`). Two `markdown::tests` moved to `link::tests`, and `theme::tests::qml_contains_no_literal_ui_colours` moved to `theme_model::tests`; a diff of the sorted test lists shows no other change. The 24 QML scenarios run inside the Qt crate's `integration_tests` and passed. The ignored test is `document::tests::checkpoint_benchmark`, which I also ran explicitly (`-- --ignored`, release-free debug build, 33 s): pass.

*GUI smoke test* (release binary, `HOME` pointed at an empty directory, a hand-built v3 notebook copy in the scratchpad; scripted `wtype`/`ydotool`, `grim` screenshots): opened by command-line path and showed the tree; selected "Ideas" and edited it (typed Czech text with diacritics), saved with Ctrl+S (the text was in the file); searched "gamma" and opened the result; moved the note to Trash through the confirmation dialog; opened File ▸ Recovery… and restored it (the note returned under its parent, the Trash entry count dropped, checkpoints were taken); no output on stderr. The identical script was then run with the baseline binary (the pre-E1 build, copied aside) on a second copy of the same fixture. The logical dumps of both notebooks (all tables, `user_version`, schema SQL) are identical except for the Unix timestamps of the two checkpoints created during the run. The Qt app wrote `user_version` 3 and unchanged table definitions.

*Packaging.* `packaging/linux/stage.sh` with its default binary path (`target/release/omatree`) stages `usr/bin/omatree` correctly. All packaging scripts read the root `Cargo.toml`, build at the root and take sources from `git ls-files` (so `crates/` is included once tracked). **Not run:** the Arch, deb, AppImage, macOS and Windows package builds and their `verify-*` scripts (containers, macOS and Windows hosts needed). That the new files are tracked before such a build is a requirement; nothing is committed yet.

**Not verified / residual:** Windows and macOS builds of the workspace; the full package builds above; `cargo build --release` of the core alone is not part of the packaging flow. The core is compiled once per profile (shared by both default members), so build time is unchanged.

### E4 implementation record (first egui application shell)

**Status: implemented and verified on `egui-integration` (not committed).** A real `.omatree` notebook (copied fixture, schema 3) opens in a native egui window, a note can be selected and edited with live Markdown rendering, saved with Ctrl+S, closed and reopened with the edit intact, and the saved file opens in the Qt application with the same hierarchy and content.

**Structure.** New workspace member `crates/egui-app` (package `omatree-egui`, executable `target/release/omatree-egui`, library `omatree_egui`). `[workspace] members` gains it; `default-members` does **not**, so a bare `cargo build/test/clippy` still covers only the Qt app and the core, and the egui crate is built with `-p omatree-egui` or `--workspace`. Files: `lib.rs` (`Host`: window-independent state and the frame function; `App`: the `eframe::App` wrapper), `store.rs` (`UnavailableStore`), `main.rs` (argument, window options), `tests.rs`.

**Boundaries.**

| Owner | Owns |
|---|---|
| `omatree-core::Document` | The notebook: tree, titles, bodies, dirty state, save, conflict detection. The host never keeps a second copy of the tree's bodies. |
| `Host` (egui-app) | One `String` buffer for the selected note, lent to the editor; the depth-first row list (built once per opened notebook, never cloned per frame); selection; status text; Ctrl+S and the reserved Ctrl+Shift+S; window title; the attachment backend; the notebook counter. |
| `omatree_editor::EditorView` | Rendering, editing, caret, selection and undo per `NoteKey { notebook, note }` (notebook = host counter incremented on every open, note = `NodeId::get()`), pending image width, slash menu. Unchanged. |

**Write-through.** On a frame whose `EditorOutput.changed` is set, and again when leaving a note or saving, the host compares the buffer with the document's body and calls `Document::set_body` only if they differ (so switching alone never dirties). Before switching or saving it calls `EditorView::commit_pending` first (a pending image width becomes one undo step in *that* note). Opening another notebook forgets every note of the old one (`forget_note`) and its images (`forget_images`) and bumps the notebook counter, so equal node numbers in a new file can never meet old caret or undo state. Undo is E3's per-note egui `Undoer`; the host has no undo mechanism of its own.

**Core change (visibility only).** `Notebook::depth_first` is now `pub` (was `pub(crate)`), so the host can list the tree in the Qt app's order instead of copying the traversal. Nothing else in `omatree-core` changed.

**Cargo arrangement.** `omatree-core = { path = "../core" }`; the editor as `omatree_editor = { package = "omatree-egui-editor-spike", path = "../../../omatree-egui-editor-library" }` (the package keeps the spike's name; its library target is `omatree_editor`); `eframe 0.37` with the same feature set as the editor repository's binary. No other direct dependency; `cargo tree -p omatree-egui` contains no `cxx`/`qt` crate (also asserted by a test on the manifest). `Cargo.lock` gains the egui, eframe, krilla, image and related crates.

*Superseded by E4.1 (see below): the editor is now a pinned Git dependency and no sibling checkout is needed.* *Original layout requirement.* The repositories had to be siblings: `<parent>/OmaTree-egui` (this worktree) and `<parent>/omatree-egui-editor-library`. Because Cargo resolves **every** workspace member, a checkout without the sibling directory now fails `cargo build` **for the Qt app as well**, even though the Qt build never uses the egui crate. This is the price of a path dependency and is acceptable only for local integration.

*Proposal for reproducible builds (not applied, needs a maintainer decision).* (1) Publish the editor repository (or a tag) and depend on it by `git = "...", rev = "<commit>"`, with `Cargo.lock` pinning it; the egui crate then builds from a clean clone with network access, and the Qt packaging (`cargo build --release --locked`, `git ls-files` sources) keeps working because the dependency is resolved from the lock file. (2) Until then, if the Qt packaging must stay independent of the sibling checkout, move `crates/egui-app` out of `members` (or into `exclude` with its own `[workspace]`) so the Qt workspace never resolves the path dependency. (3) Longer term, fold the editor into this workspace as `crates/editor` (history kept with `git subtree`), which removes the layout requirement entirely and makes the editor and the application one reviewable unit; the standalone editor would then be a bin in this workspace.

**Attachments (until E9).** `UnavailableStore` implements `AttachmentStore`: `put` fails ("images inside notebooks are not supported yet"), `get`/`info` fail for every reference. It never touches the filesystem and never fetches, so `https://`, `file://`, absolute, `..` and even an existing `assets/x.png` beside the notebook are shown as unavailable and kept in the source byte for byte. A pasted image fails before any reference is written (status: "Image paste failed: ..."). The status bar always shows "Images are not supported yet". No schema change, no `FsStore`.

**Shortcuts.** `EditorKeys::omatree()`: Ctrl+E inline code; no strikethrough or export binding; `/strike` in the slash menu. The host consumes Ctrl+Shift+S first (status "Save As is not available yet.") because egui matches Ctrl+S even with Shift held; it never reaches Save or formatting. Ctrl+S saves. No Edit/Preview toggle.

**UI.** Left panel: all notes depth-first, 14 px indent per level, selected row highlighted (virtualised list). Right: the note title, the unchanged editor (16 pt document padding, central panel margin as in the multi-note example), a one-line status bar (message, "Unsaved changes" / "No unsaved changes", the image notice). Window title `<file> * - OmaTree` when dirty. A file that cannot be opened is reported in the window and on stderr; there is no editor then. Palette: the unified theme pipeline (E6a) replaced the editor's built-in fallback palette.

#### Verification evidence (this machine: Arch/Omarchy, Hyprland/Wayland, release build)

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass (bare `cargo clippy --all-targets` covers the Qt app and core as before) |
| `cargo test --workspace` | Qt crate 42 passed (includes the 24 QML scenarios via the integration test), core 253 passed + 1 ignored, egui-app 17 passed + 2 ignored (fixture writer, timings), 0 failed |
| `cargo build --release --workspace` | pass; `omatree` 6.5 MB, `omatree-egui` 22.3 MB |

*Automated (headless egui `Context`, real `Document` on temp-dir copies):* opening a schema-3 file (header `user_version` 3) with ids, titles, depth and order preserved; selecting every note; typing reaches `Document` the same frame and the file only on Ctrl+S; save → reopen equal; every unedited body byte-identical after another note was edited and saved; Czech text, emoji and a table source across a multi-byte caret position; per-note caret and undo/redo isolation across switches (undo in one note never changes another; caret restored); replacing the notebook invalidates state (same node numbers, other file, no undo carry-over); failed open keeps the current notebook; missing / text / empty / unsupported-schema (`user_version` 99) files refused with the core's messages and left byte-identical (and never created); external modification (second `Document` saves first) reported with the buffer kept, dirty, and the other version not overwritten; unwritable directory reported, still dirty, and the next Ctrl+S succeeds; Ctrl+Shift+S neither saves nor formats; Ctrl+E inline code; image paste creates no reference and nothing is written; remote/file/absolute/`..`/existing-local image references rejected by the store and left in the source; a copied fixture edited and saved leaves the master byte-identical; no Qt crate in the manifest.

*GUI (native Wayland, release binaries, scripted `wtype`/`ydotool`, `grim` screenshots, copy of a generated v3 notebook with a Czech/emoji/table note; `master.omatree` SHA-256 unchanged):* launch by path with a flat indented list and the first note shown; selecting Plán, Journal; live rendering of the Czech heading, text, emoji and the table (rendered grid); typing Czech with diacritics and an emoji; edit Journal, switch to Plán and back with the unsaved edit retained; Ctrl+Z in Plán undid only Plán (Journal text kept); Ctrl+S → title/status "No unsaved changes"; window closed; the file's `nodes` rows read back with only Journal's body changed (read with Python's `sqlite3` module); reopened in egui with the edit present; Ctrl+Shift+S showed the reserved message and changed nothing; the same file opened in the **Qt release app** showed the identical hierarchy (Projects / Plán / Ideas / Deep / Journal) and the Plán table source and Journal text intact, and the Qt file was not modified by opening; a corrupt file showed "No notebook is open" with the core's message. **Not GUI-tested:** mouse clicks on checkboxes or tables in this app (covered by the editor repository's tests), image paste in the GUI (automated only), a conflicting external save in the GUI (automated only), X11, other platforms, IME.

*Performance (release, headless, median; 3 KB-class notes and a 210 KB, 5,000-task note; `cargo test --release -p omatree-egui -- --ignored --nocapture timings`; one "switch" includes three settling frames):* switch small → small 0.5 ms, idle frame 0.2 ms, typing one character 0.4 ms; switch small → big 39.5 ms, back 1.6 ms, idle frame in the big note 9.0 ms, typing one character in the big note 25.3 ms; Ctrl+S with the write-through already done 1.3 ms. These match the editor repository's E3.1 figures, so the E3.1 rendering work is intact. The host adds no per-frame clone: the row list is built once, `set_body` runs only on a changed frame, and the idle comparison happens only when leaving a note or saving. No new optimisation was done.

**Data compatibility.** Reads and writes only through `Document`, so the file format is unchanged (schema 3, no migration beyond what core already does for older files on save). The egui host does not write the tree's expanded state, so the Qt app opens with collapsed parents when none was stored. Title and structure are unchanged because E4 has no rename, create, delete or move.

**Remaining limitations.** No autosave, Save As, New, Open dialog, recovery UI, Search, Trash, rename/create/delete/move, expansion or keyboard tree navigation; ~~closing the window with unsaved changes discards them silently~~ (fixed in E4.1); images are unavailable; the theme is the fallback palette; one notebook per process (an `open` API exists for later tickets but no UI calls it); path-dependency layout requirement (above); a conflict is reported but offers no Save As to keep the user's version yet.

**Recommended next ticket.** E8 core of the lifecycle (close with unsaved changes, autosave, Save As using the reserved Ctrl+Shift+S, conflict handling) is the next safety gap because closing currently loses edits; E5 (tree interaction) is the larger product gap. If E5 is chosen first, add the close-with-unsaved-changes guard from E8 beforehand. Decide the repository-sharing proposal above before either lands.

### E4.1 implementation record (reproducible editor dependency, safe lifecycle)

**Status: implemented and verified on `egui-integration` (not committed).** This supersedes the E4 statements about the sibling-path dependency, the "Save As is not available yet" shortcut and silent loss on close.

#### A. Editor dependency

`crates/egui-app/Cargo.toml` now has

    omatree_editor = { package = "omatree-egui-editor-spike", git = "https://github.com/Dorotabro/omatree-egui-editor-spike.git", rev = "30a95dd43064232dc1ac04315c8020e2307a223c" }

- Remote: `origin` of the editor repository; `git ls-remote` shows `refs/heads/editor-library` at exactly that commit (`main` is at `d376a20`, which has none of E2, E3, E3.1).
- Contents of `30a95dd`: `E3.1` (`30a95dd`) on `E3` (`02365be`) on `E2` (`712f3e2`) on the PDF export.
- The five fonts the PDF exporter embeds (`assets/fonts/LiberationSans-{Regular,Bold,Italic,BoldItalic}.ttf`, `LiberationMono-Regular.ttf`, referenced by `include_bytes!` in `src/pdf.rs`) are all tracked in that commit.
- Real package name `omatree-egui-editor-spike`, library target `omatree_editor` (renamed in the dependency key only). No source is copied or vendored; the editor repository was not modified.
- `Cargo.lock` pins `git+https://github.com/Dorotabro/omatree-egui-editor-spike.git?rev=30a95dd…#30a95dd…` (the only other lock change is Cargo unifying `unicode-width`). A test asserts the manifest has a `git`+40-hex `rev`, no `path`, no `branch`, and that `Cargo.lock` pins the same commit. The Qt binary name and packaging paths are untouched (`target/release/omatree`, root `Cargo.toml`).

**Clean-checkout verification.** A copy of the tracked and new files (`git ls-files -co --exclude-standard`) was placed in an empty scratch directory with **no** `omatree-egui-editor-library` sibling, and built with a fresh `CARGO_HOME` that contained only a copy of the crates.io registry cache (no `git/` directory, so the editor had to be fetched from GitHub):
`cargo build --locked` (Qt app + core) passed; `cargo build --locked --workspace` passed; `cargo test --locked -p omatree-egui` passed (34 tests). Cargo created `git/checkouts/omatree-egui-editor-spike-…`, i.e. fetched from the remote. (The scratch build shared this worktree's `target/` directory only to save time; sources and the Git checkout were the clean ones.) Not tested: a machine without network access; a private-repository credential setup (the remote was reachable here without special configuration).

Note that Cargo resolves every workspace member, so the Qt app's builds now also need access to GitHub for the editor (once; afterwards it is in `~/.cargo/git`). Packaging scripts that build offline must pre-fetch (`cargo fetch --locked`) or vendor.

**Updating the pinned editor.**
1. In the editor repository, commit the change on `editor-library` (or merge it) and **push**; the commit must be reachable on the remote.
2. Run `git ls-remote https://github.com/Dorotabro/omatree-egui-editor-spike.git editor-library` (or `git rev-parse HEAD`) and copy the full 40-hex SHA.
3. Replace `rev = "…"` in `crates/egui-app/Cargo.toml`, then `cargo update -p omatree-egui-editor-spike --precise <sha>` so `Cargo.lock` follows; review the lock diff for unrelated changes.
4. Run `cargo test --workspace` (the pin test fails if the manifest and lock disagree), and re-run the GUI smoke checks if the editor changed behaviour.
5. Commit manifest and lock together. Never switch to `branch =` or a path dependency in a commit.

#### B–D. Lifecycle (`crates/egui-app/src/lifecycle.rs`)

**eframe 0.37 close interception (investigated in `eframe-0.37.0/src/native/epi_integration.rs`, `egui-winit`, and tested).** A native close (window button, Alt+F4, compositor, `hyprctl dispatch 'hl.dsp.window.close()'`) arrives as `ViewportInfo::events` containing `ViewportEvent::Close`, readable with `ctx.input(|i| i.viewport().close_requested())`. Answering with `ViewportCommand::CancelClose` in the same pass keeps the window; eframe checks for it after `App::logic` and `App::ui`. `App::logic` runs even when the window is minimised or occluded (`update_logic_only`), so the guard and autosave live in `Host::logic`, called from `App::logic`. A `ViewportCommand::Close` sent by the app is fed back as another close request, so the host keeps a `quit` flag set only by a successful Save, a confirmed Discard or a clean notebook. egui's own quit shortcut goes through the same path.

**Behaviour.**
- Clean notebook: the close proceeds.
- Dirty (`Document::is_dirty`): `CancelClose` and a modal (`egui::Modal`): **Save / Discard / Cancel**. Before judging dirtiness and before every save the host flushes the editor (`commit_pending`, then `Document::set_body` if the buffer differs).
- Save: `Document::save`; success closes; failure leaves the window open, the document dirty and editable, shows the core's message in the dialog and adds a **Save As…** button.
- Discard asks a second question ("Discard changes?" with *Keep editing* focused and *Discard changes*); only that closes. Cancel and Escape return to editing; the document is untouched.
- Repeated close requests while any question is open keep that question and never close. While a dialog is open the editor is unfocused, so typing and Ctrl+S do nothing behind it. Keyboard: the safe or default button is focused; Tab/Shift+Tab cycle; Enter/Space activate; Escape cancels.

**Save and Save As.** Ctrl+S: `Document::save` (status "Saved" or the core's failure text). Ctrl+Shift+S (consumed before Ctrl+S, because egui matches Ctrl+S with Shift held) opens an in-app **Save As** dialog: a path text field pre-filled with `<dir>/<name> copy.omatree`, Enter or Save, Esc or Cancel. A relative path is resolved against the notebook's directory and `.omatree` is appended when there is no extension (`with_default_extension`). An existing target raises **Replace file** (*No* focused); refusal changes nothing; *Yes* calls `Document::save_as(path, true)`, which itself refuses a file that is not an OmaTree notebook (the dialog then shows "That file already exists and is not an OmaTree notebook, so it was not overwritten."). A new target uses `save_as(path, false)`. No separate persistence code exists. After success the document's path, window title and later saves move to the new file; the old file is left byte-identical.

*Why not a native file dialog:* the plan names `rfd` as unverified on Wayland (portal behaviour, licences, a large dependency tree). A typed path works on every platform, needs no dependency and is fully testable; a native dialog remains an E8 item. Open and New are not provided (there is no untitled notebook; the app only opens an existing file by command-line argument).

**Autosave (modelled on `qml/main.qml`).** Qt: 1 s single-shot timer restarted by every real mutation, only for notebooks with a path, only when dirty, success silent, the first failure of a document reported once, later ones quiet until a success, timer stopped while a dialog is open and resumed after Cancel. egui: the same 1 s (`AUTOSAVE_DELAY`) countdown restarted on every real `set_body` change (selection and caret moves, and switching, never start it), `Document::save` decides how much to write, status "Autosaved" on success, and the dialog pauses and resumes it. Differences: success shows "Autosaved" in the status bar (Qt is silent); **after an `ExternallyModified` conflict autosave stops entirely** until a successful save, Save As or reopen (Qt would retry on every later edit), and the status bar then reads "Unsaved changes (autosave stopped)". A failed autosave never clears dirty and is reported once ("Autosave failed: …"); a non-conflict failure is retried only after the next edit.

**Conflicts.** Nothing ever overwrites the other version: `Document` refuses and the host only reports. The message ("…changed on disk… Your changes are still here. Use Save As to keep your version, or reopen the notebook…") appears in the status bar, in the close dialog after a failed Save, and in Save As errors. A close after a conflict still asks; resolution is Save As (the way out used in the GUI check) or an explicit Discard.

#### E. Automated tests (egui-app: 34 passing, 2 ignored helpers; Qt 42, core 253 + 1 ignored unchanged)

All in `crates/egui-app/src/tests/lifecycle_tests.rs` unless noted; real `Document`, private temp copies, close requests injected as `ViewportEvent::Close` and `CancelClose` / `Close` commands interpreted the way eframe does.
1 clean close; 2 dirty close then Cancel (nothing written, still editable); 3 dirty close then Save (written, closes); 4 dirty close then Discard (two steps, file untouched, *Keep editing* returns); 5 save failure during close (unwritable directory: window kept, dirty, error shown, then Save As to a writable place closes with the edit saved and the original untouched); 6 external modification (autosave blocks and reports once, no retry, other version never overwritten, close still asks, Save As resolves); 7 Save As to a new file (schema-3 header, structure identical, later saves go to the new file, old file untouched); 8 overwrite refusal and confirmation (unrelated file untouched even after *Yes*; another notebook replaced only after *Yes*); 9 autosave writes after the quiet period and restarts on further edits; 10 autosave writes nothing without a change (bytes and mtime identical); 11 autosave failure retains edits, reports once, later edit recovers; 12 Save, close and Save As flush a buffer that is ahead of the document; 13 repeated close requests (also during the discard confirmation and Save As) never close; 14 Qt-compatible schema-3 round trip; 15 the pin test above plus the clean-checkout build described in A. Also: keyboard flow (Escape, Enter on the focused Save) and "typing and Ctrl+S are ignored behind a dialog". The older E4 tests keep passing on the new harness.

#### F. GUI verification (native Wayland, release binaries, scripted `wtype`/`ydotool`, `grim`; a copy of the generated v3 fixture; `hyprctl dispatch 'hl.dsp.window.close()'` as the native close request)

GUI-tested: dirty edit then window close shows the dialog and writes nothing; Escape cancels and editing resumes (autosave then writes); Discard asks again, *Discard changes* closes and the file is byte-identical to the original; Enter on the default button saves and closes; Tab twice reaches Cancel, Enter cancels (window stays); Tab wraps; Ctrl+S failure and a failed autosave in a read-only directory (status "Autosave failed…", "autosave stopped", edits kept); close then Save fails in the dialog with the red message and a *Save As…* button; Save As to a writable path saves, closes, original byte-identical, rescued file has the edit; Ctrl+Shift+S dialog, typed path, Enter, title changes to the new file, later edits go only to the new file; overwrite prompt for an existing notebook (*No* default, Escape cancels, *Yes, replace* replaces) and for an unrelated text file (refused with the not-a-notebook message, file unchanged); autosave after typing and waiting ~1 s with no Ctrl+S ("Autosaved", file written); external modification of the file by another process (Python `sqlite3` update) then editing: autosave stops with the conflict message, the other version stays on disk, closing shows the conflict text with *Save As…*, Save As keeps "my" version; the Save As result reopened in the egui app and in the Qt release app (same hierarchy, edited text, Qt left the file byte-identical).
Automated only: Discard-then-*Keep editing* by mouse, the Cancel button by mouse (Escape and Tab+Enter were used), repeated close requests, close while minimised, Alt+F4 and egui's quit shortcut (same code path as the tested request but not individually driven), Save As to a relative path, Windows/macOS/X11.

#### G. Validation (this machine)

`cargo fmt --all -- --check` pass; `cargo clippy --workspace --all-targets -- -D warnings` pass; `cargo test --workspace` pass (Qt 42 incl. 24 QML scenarios, core 253 + 1 ignored, egui-app 34 + 2 ignored); `cargo build --release --workspace` pass; Qt release app opened and displayed the edited notebooks; clean-checkout builds as in A.

#### Remaining for E8 and later

Native file dialogs (Open, Save As) and New/untitled notebooks; command-line-less start (an empty window with Open); Open replacing the current notebook with the same unsaved-changes guard; Qt's silent flush-on-close for file-backed notebooks (E4.1 always asks when dirty, as specified); recovery dialogs; theme-aware dialogs (E6) and the keyboard reference (E7); close handling for platform quit paths other than the viewport close (macOS Cmd+Q from the menu, session end); the unsaved-changes dialog when switching or replacing notebooks; a conflict flow beyond Save As (reload from disk). Dependency: the editor pin must be bumped by hand for editor changes (above).

### E5a implementation record (hierarchical tree and keyboard)

**Status: implemented and verified on `egui-integration` (not committed).** Drag-and-drop (E5b), Search, the Recovery dialog and the keyboard-shortcut reference are not part of it.

**Structure** (`crates/egui-app/src/`): `tree.rs` (the note tree as the shell shows it: every note in depth-first order with depth, parent, has-children, last-sibling and guide bits, plus the list of *visible* rows; rebuilt from `Document` only on a structure or title change, the visible list only when that or the expanded set changes), `ops.rs` (selection, expansion, create, rename, move, delete), `tree_ui.rs` (hand-painted, virtualised rows; keys; context menu; the editable title), `lifecycle.rs` (two new dialogs: Move to Trash, and an error message), `lib.rs`. No core change beyond E4's `depth_first` visibility. A frame never walks the notebook: only the rows in the viewport are touched (an idle frame with 6,300 notes, all open, measured 0.12 ms headless in release).

**What `Document` still owns.** Hierarchy, titles, bodies, the expanded set (`is_expanded`, `set_expanded`, `set_subtree_expanded`, `set_all_expanded`, `expand_ancestors`), Trash and checkpoints (`delete`, `move_node`), and the dirty rules. The host calls `create_default_root/child`, `rename`, `move_node`, `delete`; it never edits the hierarchy itself. The tree rows are a derived cache.

**Behaviour (parity reference: `TreePane.qml`, `main.qml`, the QML scenarios, and the running Qt app for keys the QML does not define).**

| Key / action | egui | Qt parity |
|---|---|---|
| Up / Down | previous / next *visible* row, no wrap; tree keeps the keyboard | same (verified in the Qt app) |
| Left / Right | collapse an open branch / open a closed one; otherwise nothing (Left does not move to the parent, Right does not descend) | same (verified in the Qt app) |
| Ctrl+Right / Ctrl+Left | open / close the whole subtree | same |
| PageUp / PageDown | by a page of rows, stopping at the ends | same effect (Qt's page size differs slightly) |
| Home / End | not handled | same (the Qt TreeView did nothing) |
| Enter | focus the body | same |
| Escape | nothing in the tree; from the title or body: commit (title) and back to the tree | same |
| F2 | focus the title with all of it selected | same |
| Delete | ask "Move to Trash?" (No is the default; Escape cancels) | same |
| Ctrl+N / Ctrl+Shift+N | new top-level note / child of the current one, title focused and selected; refused while a pending title is invalid | same |
| Alt+Up / Alt+Down | one place among siblings, selection kept, open state travels with the note | same (works from tree, title and body) |
| Shift+F10 | context menu under the current row | same; the Menu key is **not available**: egui 0.37 has no key for it |
| Context menu (right-click, selects the row first; empty space: tree-wide entries) | New note, New child, Expand/Collapse subtree, Expand/Collapse all, Move to Trash | same entries; keys inside the menu are **Tab / Shift+Tab and Enter** (arrow keys do not navigate egui menus) |
| Click row / marker | select / also toggle (Ctrl+click on the marker: subtree) | same; a double click does not toggle |

Tree keys act only while the tree has the keyboard (checked by tests: Down, Delete, Left, Ctrl+Right in the editor or the title never touch the tree). The application-wide commands (Ctrl+N, Ctrl+Shift+N, F2, Alt+Up/Down, Ctrl+S, Ctrl+Shift+S) work from anywhere and stand down while a dialog is open.

**Title editing.** The title above the note is a text field, written to the notebook only when committed: Enter (then on to the body), Escape (then back to the tree), leaving the field, or any action that leaves the note (select, create, save, Save As, close). A refused title (empty: "A note needs a name."; a sibling already has it: `A note named "X" already exists here.`, the Qt wording) shows an OK dialog, keeps the typed text, blocks the selection change / creation / save / close, and returns the keyboard to the field.

**Selection and focus.** Selecting by click, key or creation reveals the note (its ancestors open, remembered by the document); the selected row scrolls into view. The tree has the keyboard on open and after a delete; Enter goes to the editor, a new note or F2 to the title. A dialog returns the keyboard to where it was.

**Collapse and selection.** When a collapse hides the selected note, the nearest visible ancestor becomes current (as `keepSelectionVisible`), after the pending title has been committed; if that commit is refused the collapse does not happen.

**Delete.** The editor buffer is flushed first (Trash holds the latest text); a title typed for the note is dropped; `Document::delete` takes the checkpoint and the Trash entry; caret, selection and undo of **every** deleted note are forgotten (`forget_note`), so no undo step can reach another note. Afterwards the parent is selected as in Qt; **intentional difference:** for a top-level note, where Qt selects nothing, the next (else previous) top-level note is selected.

**Dirty and persistence.** Expansion is view state (`!is_dirty()`; written by any save, including Ctrl+S on a clean notebook). Create, rename, move and delete are content changes and start the 1 s autosave countdown (a move or delete also marks the recovery state, so the next save is the full one). **Intentional difference:** a *clean* close also writes pending expansion (best effort; a failure does not keep the window open) so the open/closed state is not lost; the Qt app drops it unless some save happens.

**Fixed along the way (an E4.1 gap).** Autosave fired only while something else (typing, caret blink) caused frames: eframe replaces the previous repaint request with each frame's, so with the tree focused the countdown never ended. `Host::logic` now re-requests the wake-up on every frame while the countdown runs (test and GUI-verified: a rename followed by idling autosaves).

**Verification.** `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings` pass; `cargo test --workspace`: Qt 42 (includes the 24 QML scenarios), core 253 + 1 ignored, egui-app 61 + 3 ignored (the 34 earlier tests unchanged except that the harness focuses the editor after a selection, because the tree now keeps the keyboard); `cargo build --release --workspace` passes. 27 new tests (`tests/tree_tests.rs`): hierarchy rows and guide bits; expansion scenario of `expansion.qml`; persistence across Ctrl+S and reopen; expansion never autosaves and a clean close keeps it; Up/Down incl. hidden descendants and no wrap; Left/Right/Ctrl+arrows; hidden-selection to ancestor; reveal on select; Enter/Escape focus; tree keys not firing while typing (editor and title); PageUp/PageDown; Ctrl+N and Ctrl+Shift+N; F2 and every way a title commits; refused titles (conflict, empty) blocking selection, creation and close with the Qt messages; pending title in save and close; Alt+Up/Down incl. boundaries, checkpoints and persistence; delete asks first (Escape, No, Enter decline), child, subtree, top-level neighbour selection, flush before delete, editor state forgotten, Ctrl+Z after delete; Ctrl+Z isolation across keyboard note switches; keyboard context menu is actually drawn and Escape closes it; autosave after structure edits; a 6,300-note tree; the wake-up request. Mutation checks: removing the per-frame repaint request fails its test.

*GUI (native Wayland, release binaries, `wtype`/`ydotool`, `grim`; nested fixture A(A1(A1a(Leaf)),A2), B(B1), C):* Qt keyboard behaviour probed first in the running Qt app (Right, Left, Down, PageUp, Home); egui: keyboard-only expansion (Right, Ctrl+Right), Down through nested rows, Left on a leaf (no-op), Ctrl+Left; guides and bold selected row; mouse: marker click (expands and selects B), row click, right-click menu (selects, correct entries); Shift+F10 menu under the row, Escape closes, Tab+Enter runs an item; Ctrl+N (title selected, typed name, Enter moves to the body); Ctrl+Shift+N child; refused rename (`A2`) with the error dialog, OK returns to the title, corrected name accepted; Alt+Up moved the note; Delete dialog (No default, Tab to Yes), parent selected, tree focused; save with Ctrl+S, native close, reopen: hierarchy and expansion intact, `expanded_nodes` rows as expected; the same file in the Qt release app: identical hierarchy and expansion, file left byte-identical; editing two notes with Escape/Down/Enter and Ctrl+Z undoing only the note shown (the other note's text kept); F2 title edit committed by clicking another row; idle autosave after a rename (status "Autosaved"); E4.1 features re-checked on this build: dirty close shows Save/Discard/Cancel, Ctrl+Shift+S opens Save As, autosave of typing.
**Not GUI-tested:** drag-and-drop (not implemented); PageUp/PageDown, Alt+Down/Up *at the boundaries* and rename-by-blur with the mouse (automated only); the context menu *Expand all / Collapse all / Move to Trash* items by mouse (automated through the same commands); a notebook with thousands of notes in the real window (headless only); X11, macOS, Windows, IME.

**Differences from Qt, all deliberate or forced by egui 0.37:** no Menu key (Shift+F10 only); menu navigation by Tab rather than arrows; top-level delete selects a neighbour; clean close persists expansion; no Edit/Preview mode (D4), so Enter simply focuses the editor; no `+ delete`/`edit`/`preview` header buttons; Recovery is Ctrl+Shift+R or the tree menu (E7b), Search a field opened by Ctrl+F (E7a); double-click does not toggle; the tree shows the title in the note's header as an editable field (as Qt) but there is no pointer-hover reveal.

**Remaining for E5b:** done, see below. Other follow-ups: arrow-key navigation inside the context menu if egui gains it; the Menu key if egui adds `Key::ContextMenu`.

### E5b implementation record (tree drag-and-drop)

**Status: implemented and verified on `egui-integration` (not committed).** No core, schema, dependency or editor-library change; the pinned editor rev is untouched.

**Qt behaviour (`TreePane.qml` drag section, `NotebookModel::{can_drop,drop_node}`, `moveexpanded.qml`), the parity reference**
- Zones by the pointer's height in a row: upper quarter = *before* it, lower quarter = *after* it, middle = *child of it* (appended as the last child); empty space below the rows = *end of the top level*. Just under an **open** parent "after" means its **first child**.
- A drop is validated by `Notebook::can_reparent` (no cycle, no equivalent title already at the destination); same-parent reorders are never a conflict. A refused target is drawn as a 2 px danger outline round the row under the pointer, and releasing there does nothing. Dropping where the note already is is a no-op (`plan_move` gives `None`: no checkpoint, no dirty).
- A drop is one `move_node` (one checkpoint "Before moving …"); the subtree and every note's content and recorded expansion travel unchanged. The moved note is then selected and revealed, which opens the path to it; nothing else about expansion changes (`moveexpanded.qml`).
- Qt has **no** auto-expand on hover, **no** auto-scroll, no Escape cancel (a release outside the tree cancels). Those three are specified here instead.

**egui implementation**
- `drag.rs` (pure, no egui): `zone` (the Qt zones above), `verdict` (`Document::plan_move` → `Move{parent, position}` / `NoOp` / `Refused`; slots are "before row N with the note still in place", converted to a final position as Qt's `drop_node` does), `locate` (memoised while the pointer stays in one zone, so `plan_move` is not asked every frame), `scroll_speed`. `Drag` is the in-flight state; the notebook is untouched while it exists.
- `tree_ui.rs`: rows are `click_and_drag`. A drag begins only when egui reports a drag **and** the pointer is ≥ 6 px from where it was pressed (egui alone also calls a press held ~1 s in place a drag). `drag_frame` runs once per frame: Escape cancels (the drag is ignored until release), the target follows the pointer, edge scrolling, hover expansion, and release/loss ends it. It returns commands (`DragOpen`, `EndDrag`, `Drop`) that are applied after drawing, like every other tree command.
- Indicator (exactly the move that would happen): before/after/end = 2 px accent line at the row's top/bottom/below the last row; child = 2 px accent outline of the row; refused = 2 px danger outline (danger line for a refused end-of-list); a **no-op target shows nothing** (Qt draws a line there; hiding it is more honest because nothing will happen). The dragged row is drawn at half strength; the cursor is *grabbing*, or *not allowed* over a refused target. No animation.
- `ops::drop_note`: commit a pending title (an invalid one blocks the drop, as it blocks any selection change), flush the editor buffer, `Document::move_node`, rebuild the tree, select + reveal the moved note, tree keeps the keyboard. `edited` is set only if the move changed something, so only a real move starts the autosave. Per-note editor state is keyed by note id and is untouched.
- Rows now use stable ids (`make_persistent_id`) instead of `Id::unique`, which egui's drag/focus memory cannot follow across frames (a precaution; see GUI evidence).

**Auto-expand (transient).** A collapsed note whose *middle* zone the pointer rests on for 0.6 s is opened (not selected, no editing; its own subtree is excluded, since a drop there is refused). Moving away before the delay resets it. Hover-opened notes are **closed again when the drag ends** (drop or cancel), so a drag leaves recorded expansion exactly as it was; a successful drop then reveals the moved note, which re-opens the path to it (view-only, as in Qt). A note stays open while Escape-cancelled drag still holds the button, until release.

**Auto-scroll.** While dragging, pointer within 28 px of the top/bottom of the tree viewport scrolls it, speed rising linearly to 1,500 px/s at the edge; it stops when the pointer leaves the edge zone or the viewport, or the drag ends. The target is recomputed every frame from the current scroll offset, so it is accurate as rows scroll or open.

**Dirty/persistence.** Expansion stays view-only; only a real move marks content dirty (checkpoint + autosave + close guard); no-op, refused, cancelled and outside-release drags change and dirty nothing.

**Tests (25 new frame-driven tests in `tests/drag_tests.rs`, plus the existing 61; 87 passed + 4 ignored in the egui crate).** All use real pointer events against a real `Document` on disposable files: threshold vs click (3 px wobble, long press, real move), an editor-side drag does nothing, before/after, first-child-under-open-parent, same-parent reorder both ways, cross-parent, to/from top level, end-of-list, whole subtree with note bodies, self/descendant refusals (five zones, no checkpoint added), duplicate-title refusal, five no-op cases (clean, no autosave), Escape, release outside, indicator ↔ move consistency over 4 dragged notes × 14 zones (row index, kind, resulting parent/position), hover expansion after the delay and not before (not selected, title field untouched, closes after cancel), cancelled pending expansion, no expansion over own subtree, drop into a hover-opened note, auto-scroll both directions and stopping, drop after scrolling lands where the indicator was, save/reopen, autosave after a drop, **`moveexpanded.qml` ported** (checkpoint count, recorded expansion kept, reopen identical), text/caret/undo preserved for the moved note and other notes untouched, cancelled/refused drag leaves the editor alone, keyboard (Down, Alt+Down) after a drop, and a 6,300-note drag.

**Performance.** Release, headless, 6,300 notes all open: 0.11 ms per drag frame (idle frame 0.13 ms). The notebook is not touched per pointer move (verified: not dirty, tree not rebuilt); SQLite is only written by the drop and the normal autosave/save.

**GUI evidence (native Wayland, release build, disposable copies; nested fixture A(A1(A1a(Leaf)),A2) | B(B1) | C and a 120-note flat one).**
- Seen working: ordinary click vs drag; before / inside / end-of-list drops with their indicators; whole-subtree move (A1 with A1a and Leaf into C); child drop selecting and revealing the moved note; refusal outlines (self, descendant, "before a descendant") with the notebook byte-for-byte unchanged afterwards; release over the editor cancels; Escape removes the indicator and cancels; hover auto-expansion after ~1 s and its undo on cancel; drop into a hover-opened note (stays open, moved note selected); auto-scroll down, stop, and back up with the target following; Down/Up/Alt+Up after a drop; Ctrl+S, close, reopen: same hierarchy and expansion; the same file in the Qt release app shows the identical hierarchy and expansion and was left unchanged (checksum).
- **Not exercised in the GUI:** the 6,300-note notebook in a real window (headless numbers only); a duplicate-title refusal by mouse (headless only); pointer drags with touch/pen; X11, macOS, Windows; high-DPI scaling other than 1.0; dropping onto the footer area.

**GUI verification found no app bug.** The first GUI attempts misbehaved because of my own helper script (pointer moves not converging; the window origin had changed between runs), and were redone with a corrected script. Switching the rows to stable ids was a precaution: drags worked with both kinds in the tests, and I did not show it was necessary in the GUI.

**Differences from Qt:** no-op target shows no indicator; Escape cancels; hover auto-expands; edge auto-scroll; dragged row dimmed and a cursor change; press-and-hold in place is not a drag; a drag is ignored while it is cancelled until the button is released. Selection range of the previously edited note collapses when the tree takes the keyboard (egui `TextEdit` behaviour, the same as any click on the tree since E5a); text, caret and undo history are preserved.

**Remaining limitations:** no keyboard-driven cross-parent move (only Alt+Up/Down among siblings, as in Qt); no drag ghost label; no auto-scroll acceleration beyond proximity; drop indicator for before/after is full-width rather than indented to the destination depth (as Qt).

### E6a implementation record (unified semantic theme)

**Status: implemented and verified on `egui-integration` (not committed).** No visual polish (E6b), no tree/drag changes, no editor feature, schema or dependency change, and the pinned editor repository is untouched.

**Audit (from the source)**
- *Core* (`omatree_core::theme`): 14 semantic roles (`background, surface, surface_raised, foreground, muted_foreground, accent, selection, selection_foreground, border, danger, danger_foreground, accent_secondary, positive, warning`); `select(omarchy, scheme)` = valid Omarchy palette, else OmaTree Dark/Light by system scheme (Dark when unknown); `omarchy_colors_path` honours `XDG_STATE_HOME` then `$HOME/.local/state/omarchy/current/theme/colors.toml`, Linux only; `OmarchyWatcher` re-resolves the path on every poll, needs the same new content on **two consecutive polls**, keeps the current palette through missing/unreadable/malformed files (one warning), and takes `mode` (case-insensitive) before background brightness (< 0.5).
- *Qt*: `Theme` singleton = `select(omarchy, scheme)`; `main.qml` polls every 500 ms; the system scheme is `Qt.styleHints.colorScheme`; every colour a QML item draws is one of the roles.
- *Standalone editor* (`omatree_editor::theme::Palette`, `host_theme.rs`): a *derived* palette (canvas, text, muted, H1–H6, link, code, quote, rule, selection, reveal, image, menu, separator) built by `Palette::from_toml`, which also enforces contrast (text 4.5, headings 4.5, link/muted/reveal 3, quote text 4.5, rules 1.8, text on selection 4.5, selection vs canvas 1.25). Its own host helper polls every 1 s with single-sighting acceptance and reads `$HOME`/`USERPROFILE` only.
- *egui host before E6a*: no theme code at all; `Palette::fallback()` (`#1e1e1e` grey) installed once through the editor's `apply`. Hard-coded colour values in the host: none, but the tree took its colours from egui's default visuals (`hyperlink_color`, `faint_bg_color`, `error_fg_color`), so they could never follow the palette.
- *Differences Qt vs standalone editor (resolved by routing everything through core)*: path rules (`XDG_STATE_HOME` vs `HOME`/`USERPROFILE`); confirmation (two polls vs one); poll interval (0.5 s vs 1 s); `mode` matching (case-insensitive vs exact); brightness threshold (0.5 on a simple luma vs 0.4 on relative luminance); `#rgb` shorthand (editor only); fallback palettes (OmaTree Dark/Light vs `#1e1e1e`); optional colours (core maps surface/danger/etc., editor derives its own roles).

**Pipeline** (`crates/egui-app/src/theme.rs`): `OmarchyWatcher` (core) → `select(omarchy, system scheme)` (core) → `ThemeState` (host) → `visuals()` for egui and `editor_palette()` for the editor. One watcher, one selection, one place that computes colours. `ThemeState::update` runs once per frame from `Host::logic`: reads `ctx.system_theme()`, polls the watcher on a 0.5 s beat (re-requesting the wake-up each frame, because eframe keeps only the last repaint request), and installs a palette only when the chosen one differs from the one in use. `App::new` follows `default_omarchy_colors_path()`; `Host::new()` follows nothing (tests and non-Omarchy hosts get the built-in palettes and no wake-ups); `Host::follow_omarchy(path)` is the explicit switch.
- **Core change:** `Palette` gained `dark: bool` (the theme's explicit `mode`, else background brightness). Without it the explicit mode would be lost after parsing and a toolkit could only guess from colours. Qt ignores the field; 1 core test added; all existing core theme tests unchanged.
- **egui theme is pinned to the palette:** the host calls `ctx.set_theme(Dark|Light)` according to `palette.dark` and fills that style slot, so a system light/dark flip can never switch egui to a style slot that was never filled in (the system scheme only feeds `select`). `set_theme` also keeps the native window decorations in step.
- **Editor palette:** built with the editor's own constructor (so its contrast safeguards are kept, none re-implemented) from the core roles: `dark→mode, background, foreground, accent→accent (H1, links), accent_secondary→magenta (H2), positive→green (H3), selection, muted_foreground→dark_foreground (muted text, revealed delimiters), selection_foreground→bright_foreground, border→muted (image borders)`. The editor receives the finished palette in `EditorInput`; it reads no files or environment.

**Mapping (Qt parity)**: `background` central panel and title; `surface` tree panel, status strip, text-input background; `surface_raised` menus, popups, dialogs, hover/faint rows and button fill; `border` hairlines, separators, tree guides, window stroke; `muted_foreground` weak text and tree markers; `foreground` all text and the text cursor; `accent` links, focus accents, tree focus bar, drop indicators; `selection` / `selection_foreground` selected rows and the pressed-button state; `danger` refusal outline, error text and the destructive "Yes" (filled with `danger`, labelled `danger_foreground`); `warning` egui warnings. `override_text_color` is not set, so buttons colour their own text. Widget fills for hovered/open states are the raised surface mixed 10 % towards the foreground (a derived value, the only one in the chrome).

**Caches and fonts.** A palette change does the following and nothing else: `set_theme` + `set_visuals_of` once, one repaint; the tree and its virtualised rows are not rebuilt (colours are read at paint time from the host's palette); fonts are not touched, so the E3.1 geometry, viewport culling and bold-family installation are as before; no editor widget state is reset (caret, selection, undo and scroll live in egui memory and the editor's per-note state). The editor itself compares the palette it receives with the one its caches were built under and **drops its table galley cache on any difference** (cached galleys point into the font atlas, which egui rebuilds when the light/dark glyph gamma changes; this is the stale-table-glyph regression). Text/layout caches that do not depend on colours are kept. Nothing is parsed or read per frame; the filesystem is read once per 0.5 s beat only while an Omarchy file is being followed.

**Tests (17 new in `tests/theme_tests.rs`, 1 in core; workspace: Qt 42, core 254 + 1 ignored, egui 104 + 5 ignored).** Mapping of every role into the visuals (4 palettes) and the host actually installing it and pinning egui's theme; the same palette into the editor with the editor's contrast rules asserted on dark, light, and two Omarchy-derived palettes, plus chrome text legibility on every surface; fallback without Omarchy by system scheme and with an absent file; Omarchy beating the system scheme and an explicit `mode` beating background brightness; a gap (missing theme dir), unchanged return, three kinds of broken replacement and a valid theme after them, through the real core watcher on atomically replaced theme directories (no fallback flash, one application for the one real change); no application on unchanged frames or polls; wake-ups only while watching and only on the poll beat; live switching keeping text, caret and selection, tree rows, expansion, selection and scroll origin, dirty state and autosave state; per-note undo across a theme change; an open delete dialog surviving a switch and still working; a note with headings, link, code, table, tasks, emoji and an unavailable image rendering through four switches byte-for-byte unchanged with fonts/scale unchanged; no write to the file on disk because of a theme; the `visuals()` cost. Parsing and path rules are not re-tested (core's tests cover them).

**GUI evidence (native Wayland, release build, isolated fixture via `XDG_STATE_HOME`, copied notebook; the user's real Omarchy theme was never touched).**
- Seen: dark startup; light startup (reopen in light); dark→light→dark live switching with an atomic theme-directory replacement; a note with a level-1/2 heading, link, inline and block code, a table, task checkboxes, a quote, emoji and an unavailable image in both themes, with table glyphs and checkboxes crisp after each switch; a selected text range (14 characters) identical before and after; the tree (selected, expanded, focus bar) restyled in place; the Shift+F10 context menu staying open and restyled; the delete confirmation staying open and restyled, with its danger-filled "Yes"; the theme directory missing, a malformed `colors.toml` and one lacking the required colours each leaving the last good theme on screen, then a valid light theme being picked up (one warning logged); no-Omarchy start (empty state dir) showing OmaTree Dark because the desktop prefers dark; the window title never showed the unsaved marker because of a switch; the notebook's nodes were byte-identical (hash) after a session of switches; an edit made while switching saved and reopened correctly.
- **Not exercised in a GUI:** a system colour-scheme change at run time (egui reports it only through the windowing layer; covered headlessly by feeding `RawInput::system_theme`); a non-Omarchy *light* desktop; Windows and macOS (the platform rule is `default_omarchy_colors_path()` returning none there; unit-level only, and not testable on this Linux host); X11; high-DPI.

**Qt parity differences.** egui's chrome is egui's widgets restyled, not Qt Quick Controls, so shapes differ (E6b); the system scheme comes from egui/winit, not `Qt.styleHints`; hover/open widget fills are a derived 10 % mix (Qt has per-item alphas); the editor's text colour is the core `foreground` unless the editor must adjust it for legibility on the selection (never for the built-in palettes; the test asserts equality in that case). Omarchy-derived `danger`/`danger_foreground` can be a weak pair (a light `bright_foreground` on a pastel `red`, as in a Tokyo-style theme, measured 1.64:1 on the delete button); Qt has the same mapping, so it is core behaviour, noted for E6b rather than patched here.

**Remaining limitations / E6b readiness.** Spacing, typography, alignment and hierarchy are untouched and every colour now comes from one palette, so E6b can restyle without touching the pipeline. Open for E6b: the weak destructive pair; the status strip and title still use egui's default paddings; the egui window decorations follow the palette via `set_theme` but their look is the compositor's.

### E6b implementation record (visual polish and cohesion)

**Status: implemented and verified on `egui-integration` (not committed).** Presentation only: no behaviour, notebook, schema, editor, search/recovery/lifecycle, dependency or Qt change, and the pinned editor repository is untouched. It builds on the E6a pipeline (no second styling system): colours still come from `omatree_core::theme`; this ticket adds *derived presentation colours* and the host's spacing rules.

**Visual issues found (baseline screenshots of the E6a build at 1000x700 and 520x480, dark/light/low-contrast, rich, nested and long notebooks):**
1. The title started ~8 px left of the Markdown body (the title field's margin was ignored), so title and body shared no axis.
2. The title was regular weight and read lighter than the body's own headings; a full-bleed rule under it was heavy, especially in light themes.
3. The tree's first row sat higher than the title text; rows started 8 px inside the pane, so the selection band floated.
4. Selected-with-focus and selected-without-focus differed only by the colour of a 3 px bar; hover and selection fills were alpha blends that nearly vanished in some themes.
5. "+ note / + child" were bordered pills (a toolbar look); the two bottom bars had different heights and the footer had a rule the status bar did not.
6. The status bar never changed: a stale message stayed for the whole session (a rename error was still there after a successful rename), "No unsaved changes" and "Images are not supported yet" were always on, and in a narrow window the right-hand items pushed the message out (it showed as "aved changes"). Long messages were one clipped line.
7. Dialogs were cramped (heading, text and buttons touching, small buttons) and had unequal widths; the destructive answer was "Yes" on a colour alone, with a 1.64:1 label in Tokyo-style themes.
8. A rename refusal appeared only in a modal and in the status bar, never next to the title.
9. In a 520 px window the tree took half the window; the tree had no width limits.
10. Text roles of faint themes (muted hints, secondary text, disabled menu entries) fell to 1.5-2.5:1.

**Spacing rules (`crates/egui-app/src/layout.rs`, the only place with measurements):** `GUTTER` 16 = the editor's own document padding (`DOC_PAD`, unchanged), the shared left axis of title, title rule, Markdown body and status text; `BAR_H` 32 for the tree's command strip and the status bar (their top hairlines are one line across the window); `TREE_TOP` 10 / `TITLE_TOP` 10 put the first row's text and the title text on one centre line; `ROW_H` 26, `INDENT` 16 and the tree's inner left inset 8 are unchanged, so drag geometry, drop zones and hit-testing are as in E5b; the tree pane has no inset (rows and selection run edge to edge) and is resizable between 150 px and 60 % of the window (starting at 260, or 40 % of a small window).

**Changes.**
- *Title:* bold (the editor's bold face), 20 px, in the text colour, on the shared axis (an explicit frame around the field: an egui text edit without its own frame ignores its margin). Under it one reserved 15 px line for this title's validation message (danger colour, associated with the field, full text in a tooltip; the editor never moves when it appears), then an inset hairline: quiet normally, accent while the field has focus, danger while the title is refused. The message is state of the title only (`title_error`, cleared when the title is edited, accepted or another note is loaded); the modal, the status string and every transition are unchanged.
- *Tree:* guides are a quieter mix of border and surface; selected rows have two states (strong fill with an accent bar while the tree has the keyboard, lighter fill with a muted bar when it does not); hover is a visible lift of the surface towards the text colour; a title cut by the row now shows in full in a tooltip. Disclosure triangles, elbows, virtualisation and every key/drag path are untouched.
- *Sidebar actions:* flat text actions on the bar (secondary text, a fill and an accent outline under the pointer or the keyboard focus, disabled = muted and inert). Tooltips and shortcuts as before; reachable with Shift+Tab from the tree.
- *Status bar:* left, what just happened (up to two lines, ellipsis, whole text in a tooltip; failures in the error colour); it fades after 6 s (15 s for a failure) and restarts when the message or the saved state changes (`status()` still returns it). Right, the document's state: "● Unsaved changes", or "● Autosave stopped" with the reason in a tooltip; a clean document shows nothing. The image notice appears only for a note that mentions `![`, and only where there is room. A refusal already shown beside the title is not repeated in the bar. Autosave timing, conflict handling and dirty logic are untouched.
- *Dialogs and menus:* one frame, width rule (clamped to the window), heading style and button spacing for all six dialogs; destructive answers (`Move to Trash`, `Discard changes`, `Yes, replace`) are filled with the danger colour and *say what they do*; the safe answer keeps the first focus, Escape/Enter/Tab and every transition are unchanged. Context menus have a minimum width, roomier entries and a danger-coloured "Move to Trash".
- *Colour (`theme::Chrome`, computed once per palette change, never per frame):* every colour is a palette role, a mix of two roles, or a role pulled toward black/white only as far as needed (`readable`); a role that already reads is returned unchanged, so ordinary themes keep their own colours. Floors: text 4.5:1 on background, surface and raised surface; secondary text 4.5:1; hints/markers 3:1; accent 3:1; error and warning text 4.5:1; selected-row text 4.5:1 on both selection fills; faded disabled text 2.5:1 (egui fades toward `noninteractive.weak_bg_fill`, which is lifted just enough). The destructive label is the first of `danger_foreground`, `background`, `surface`, `foreground` that reaches 4.5:1 on the danger fill, else the first pulled until it does (black/white always give >= 4.58:1). `omatree-core::theme` was not changed.

**Colour decisions / measurements.** Tokyo-style fixture: `bright_foreground #c0caf5` on `red #f7768e` was 1.64:1; the label is now the theme's own dark `background` at 6.9:1. Built-in dark/light and Latte keep `danger_foreground`. Verified for seven palettes (dark, light, Tokyo, Latte, a low-contrast fixture, a mid-grey danger, "faint everything") in `polish_tests`. Deviation from E6a: `weak_text_color` is now the 4.5:1 secondary colour (was `muted_foreground` raw) and the egui text/stroke colours carry the floor.

**Tests (26 new in `tests/polish_tests.rs`, 1 updated in `theme_tests.rs`):** read from the painted shapes of real frames, not pixels: title/tree-row centre line; title, body and status text on one axis at 1000, 640 and 420 px; bars' hairlines on one line without a gap; indentation step and guide columns at every depth; selected/hovered/focused fills and the bar; long titles (ellipsis, inside the pane, one line); 14-level nesting; virtualisation (only rows in view painted with 6,300 notes); small windows (520, 360, 300 px, and shrinking); long status (two lines, ellipsis, error colour); document state vs feedback; feedback fading; a refused title shown once beside the field; destructive contrast and legibility floors over seven palettes; `readable` only changes what does not read; dialog and theme change while open; keys and clicks on painted rows; dialog default focus/Escape/Enter/Tab; the keyboard-opened menu; drop indicators on row edges (before/after, inside, refused); a whole drag still moves; hover/resize/menu/theme change leave content, dirty state, disk bytes and the editor selection alone; the rich note painted on the editor's axis in wide and narrow windows; flat actions by keyboard and disabled. Existing E5a/E5b/E6a tests pass unchanged except one assertion in `every_core_role_reaches_the_egui_visuals`, which now states the floor rule.

**GUI verification (native Wayland, release binary, isolated `XDG_STATE_HOME` fixtures, copied notebooks; the desktop theme was never touched; floating windows resized by `hyprctl`; ydotool/grim).** Before/after at identical sizes (1000x700 and 520x480) for dark, light and low-contrast themes, with the rich-Markdown, nested/long-title and 120-note notebooks. Exercised after the changes: tree keyboard navigation (down/right/ctrl-right), selection, F2 rename with an empty title and a duplicate title (inline message, red rule, modal, no repeat in the bar), Shift+F10 menu, Delete confirmation (Cancel focused), Save As and the overwrite question, an autosave conflict (two-line failure message, "Autosave stopped"), the unsaved-close dialog and the discard confirmation (Enter = "Keep editing"), live theme switching with the delete dialog open (dark -> light -> low-contrast, dialog stays and restyles), mouse hover with the full-title tooltip, and drag-and-drop (before-line, inside outline, drop). Notebook files used were scratch copies; no real notebook was opened.

**Subjective improvements (not measured):** the title now reads as the note's heading and lines up with the text under it; the sidebar and the bottom bars feel like one frame; the status bar is calm when nothing is happening; dialogs are easier to read and the destructive answer is unmistakable.

**Performance (release, headless, 6,300 notes; three runs, medians of 20 frames).** Idle tree frame: before ~123-213 us, after ~113-120 us; drag frame: before ~83-92 us, after ~75-116 us (no regression; noise level). Editor (E3.1) timings unchanged: switch to a 5,000-line note 41 ms -> 39 ms, typing in it 25.5 ms -> 25.4 ms, small note 0.5 ms. No per-frame filesystem access, parsing or string building was added; `Chrome` is a `Copy` struct copied per frame, and the status line allocates only as the old label did. The pre-existing continuous idle repaint (the full UI asks for a repaint on every idle frame) is unchanged and not addressed.

**Deviations from Qt.** The destructive answers and "Cancel" are worded as actions ("Move to Trash", "Cancel") instead of "Yes"/"No"; the status bar hides a clean document's state and fades feedback; the title carries its own validation line; text colours are floored for legibility where the Qt app takes the theme's colours as they are.

**Remaining visual shortcomings / not verified.** Dialog buttons stay left-aligned (right-aligning would change Tab order); very deep trees truncate titles in the default 260 px pane (tooltip shows them; the pane can be widened); ellipsis tooltips need a pointer (no keyboard equivalent); the 2.5:1 floor for disabled text is below the 4.5:1 of live text by design; window decorations are the compositor's. Not exercised: X11, Windows, macOS, high-DPI, a run-time system-scheme flip in a real window, IME. Screenshots live in the session scratchpad and are not committed.

### E7a implementation record (Search and keyboard navigation)

**Qt baseline** (read from `SearchPane.qml`, `SearchField.qml`, `TreePane.qml`, `main.qml`, `tests/qml/search.qml`, `searchheader.qml`, and run side by side with the egui build):
- A permanent field at the top of the pane; Ctrl+F focuses it with the last query selected. An empty query shows the tree; any query replaces the tree with the result list (the tree, its expansion and the selection stay underneath).
- The searching is `omatree_core::search::search` over the in-memory notebook: trimmed, lower-cased substring in title or body; title matches first, then body-only matches, each group in tree order; one result per note; three lines per result (title, `A › B` path, snippet).
- Up/Down move a highlight (the first result at a new query; kept in place on a refresh), Enter chooses it, a click chooses. Choosing clears the query, selects the note by looking it up again, reveals it (expands ancestors) and focuses the tree. A stale result gives an invalid index and nothing is selected. Escape clears a non-empty query (focus stays in the field); with an empty query it returns to the tree. Results refresh after a 120 ms debounce when the notebook changes under an open search. New/Open clear the query. Searching never dirties, mutates, checkpoints or autosaves.
- Qt has no explicit rule for an invalid pending title when a result is chosen: the editor commits the title on leaving it, so the rename is refused there.

**egui implementation** (`crates/egui-app/src/search.rs`, drawing from E6b's `Chrome` and `layout.rs`):
- Ctrl+F opens a one-line field on top of the sidebar (`/` prompt, hint "search notes", rule under it that turns accent while focused) and focuses it with the old query selected. Not a toolbar: an empty field that loses the keyboard closes again.
- Results replace the tree below the field while the trimmed query is non-empty; an empty query shows the tree. Rows are painted by hand and virtualised (cumulative tops, binary search for the first visible row), two lines (title, snippet) or three (title, path, snippet) for a note inside a branch, with the same selection band, accent bar, hover fill and bold title as the tree. A path too long for the pane loses its *outermost* ancestors (`… › Level 11 of …`), so the parent stays visible. "No matching notes" when nothing matches.
- Keys act only while the field has the keyboard: Up/Down move the highlight (clamped, no wrap), Enter chooses, Escape dismisses (query cleared, field closed, focus to the tree). Tab and Shift+Tab are untouched. The focus-lock filter keeps egui from using Up/Down/Escape for widget navigation. Ctrl+F is ignored behind a dialog and during a drag; the editor has no Ctrl+F binding.
- Choosing (Enter or click) calls `Host::select` (which commits the pending title first and refuses when the notebook refuses it, leaving Search open and the selection where it was), then closes Search, reveals the note (`doc.expand_ancestors`, scroll to it) and gives the tree the keyboard. Hierarchy logic is not duplicated. Caret, selection and undo stay per `NoteKey`; nothing moves between notes.
- A request for the tree while results are showing (Escape from the editor, a dialog closing) goes to the results' field instead, because the tree is not on screen.
- Invalidation: a changed trimmed query searches at once (a body edit, create, rename, move, delete and open each mark the results stale: structure on the next frame, bodies after 120 ms, at most one scan per window however many keys). If the last scan took more than 8 ms the notebook is "slow" and a new query also waits for a pause (each key restarts the wait, as in Qt). Enter always searches the typed query first, so it can never choose from a list of an older query. Idle frames scan nothing (tested: counter).
- Stale results: `activate_result` looks the note up again; a deleted note selects nothing and refreshes the list.

**Intentional differences from Qt:** the field exists only after Ctrl+F (ticket: no persistent toolbar) and closes when left empty; one Escape dismisses Search entirely (Qt: first clears the query, second returns to the tree); Escape from the editor with results showing lands in the field; the path is shortened from the left; a new query searches immediately in a notebook that scans quickly (Qt always waits 120 ms); no F-key hint line and no footer entry (as Qt, since E5).

**Tests:** 35 new in `tests/search_tests.rs` (+1 ignored `search_timings`) covering all twenty areas of the ticket on real frames: empty/blank query (no scan), title, body and snippet, title-first order, Czech case folding and emoji, no-result state, invocation and dismissal, Up/Down/Enter, Escape and focus, Tab/Shift+Tab, editor keys not stolen, modal and drag guards, collapsed and deeply nested notes revealed and scrolled into view, mouse choice, per-note caret and undo, unsaved and further-typed text, scan counts, rename, create, delete, move/reorder identity, stale results, invalid pending title (keyboard and click), dirty/disk/checkpoint/Trash untouched, replacing the notebook, legibility in four themes with a live theme change. Existing E5/E6 tests pass unchanged (`Focus` gained a variant; the test helper `has_focus` handles it).

**GUI verification** (native Wayland, Hyprland 0.56.2, release build, disposable copies; the user's Omarchy theme and notebooks untouched; the Qt build run side by side): Ctrl+F; title and body results with paths and snippets; a note inside a collapsed 12-level branch chosen by keyboard (Enter) and by mouse click, branch revealed and scrolled to; Down/Up highlight; typing in a note and finding the text before the autosave; rename while results were showing (the results followed); Escape; Ctrl+F from the editor; invalid title then Ctrl+F (the refusal dialog opens, the title and selection stay); dark, light, 1000×700 and 520×480; a 6,300-note notebook with 6,000 results and a jump to the last note. Screenshots are in the session scratchpad (not committed).

**Performance (release, 6,300 notes, 4-line Czech bodies, ~2 MB):** the frame in which a query is scanned and drawn ~35–42 ms (the scan is `omatree_core::search`, unchanged: a lowered copy and an origin table per body); idle frame with the tree 0.08 ms, with 6,000 results open 0.07 ms; Down through results 0.09 ms per key; typing in a note while 6,000 results show: median 0.13 ms, one ~42 ms scan per 120 ms window; choosing the result in the last note of a collapsed tree 0.9 ms (Enter frame) then 0.1 ms. Idle CPU with a text field focused is ~9% of a core in this app whether the field is Search, the title or the editor (pre-existing continuous repaint while a TextEdit has focus; not changed here; ~0% with the tree focused). A faster scan would need `omatree-core` work (an ASCII fast path in `find_in_body`); not done because nothing here needed it.

**Remaining / E7b:** Trash and checkpoint (Recovery) dialogs, themed dialog polish for those, F1 keyboard reference (should list Ctrl+F), `Escape` conventions in `escape.qml` for the new surfaces, match highlighting inside titles and snippets, Page Up/Down in the results, a result count for assistive technology, IME composition in the field (unexercised), X11/Windows/macOS (unexercised), high-DPI (unexercised), and Search after a restored Trash entry or checkpoint (no restore flow exists yet).

### E7b implementation record (Recovery, F1 reference, Search additions)

**Qt baseline** (read from `RecoveryDialog.qml`, `main.qml`, `tree_model.rs`, `KeyboardShortcuts.qml`, `Keymap.qml`, `recovery.qml`, `recoverykeys.qml`, `dialogs.qml`, `escape.qml`; the egui-saved notebook was opened in the Qt build and its Recovery dialog compared):
- Recovery is reached from the footer FILE menu ("Recovery…"); it has no shortcut. A modal dialog with two tabs, Trash (n) and Checkpoints (n), always opening on Trash with the first row current; with entries the list has the keyboard, otherwise Close does.
- Both lists are newest first (core stores oldest first; the bridge maps display index to stored index). A Trash row shows the title and "Deleted <local time> · N notes"; a checkpoint row its reason ("Before deleting "X"", "Before restoring …") and a local time.
- Enter or Space restores the marked row; a click on RESTORE restores that row; Up/Down are bounded (no wrap); Left/Right switch tabs only on the header; Tab goes header → list → Close; Escape and Close never restore; Delete and typing do not reach the tree behind. Entries vanishing while open never leave a dangling selection.
- Restore = `Document::restore_trash(index)` (checkpoints the current notebook first, original parent and position when possible, else the last root; on failure nothing changes) or `Document::restore_checkpoint(index)` (checkpoints what it replaces, keeps Trash and the other checkpoints, ids never reused). Failures show the core's message in the error dialog. A **checkpoint restore has no confirmation in Qt**, the tree is a model reset and the selection is dropped. After either, Qt warns once about duplicate sibling names the restored data may contain.
- N/Y are mnemonics of the Yes/No dialogs only; Recovery has none. Unsaved edits are not specially handled by the dialog: Qt's autosave and the checkpoint taken by the core cover them.

**egui implementation** (`recovery_ui.rs`, `help.rs`, `keymap.rs`; dialog helpers from E6b made `pub(crate)`):
- Reach Recovery with **Ctrl+Shift+R** or the tree menu's "Recovery…" (Qt: FILE menu only). Opening first commits the pending title (a refused title blocks it with the usual dialog) and flushes the editor buffer, so the lists, the checkpoint a restore takes and the restore itself all start from everything the user has written; typing behind the modal is impossible, so nothing can change between.
- One themed modal (E6b frame, width rule, `Chrome` colours) with a one-stop tab header (Left/Right), a one-stop list (Up/Down/Page/Home/End, Enter/Space restore, per-row **Restore** commands for the mouse) and Close. Rows show the title, "Deleted <UTC time> · N notes", and where the note returns ("Returns to A › B", "Returns to the top level", "… (its old place is gone)", or the core's refusal in the danger colour, computed once with `Document::plan_trash_restore` when the dialog opens and after a restore, never per frame). A one-line scope sentence per tab says what a restore does. Empty states: "Trash is empty", "No checkpoints yet". Default focus: the list, or Close when it is empty. Rows and tooltips cope with long titles and Czech text; at 420 px width everything stays inside the window (test).
- **Trash restore:** `Host::restore_trash` → `Document::restore_trash`; on success the tree is rebuilt (so Search is invalidated), the restored notes' editor state is forgotten (they come back as new notes: no caret, no undo history), the restored root is selected and revealed, the status says "Restored “X” from Trash", the notebook is marked edited (autosave countdown), focus goes to the tree. Nothing else is touched: other notes keep caret and undo. On a refusal the entry stays in Trash and the dialog shows the message inline (and in the status).
- **Checkpoint restore:** always asks first ("Replace the notebook?" with the checkpoint's reason and time, what is lost, and that what is replaced is checkpointed first, unsaved edits included). Buttons: Cancel (focused) and a destructive "Replace notebook"; Enter on the default and Escape both step back to the list. On confirmation `Host::restore_checkpoint` hashes every body before and after the core call and forgets the editor state (caret, selection, undo) of every note that is gone, new, or whose body is not byte-identical afterwards; notes whose text is identical keep theirs (their history only describes their own text), so Ctrl+Z can never bring back text that existed only before the restore. The selection stays when that note still exists (title and text re-read), else the first note; the tree, search results and expansion follow.
- Duplicate sibling names in restored data produce Qt's one-time explanation.
- **F1** opens/closes the reference (ignored behind another question). Sections: Notes, Tree, Editor, Search, Files, General, each with where it applies. Application bindings come from `keymap.rs` (the same constants `handle_shortcuts` consumes) and are formatted by egui, so labels follow the platform; editor bindings are read from the `EditorKeys` the editor is given; the structural keys (tree keys, search keys, slash menu) are listed beside the code that handles them. No Edit/Preview, no Open (not implemented yet), no N/Y.
- **Search additions:** Page Up/Page Down move by a page of results; the query is highlighted (an accent-tinted background, text colour unchanged) in titles and snippets using the same Unicode lower-casing as the core search (`match_ranges`); a result count sits at the right of the field and is exposed as "N results" through the widget info (verified only at the egui API level; a screen reader was not run, remaining for E12). Search refreshes after every restore through the structure invalidation.

**Deliberate differences from Qt:** Ctrl+Shift+R and a tree-menu entry; checkpoint restore asks first; dates in UTC with the zone written (the standard library has no time zones and no time crate was added; Qt shows local time); each Trash row says where it returns; explicit button words rather than N/Y mnemonics (the dialogs use "Cancel" / "Replace notebook", not Yes/No); the selection survives a checkpoint restore when the note still exists (Qt drops it); the restored Trash root is selected (Qt leaves the selection); the "Restore" command is a text command inside the row.

**Attachments (E9/E11 contract):** restored bodies are byte-identical text, `assets/<id>.<ext>` references are never read, rewritten or stripped (test with two image references, through Trash and checkpoint restore, on disk). No attachment counts are shown, nothing is deleted as a side effect.

**Tests:** 29 new in `tests/recovery_tests.rs` (+1 ignored `recovery_timings`) and the E7a fixture reused. They cover: empty lists and focus; ordering, dates, counts and return paths; restoring a leaf and a three-level subtree (bodies, hierarchy, selection, reveal, focus); refusal by a name conflict (entry stays, nothing changes, dialog stays), restoring twice and stale entries; other notes' caret and undo untouched; confirmation, cancel and Escape; checkpoint replacement and the checkpoint it takes (and undoing the restore from Recovery); unsaved edits and the editor buffer checkpointed; an invalid pending title blocking Recovery; undo forgotten for changed notes and kept for unchanged ones; restoring the selected note and a checkpoint lacking the selected note; Search open during a restore; modal keys (Up/Down/Page/Home/End, Left/Right, Tab order, Delete, typing, Ctrl+F, Ctrl+N all inert); a mouse click on Restore; autosave after a restore, save and reopen, schema v3 header, trash and checkpoints persisted; external modification and a failed save after a restore (the restored notes stay, nothing is overwritten); a narrow window with a long Czech title; F1 contents against the key map and the absence of Preview/N-Y; F1 toggling and scrolling; Search paging, highlighting and `match_ranges` Unicode cases; `format_utc`. The E5, E4.1 and E7a tests pass unchanged (`Dialog` gained two variants, `rebuild_tree` is `pub(crate)`). The core refusal `IdCollision` cannot be produced through the UI (ids are never reused); it is covered by the core tests.

**GUI verification** (native Wayland, Hyprland 0.56.2, release build, disposable copies; the real Omarchy theme and notebooks untouched; the Qt build run on the saved result): deleted two subtrees with Delete + the confirmation, opened Recovery with Ctrl+Shift+R, restored a subtree with Down/Enter, created a note, restored a checkpoint via Shift+Tab/Right/Tab/Enter, the confirmation, Escape back and Cancel, the replacement; Ctrl+Z; F1; Ctrl+S, close; the saved notebook opened in Qt (same tree; its Recovery dialog lists the remaining Trash entry and four checkpoints written by egui); dark and light, 1000×700 and 520×480. Not exercised in the GUI: the mouse Restore command (unit-tested with real pointer events), Search during a restore, an invalid rename before opening (unit-tested; the same dialog was seen in E7a), failed-save and conflict flows (unit-tested).

**Performance (release, 6,300 notes, ~2 MB):** opening Recovery with 20 Trash entries and 6 checkpoints 5 ms (rows derived once); idle frame with the dialog open 0.11 ms; restoring a Trash entry ~290 ms and a checkpoint ~530 ms, of which almost all is the core cloning and checkpointing a 6,300-note notebook (deleting a subtree costs the same ~250 ms, already in E5); the host adds the tree rebuild and, for a checkpoint, two passes of hashing the bodies. Nothing is copied or rebuilt per frame.

**Recovery and editor-state invalidation rules:** (1) everything is flushed before the lists exist; (2) the document is only ever changed by `restore_trash` / `restore_checkpoint`; (3) Trash restore: restored notes' state forgotten, nothing else; (4) checkpoint restore: forget every note that is gone, new or textually different, keep byte-identical ones; (5) the selection is repaired in the same call, so no frame can show a note that does not exist; (6) the tree is rebuilt, which invalidates Search; (7) the notebook is marked edited so autosave and close protection treat it like any change.

**Remaining limitations:** UTC times; no permanent deletion of single Trash entries or checkpoints (as Qt); no attachment counts until E9; precision of "unchanged" is byte equality of the body (titles are not part of editor state); the mouse path of Restore and Search-during-restore not seen in a real window; screen-reader exposure of the result count and dialogs unverified (E12); IME, X11, Windows, macOS and high-DPI unexercised; the F1 text is English only.

**E8 readiness:** the file lifecycle items still missing are Open/New (with the unsaved-changes guard), the file dialogs, command-line-less start, close paths other than the viewport close and a reload-from-disk answer to a conflict. Recovery, Search, F1 and every dialog already go through `Host::open_dialog`, `focus_back` and `Document`, and `Host::open` resets Search and Recovery state, so E8 can add Open/New without touching them; it should add Open to the F1 reference (one line in `help.rs`).

### E8a implementation record (New, Open and native file dialogs)

**Qt baseline** (read from `main.qml`, `TreePane.qml`, `Keymap.qml`, `tests/qml/{lifecycle,replacement,paths,autosave,conflict,titleclose}.qml`; the Qt build was run on a notebook saved by egui):
- Launched with no path, Qt shows an empty **Untitled** notebook (`Document::untitled()`: in memory, no file, clean, "Select or create a note"). A bad path argument shows an error dialog and keeps the Untitled notebook; more than one argument is refused.
- Qt has **no shortcut for New Notebook**: it is in the footer FILE menu only (Ctrl+N is "new note"). Open is Ctrl+O, Save Ctrl+S, Save As Ctrl+Shift+S, all disabled while a modal is open.
- Leaving a document (New, Open, system open, close) goes through `whenSafeToLeave`: the pending title is committed first (a refused one stops everything); a file-backed notebook is **saved quietly** and the action goes ahead; only if that fails, or for an untitled notebook, the user is asked Save (labelled "Save As…" when there is no file) / Discard / Cancel. Save carries on only if the save worked (a cancelled Save As drops the action); Discard lets it proceed; Cancel drops it and resumes autosave. Open asks *before* showing the file dialog.
- An untitled notebook is never autosaved ("no file to save to"). Save As appends `.omatree` only when there is no extension, asks before replacing another existing file (the dialog's own overwrite check is disabled), and `Document::save_as` refuses to overwrite a file that is not a notebook. Opening is `Document::open_existing`; a failure shows the message and changes nothing.
- Title: "OmaTree — <name>[ *]". After New/Open the model resets: selection and Search are cleared, the editor is empty; Qt selects nothing after Open (egui selects the first note, as since E4).

**egui implementation** (`files.rs`; `lifecycle.rs`, `lib.rs`, `keymap.rs`, `help.rs`, `tree_ui.rs` changed):
- **Start:** `App::new` always starts with an untitled in-memory notebook (`Host::start_untitled`) and then tries the command-line path; a path that cannot be opened (missing, not a notebook) leaves the untitled notebook with the reason in an error dialog. Nothing is created on disk. The window title reads "Untitled - OmaTree" (then "name.omatree - OmaTree", with " *" when dirty).
- **One guard:** `Host::request_leave(Leave::{Close, New, Open})` replaces the E4.1 close-only path. It commits the title, flushes the editor, and ignores itself while a question or file dialog is open. A clean notebook is left at once (a file-backed one gets the same best-effort view-state save as a clean close); a dirty one asks Save / Discard / Cancel, and Discard needs the established second confirmation. The waiting action is one `Option<Leave>`, taken exactly once by `continue_pending` after a *successful* save or a confirmed discard, and cleared by every cancel. Save on an untitled notebook means Save As. A failed save (including a conflict) keeps the question, the document, the buffer and the pending action. Close, New and Open share all of it; there is no second implementation. (Deviation kept from E4.1: a dirty file-backed notebook always asks; Qt saves it quietly first.)
- **One installer:** opening a path, New and the first start all call `Host::install`: flush the old editor, forget its editor state (caret, selection, undo, images), advance the notebook identity, build the tree from the new `Document` with its stored expansion, select the first note, reset Search, Recovery, drag, menu, autosave and conflict state, give the tree the keyboard. Nothing is written by opening. A failed `Document::open_existing` changes nothing (the error dialog is the only effect) and the file is not created or touched. `open` reads the file once.
- **Shortcuts (`keymap.rs`):** Ctrl+O Open, **Ctrl+Alt+N New notebook** (chosen because Qt has none and Ctrl+N / Ctrl+Shift+N stay note / child; egui never ignores Alt, so it cannot be mistaken for Ctrl+N), plus the existing set. All are inert behind any dialog (the guard checks it) and during a drag; in a text field they act after committing the title, like Ctrl+S. F1 lists both, and the Save line now says an untitled notebook asks where.
- **Menu:** a flat "file" action at the right end of the sidebar's command strip (not a toolbar) opens a menu: New notebook, Open…, Save, Save As…, Recovery…, Keyboard shortcuts, each with its shortcut from the key map. It is one Tab stop, opens with Enter, closes with Escape; items are reachable by Tab.
- **Native dialogs:** `rfd 0.17.2` (`xdg-portal` + `pollster`, no GTK/Qt linkage; MIT, pollster Apache-2.0/MIT; two new crates) through the desktop portal. A dialog runs on its own thread and answers over a channel that `Host::logic` polls once per frame, so the UI never blocks; while it is open a small "Choose a file" modal blocks the keyboard and nothing is touched until it answers. Open filters `.omatree` (and "All files"); Save As offers "<name>.omatree" (for a notebook with a file: "<name> copy.omatree") starting in the notebook's folder. The answer goes through the same `submit_save_target` as the typed dialog (extension added when there is none, a yes before replacing another existing file, `Document::save_as` refusing a non-notebook), a cancel changes nothing, drops any pending action and returns the keyboard. A dialog thread that dies counts as a cancel. **Fallback:** with no picker (tests, or `OMATREE_FILE_DIALOG=typed`) Open and Save As ask for a typed path in a themed dialog.
- **Limitation:** `rfd` reports a failure to reach the portal as a cancel, so on a system without a working file-chooser portal Open/Save As silently do nothing (safe, but quiet): set `OMATREE_FILE_DIALOG=typed` there. The dialogs are not parented to the egui window (no `set_parent`; needs a window handle that is not `Send`). The GTK portal asks its own "replace?" before ours, so replacing an existing file is asked twice; ours stays because it also covers the case where the extension was added.

**Parity differences from Qt:** New Notebook has Ctrl+Alt+N (Qt: menu only); a dirty file-backed notebook asks first (Qt saves quietly); the first note is selected after Open/New-with-notes (Qt: nothing); the file menu is a flat text action (Qt: FILE); UTC/OS dialog differences as before; no system "open with" events (E8b/platform).

**Tests:** 23 new in `tests/files_tests.rs` with a scripted, recording picker: start without a path (no file created, title), with a valid path, with a missing and with a non-notebook path (file untouched); New clean and with each answer (Cancel, Discard + confirmation, Save), New on an untitled notebook through Save As; Save of an untitled notebook (extension added, explicit extension kept, title); no autosave while untitled (no dialog, no failure, nothing on disk); Open clean (dialog start folder, nothing written), Open dirty with Save / Discard / Cancel; a cancelled dialog (and a cancelled Save As during a leave); invalid Open keeping the notebook, caret and buffer (non-notebook and missing, never created); a failing save keeping question and document, then succeeding; an external modification not overwritten; overwrite confirmation, refusal and the non-notebook refusal; Czech/accents/spaces/emoji paths through Save As and Open; schema v3 header and reopen; editor state not crossing notebooks with overlapping NodeIds (undo and caret); Search, expansion, Recovery and autosave/conflict state reset; repeated Ctrl+O/Ctrl+Alt+N and repeated callbacks running once; shortcuts inert behind dialogs and in the title field; the file menu and its keyboard path; the typed fallback; F1 lines. Two existing tests were adjusted (Shift+Tab now meets "file" before "+ child"; the F1 Save label). Qt-compatible round trip: the saved file is user_version 3 and opens in the Qt build.

**GUI verification** (native Wayland, Hyprland 0.56.2, release build, copies in a scratch folder; the user's notebooks listed by the portal's Recent view were never opened, the Omarchy theme untouched): launch without an argument; Ctrl+O opened the real GTK portal dialog (starting in the notebook's folder), Escape cancelled it with nothing changed; opening a Czech-named notebook with spaces ("Poznámky č. 1.omatree", via Tab completion because `ydotool type` cannot type accents) updated the window title; opening a non-notebook file showed the error dialog and left the notebook as it was (seen twice); an edit followed at once by Ctrl+O showed Save / Discard / Cancel, Cancel kept it, Discard asked "Discard changes?" and then opened the other notebook with the discarded text absent from the first file; Ctrl+Alt+N → new note → Ctrl+S opened the portal's Save dialog offering "Untitled.omatree" and saved as "Novy sesit.omatree"; Save As onto an existing notebook raised the portal's own replace question and then ours, whose Cancel left the target untouched; closing the window with unsaved changes asked; the file menu in the light theme at 520×480; the saved notebook opens in the Qt build (title "OmaTree — …", user_version 3). **Automated only:** Save failure and conflict during replacement, the Save answer of the question in a real window, overwriting an existing file with Yes, overlapping NodeIds and undo after switching, Search/Recovery reset, repeated commands, typed fallback. Not exercised: X11, other portals (KDE/Hyprland-only chooser), Windows, macOS, high-DPI, "open with" from the file manager.

**Performance:** nothing on the per-frame path changed (the picker poll is one `try_recv` while a dialog is open and nothing otherwise); Open reads the file once (`Document::open_existing`); the E3.1 editor and E5 tree virtualisation are untouched (the whole existing suite passes).

**Remaining for E8b:** the platform "open this file" events (command-line arguments beyond one path, macOS Finder, Windows association), a "reload from disk" answer to an external-modification conflict (today: Save As elsewhere or reopen), recent files, parenting the dialog to the window, a visible hint when the portal is missing, and the cut-over items outside the file lifecycle.

### Recommendation: first ticket

**Start with E1 (extract `omatree-core`)**, run in parallel with E2 in the spike repository if there are two people or sessions.

Why E1: it is mechanical, has an objective pass condition (the existing ~250 pure tests build and pass without Qt; the other 43 Rust tests and 24 QML scenarios pass unchanged), changes no behaviour, and everything that follows depends on the egui app being able to link the domain code without Qt. If the visibility surface turns out larger than the ~39 `pub(crate)` items counted here, we learn that before any UI work. E2 is the risk-reduction ticket for the editor side and does not touch OmaTree at all, so it can start at the same time.

---

## 7. Risk register

| # | Risk | Likelihood | Impact | Mitigation / ticket |
|---|---|---|---|---|
| R1 | A second SQLite connection for attachments breaks saves (`data_version` guard) | certain if designed that way | High | One connection; store is a view on `Document` (§3.6, E9) |
| R2 | Attachment table with a cascading FK is wiped by `DELETE FROM nodes` on every save | certain if designed that way | High | No FK to nodes (§5.3) |
| R3 | Bumping `SCHEMA_VERSION` re-runs `CREATE TABLE expanded_nodes` on v3 files | certain without restructuring | High | Per-version migration steps (§5.3, E9) |
| R4 | Pruning deletes an attachment still referenced from Trash, a checkpoint, or pending state, or one decided from visible notes only | medium | High (data loss) | Referenced set is computed from the whole stored state; only Compact Notebook prunes; dry run, confirmation, checked-write guard, rollback tests (E9, E11) |
| R20 | Orphaned attachments accumulate because nothing prunes automatically (D9) | high | Low–Medium | Compact Notebook reports them; the app can show counts; never auto-delete |
| R21 | `VACUUM` fails or damages the user's file (low disk, open connection, interruption) | low–medium | High | Separate, confirmed step; original file intact on failure; interrupted-operation tests (E9) |
| R5 | Undo/caret state applied to the wrong note's text | medium | High | Per-note ids; `forget_note`; invariant test (E3) |
| R6 | Text/selection/undo semantic differences vs Qt `TextArea` (word/line movement, IME, dead keys, Czech compose, platform shortcuts) | medium | High | egui handles it in `TextEdit`; the spike did **not** test IME or accessibility; dedicated checklist rows (E12) |
| R7 | Shortcut conflicts: Ctrl+E, Ctrl+Shift+S | resolved by D5, D6 | Low | `EditorKeys` supplied by host; strikethrough binding chosen in E2; F1 reference lists the real keys |
| R8 | Hand-written tree view (drag-and-drop, keyboard, scrolling) is large and fragile | high | Medium | E5 scoped separately; port scenario tests; performance test with a large notebook |
| R9 | The concealment technique (0.1 pt glyphs, one-frame selection lag) behaves differently on other platforms/GPUs | medium | Medium | spike validated only on Wayland (X11 inconclusive, macOS/Windows not tested); test matrix in E12 |
| R10 | Large notes: per-frame re-layout, whole-text copy per changed frame | unknown | Medium | measure in E3; the spike did not measure large documents |
| R11 | Accessibility: Qt exposes accessibility; egui has AccessKit but hidden-text editing is not validated | medium | Medium (product decision) | state a requirement; verify with a screen reader; or accept a deviation |
| R12 | Packaging rewrite on four platforms; macOS Finder open-file events and Windows AppUserModelID have no egui equivalent verified | high | Medium | E12; keep Qt packaging until then; macOS/Windows hardware needed |
| R13 | Native file dialogs and theme-following (`rfd`, system theme) behave differently under Wayland portals | medium | Medium | E8/E6, platform checks |
| R14 | Binary size and dependencies: spike adds ~19 MB → 26 MB release; PDF adds ~7 MB; eframe pulls winit/glow/accesskit | certain | Low–Medium | Cargo feature for PDF; Qt's own runtime is not shipped so net package size may fall (unmeasured) |
| R15 | Two repositories drift while the integration is in progress | high | Medium | Library crate with path dependency (E2/E4); freeze spike features during the migration |
| R16 | Principle conflict: "never loads resources", "plain-text editor", "no networking" vs images and PDF | medium | Medium | D8, §5.6; `FsStore` not linked; no network code; attachment-loading tests (E10) |
| R17 | Sharing the same `.omatree` between Qt and egui apps during the parallel period | medium | High | Lazy v4 migration (only when images are used); never test on the user's real notebook; copy fixtures |
| R18 | egui API churn (the spike uses egui 0.37 internals such as `Undoer`, `TextEditState`) | medium | Medium | Pin versions; tests around undo boundaries stay |
| R19 | Test debt: ~43 Rust tests and 24 QML scenarios have no egui equivalent yet | certain | Medium | each ticket ports named scenarios; §8 tracks it |

---

## 8. Feature-parity checklist

Each row is "Qt feature → the existing Qt test that defines it → the egui ticket". A row is **PASS** only after it has run in the egui app.

| Feature | Defined by | Ticket | Status |
|---|---|---|---|
| Untitled notebook; create, save, reopen | `lifecycle.qml` | E4/E8 | E8a implemented |
| Create root/child, default titles ("New note 2") | `newnote.qml`, `notebook.rs` | E5 | not started |
| Rename with empty/duplicate refusal; pending title committed before leaving or closing | `titles.qml`, `titleclose.qml` | E5/E8 | not started |
| Edit body; every real change dirties; unchanged text never does | `EditorPane.qml`, `document.rs` | E3/E4 | not started |
| Autosave after edits; failure shown once; retry | `autosave.qml`, `main.qml` | E8 | not started |
| Save, Save As, overwrite confirmation, non-notebook refusal | `lifecycle.qml`, `document.rs` | E8 | E8a implemented (native dialogs) |
| Open/New replace the whole notebook safely (asks first) | `replacement.qml` | E8 | E8a implemented |
| External modification conflict message; nothing written | `conflict.qml` | E8 | not started |
| Paths with spaces/accents/Czech; `.omatree` appended | `paths.qml` | E8 | E8a implemented |
| Tree: expand/collapse, subtree toggle, persisted expansion | `expansion.qml`, `moveexpanded.qml` | E5 | not started |
| Tree keyboard (arrows, Enter, F2, Delete, Alt+Up/Down, context menu key) | `keyboard.qml`, `keymap.qml` | E5 | not started |
| Drag and drop move, cycle prevention, drop indicators | `TreePane.qml`, `notebook.rs` | E5 | not started |
| Delete → Trash with confirmation; Trash and checkpoint restore | `recovery.qml`, `recoverykeys.qml` | E7 | E7b implemented (see record) |
| Search: title first, body snippets, reveal the result | `search.qml`, `searchheader.qml` | E7 | E7a implemented (see record); E7b: result paging, highlight, count |
| Escape returns to the tree from title/body; transient surfaces keep their own (the preview case is retired with Preview) | `escape.qml` | E5/E7 | not started |
| Dialog keyboard (one focus, Enter presses it, N/Y/Esc) | `dialogs.qml` | E7 | E6b/E7b: one focus, Enter, Escape, Tab; explicit words instead of N/Y |
| F1 shortcut reference built from the real key map | `keymap.qml` | E7 | E7b implemented (`keymap.rs`, `help.rs`) |
| Omarchy palette live; system light/dark fallback; non-Linux has no Omarchy | `theme.qml`, `systemtheme.qml` | E6 | E6a implemented (pipeline); E6b implemented (visual polish) |
| ~~Edit/Preview toggle, preview spacing~~ | `preview.qml`, `spacing.qml` | **not carried over (D4)**: live rendering replaces it; raw Markdown stays reachable while editing | retired |
| No remote or arbitrary-file image loading (was `resources.qml` for Preview) | `resources.qml` | E10 (new test) | not started |
| **New:** attachment pruning never removes recoverable images; Compact Notebook dry run, confirmation, result report; separate space reclamation | §5.4, D9 | E9/E11 | not started |
| macOS Finder "open with" goes through the unsaved-changes flow | `systemopen.qml` | E12 | not started |
| Window icon, Wayland app id, Windows taskbar id | `cpp/app_identity.cpp` | E12 | not started |
| Cross-platform keyboard: ⌘ on macOS, Tab reaches controls | `keyboard_policy.cpp`, `keymap.qml` | E12 | not started |
| **New in egui:** live-styled Markdown editing, tasks, tables, links, emoji, slash menu | spike `FINDINGS.md` | E2/E3 | spike only |
| **New in egui:** images stored in the notebook, resizing | §5 | E9/E10 | not started |
| **New in egui:** Markdown/folder/ZIP/PDF export | spike | E11 | spike only |
| Accessibility, IME | none today in tests | E12 | NOT TESTED in spike |
| Linux Wayland / X11 / Windows / macOS | `packaging/*` | E12 | spike: Wayland only |

---

## 9. Blockers and architectural uncertainties

No hard blocker found. The core is cleanly separated from Qt, and the editor's rendering, editing, attachment and export modules do not depend on `EditorApp`. These items need an answer or a spike:

1. **Strikethrough binding** (shortcut vs slash command only) after D6. Shortcut ownership and Preview are otherwise decided (D4–D6).
2. **Undo across note switches and external body replacement:** the design relies on egui's per-`Id` `TextEditState` and `Undoer` public API. It should work, but only a test in E3 proves it.
3. **IME, accessibility, X11, Windows, macOS:** the spike has none of these verified (its own `FINDINGS.md`). They bound the final verdict and must not be assumed.
4. **Tree view in egui:** drag-and-drop, keyboard, and scale are the biggest unbuilt piece of UI; third-party tree crates were not evaluated.
5. **Window lifecycle** (close interception, macOS file-open events, native dialogs) is unverified in egui/eframe 0.37.
6. **Large-note performance** was never measured in the spike.
7. **Code-sharing between the two repositories** (path dependency vs git dependency vs moving the spike into the OmaTree workspace) is a maintainer decision with release implications.
8. **Attachment id scheme** (random vs content hash) and **image formats** (PNG-only display versus JPEG accepted by PDF).
9. I could not run OmaTree's tests or any egui GUI during this assessment; every status above is from reading code, not from execution.
