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
- Objective: new workspace crate with an `eframe` window, opens a notebook path via `Document`, shows a **flat read-only tree** and a single `EditorView` for the selected note; `set_body` write-through; Ctrl+S.
- Affected: `crates/egui-app`; depends on `core` and `editor` (path dependency; this is where the code-sharing decision from §0 is exercised).
- Behaviour: open an existing `.omatree`, select a node, edit its body, save, reopen: content and tree unchanged; the Qt app opens the same file.
- Tests: headless tests with `egui_kittest`-style or the spike's frame harness [Unverified which; the spike's `AppHarness` is custom]: edit → `Document` body equals buffer → `save` → reopen → equal. A schema v3 notebook written by the Qt app round-trips byte-for-byte in `nodes`.
- GUI: open a copy of a real notebook (not the original); compare tree and bodies with the Qt app; confirm `cmp` on untouched notes.
- Acceptance: no data loss on open/edit/save/reopen with fixture notebooks produced by the Qt app; the file remains openable by Qt.
- Risks: first time the shared `Document` is driven from egui; copy fixture, never the user's real notebook.

**E5: Tree pane parity**
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
| Untitled notebook; create, save, reopen | `lifecycle.qml` | E4/E8 | not started |
| Create root/child, default titles ("New note 2") | `newnote.qml`, `notebook.rs` | E5 | not started |
| Rename with empty/duplicate refusal; pending title committed before leaving or closing | `titles.qml`, `titleclose.qml` | E5/E8 | not started |
| Edit body; every real change dirties; unchanged text never does | `EditorPane.qml`, `document.rs` | E3/E4 | not started |
| Autosave after edits; failure shown once; retry | `autosave.qml`, `main.qml` | E8 | not started |
| Save, Save As, overwrite confirmation, non-notebook refusal | `lifecycle.qml`, `document.rs` | E8 | not started |
| Open/New replace the whole notebook safely (asks first) | `replacement.qml` | E8 | not started |
| External modification conflict message; nothing written | `conflict.qml` | E8 | not started |
| Paths with spaces/accents/Czech; `.omatree` appended | `paths.qml` | E8 | not started |
| Tree: expand/collapse, subtree toggle, persisted expansion | `expansion.qml`, `moveexpanded.qml` | E5 | not started |
| Tree keyboard (arrows, Enter, F2, Delete, Alt+Up/Down, context menu key) | `keyboard.qml`, `keymap.qml` | E5 | not started |
| Drag and drop move, cycle prevention, drop indicators | `TreePane.qml`, `notebook.rs` | E5 | not started |
| Delete → Trash with confirmation; Trash and checkpoint restore | `recovery.qml`, `recoverykeys.qml` | E7 | not started |
| Search: title first, body snippets, reveal the result | `search.qml`, `searchheader.qml` | E7 | not started |
| Escape returns to the tree from title/body; transient surfaces keep their own (the preview case is retired with Preview) | `escape.qml` | E5/E7 | not started |
| Dialog keyboard (one focus, Enter presses it, N/Y/Esc) | `dialogs.qml` | E7 | not started |
| F1 shortcut reference built from the real key map | `keymap.qml` | E7 | not started |
| Omarchy palette live; system light/dark fallback; non-Linux has no Omarchy | `theme.qml`, `systemtheme.qml` | E6 | not started |
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
