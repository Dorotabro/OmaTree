# Qt → egui migration: decision record

Ticket E0. Documentation only. Branch: `egui-integration` (worktree `OmaTree-egui`).

Status labels used in this document and in the instructions it updates:

- **Current** is what exists in the repository today (the Qt/QML application).
- **Target** is the approved egui/eframe direction. Most of it is **not implemented yet**; `docs/EGUI_MIGRATION_PLAN.md` sequences the work.

## Approved decisions

| # | Decision | Consequence |
|---|---|---|
| 1 | Qt/QML will be replaced with egui/eframe. The Qt application stays buildable and functional throughout the migration. | Qt is removed only by a separate, explicit cut-over decision (ticket E13), after the parity checklist passes. Until then no change may break the Qt build, its tests or its packaging. |
| 2 | The Qt-free Rust notebook model, SQLite persistence, recovery (Trash and checkpoints), search and lifecycle code are preserved. | They are extracted into a shared crate (E1) and are not rewritten. Behaviour is pinned by their existing tests. |
| 3 | The standalone egui Markdown editor stays operational. Its reusable functionality is exposed through a library crate. | The editor repository keeps a runnable demo binary; OmaTree depends on the library (E2, E4). **Since P2 the library is the workspace crate `crates/editor`** (a snapshot of the prototype's revision `30a95dd`); the demo binary stays only in the private prototype repository. |
| 4 | There is no separate Edit/Preview toggle in the egui application. Live Markdown rendering replaces it. Raw Markdown stays accessible while editing. | The editor already reveals the source of the span under the caret or selection. The Qt Preview and its tests (`preview`, `spacing`) are legacy-only and are not ported. `resources.qml`'s rule survives as decision 8. |
| 5 | **Ctrl+E** is reserved for inline code in the egui editor. | Qt's Ctrl+E (Edit/Preview toggle) has no egui counterpart. |
| 6 | **Ctrl+Shift+S** is reserved for Save As in OmaTree. Strikethrough gets an alternative shortcut or a slash command. The standalone editor's shortcuts may stay configurable and different. | The editor library takes a key map from the host (`EditorKeys`). The alternative strikethrough binding is chosen in E2; the shortcut reference (F1) must list the final bindings. |
| 7 | Attachment identifiers are random 128-bit ids, rendered as portable relative paths such as `assets/<32-hex-id>.png`. | Content hashing and deduplication are not part of the first version. Exported Markdown keeps working in other tools. |
| 8 | The integrated app loads images only from notebook-owned attachments. It never fetches remote resources and never loads arbitrary filesystem images. | The editor's filesystem store (`FsStore`) is not linked into OmaTree. Remote and unresolved references show as text or a placeholder. The existing "no networking" rule stands. |
| 9 | Attachment cleanup and database compaction are required product functionality. | Specified below; ticket E9 includes pruning and Compact Notebook, and E10 and later tickets must not close without them. |

## Attachment cleanup and compaction requirements

These are requirements, not an implementation. The transaction strategy is decided when SQLite attachments are implemented (E9), but each point below needs an automated test then.

1. **Reference-aware pruning.** An attachment may be removed only if no body anywhere references it.
2. **What counts as "referenced".** Bodies of active notes, of every Trash entry and of every retained recovery checkpoint, plus attachments not yet written to the file (pending). Pruning must never be decided from the notes currently visible or selected.
3. **No automatic deletion of recoverable attachments.** Nothing in autosave, ordinary save, switching notes or closing deletes an attachment. An attachment that Trash or a checkpoint can still bring back is never removed; it becomes removable only after that Trash entry or checkpoint is itself gone.
4. **Compact Notebook** is a deliberate user operation (never automatic) that:
   - first reports what it expects to do: the number and total size of unreferenced attachments, the number and size of attachments kept because Trash or checkpoints still reference them, and the expected file-size change;
   - requires confirmation;
   - afterwards reports the actual result: attachments removed, attachments kept, bytes freed, and file size before and after.
5. **Pruning and space reclamation are separate steps.** Deleting rows frees pages inside the SQLite file but does not shrink it. Shrinking (for example `VACUUM`) is its own step, reported separately, and only where appropriate (for example not while another connection is open, and with enough free disk for the copy it makes).
6. **User-facing reporting** states attachment counts and file sizes in plain words, before and after.
7. **Safety tests required** (E9): rollback when pruning or compaction fails part way; an interrupted operation leaves a valid notebook; the external-modification conflict guard (`ExternallyModified`) applies to pruning and compaction and nothing is deleted when it fires; images reachable from Trash or a checkpoint survive pruning and are shown again after restore; a notebook without attachments is untouched.

## Documents updated by this ticket

| File | Change |
|---|---|
| `AGENTS.md` | States the migration, separates Current (Qt) from Target (egui) rules, and keeps the Qt rules for the legacy application. |
| `ARCHITECTURE.md` | Existing Qt architecture kept as the *Current* section; the proposed egui architecture added as *Target*. |
| `VISION.md` | Qt-specific wording made technology-neutral where it described the product, not the toolkit. |
| `docs/EGUI_MIGRATION_PLAN.md` | Records the decisions, revises the affected tickets (E2, E7, E9, E10, E11, E12), the risks and the parity checklist. |

## Remaining open questions

- **Alternative strikethrough shortcut** (decision 6): a binding or only a slash command. Chosen in E2.
- **How the two repositories share code**: path dependency, git dependency or moving the editor into this workspace. Decided in E2/E4 (git dependency pinned to a commit) and **superseded in P2: the editor is now a workspace crate** so that the public repository builds without the private prototype.
- **Image formats** the editor can display (the standalone editor builds `image` with PNG only; PDF export accepts JPEG).
- **Compact Notebook entry point** (menu, command or dialog) and its exact wording: decided in E9/E11 within OmaTree's "no permanent toolbar" rules.
- Platform, input-method and accessibility gaps listed in §9 of the migration plan.
