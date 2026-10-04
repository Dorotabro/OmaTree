# Ticket 027 — Pre-release code audit

## Goal

Perform a complete pre-release review of the OmaTree codebase.

This ticket is REVIEW ONLY.

Do not modify production code.

Do not refactor.

Do not reformat files.

Do not update dependencies.

Do not fix findings during the audit.

The purpose is to identify real risks before packaging and the first public
release, while the current working application and test suite provide a stable
baseline.

OmaTree is now feature-frozen for v0.1 except for fixes accepted from this
audit.

---

# Primary review question

Could the current code safely become OmaTree v0.1?

Focus especially on:

- data loss
- corrupted notebook handling
- unsafe filesystem behaviour
- invalid model/index state
- persistence bugs
- Qt/QML lifetime or notification errors
- security/privacy issues
- release-build surprises
- cross-platform assumptions
- important missing tests

Do not optimise for style purity.

Working, understandable code is preferable to theoretically elegant rewrites.

---

# Severity model

Classify each finding as:

## Critical

Could reasonably cause:

- data loss
- corruption of an existing notebook
- arbitrary command/code execution
- unintended network/local-file access
- serious security/privacy failure
- unrecoverable application state

Critical findings block packaging.

## High

Could cause:

- common crashes
- incorrect saves
- broken recovery
- invalid model state
- destructive behaviour under realistic user actions
- significant release/platform failure

High findings normally block v0.1.

## Medium

Real correctness, maintainability, UX-state or platform risk worth addressing
before release if reasonably small.

Medium findings should include a recommendation but do not automatically block
release.

## Low

Minor robustness issues, cleanup, small inefficiencies or technical debt.

Do not inflate Low findings merely to produce a longer report.

## Observation

Not a bug.

Examples:

- unusual but intentional architecture
- acceptable performance trade-off
- future scaling concern
- code that deserves documentation but not modification

---

# Evidence standard

Every finding must include:

- severity
- short title
- affected file(s)
- affected function/component where possible
- exact behaviour/risk
- why it matters
- concrete reproduction or reasoning
- recommended minimal fix
- whether an existing test covers it
- suggested regression test if one is missing
- confidence: high / medium / low

Do not report vague statements such as:

    "error handling could be improved"

without showing where and why.

Do not report purely subjective style preferences as defects.

---

# Part 1 — Repository and architecture survey

Read at minimum:

- README.md
- VISION.md
- ARCHITECTURE.md
- AGENTS.md
- all tickets that materially describe current behaviour
- Cargo.toml
- build.rs
- src/
- cpp/
- qml/
- tests/
- docs/
- THIRD_PARTY.md
- license files

Build a concise architecture map before judging individual components.

Understand the intended boundaries:

- Notebook
- Document
- Storage
- Recovery
- Search
- Theme
- Markdown renderer
- CXX-Qt bridge/models
- QML presentation
- integration-test runner

Do not criticise deliberate architectural choices without considering the
project's stated simplicity goals.

---

# Part 2 — Rust correctness review

Inspect all production Rust.

Look for:

- unwrap()/expect() reachable from user input or normal runtime
- panic paths
- unchecked assumptions
- integer conversion hazards
- stale NodeId handling
- missing validation
- accidental state divergence
- ownership/lifetime mistakes
- unsafe blocks
- interior mutability misuse
- hidden global state
- errors converted to misleading success
- ignored Result values
- unexpectedly lossy conversions
- Unicode/string slicing mistakes
- quadratic or pathological algorithms in frequent paths

If an unwrap is provably safe because of a local invariant, record it only as
an Observation if useful.

Do not demand abstraction merely to remove a harmless unwrap in test code.

---

# Part 3 — Notebook model invariants

Review the core tree model carefully.

Verify reasoning around:

- NodeId allocation
- root/child creation
- sibling positions
- rename uniqueness
- empty-title rules
- moves
- same-parent reorder
- cross-parent reparent
- cycle prevention
- delete/subtree handling
- legacy duplicate compatibility
- breadcrumb generation
- tree-order traversal
- search traversal
- corrupted internal structures

Look for any sequence of valid API calls that can produce:

- duplicate sibling positions
- missing parents
- cycles
- unreachable nodes
- reused live NodeIds
- inconsistent ordering

Do not duplicate existing tests unless a genuine gap exists.

---

# Part 4 — Persistence and SQLite

Treat this as one of the highest-value sections.

Inspect:

- schema v1
- schema v2
- schema v3
- migration logic
- opening legacy files
- save_active
- full document save
- Save As
- transactional behaviour
- rollback behaviour
- next NodeId persistence
- Trash persistence
- checkpoint persistence
- expanded_nodes persistence

Verify:

- opening never mutates the file unexpectedly
- migrations occur only during successful writes
- failed migration/save cannot partially corrupt the file
- Save As cannot partially overwrite an existing unrelated file
- invalid future schema versions are rejected safely
- malformed/corrupted database contents fail clearly
- foreign-key assumptions are actually enabled where needed
- old files remain recoverable
- expansion metadata cannot break document loading

Look especially for ordering problems where in-memory state is mutated before a
transaction is known to have succeeded.

---

# Part 5 — Autosave and dirty-state review

Review all dirty-state categories:

- active/content dirty
- recovery dirty
- view/expansion dirty
- Untitled state
- file-backed state

Verify interactions among:

- ordinary edits
- rename
- creation
- move
- delete
- Trash restore
- checkpoint restore
- expansion changes
- theme changes
- Preview/Edit changes
- Search
- New Notebook
- Open
- Save
- Save As
- Close

Look for:

- changes that should save but can be lost
- changes that unexpectedly cause save
- false dirty stars
- autosave occurring when it should not
- safe-to-leave paths that can discard data
- timers surviving document replacement
- failed autosave suppression behaving incorrectly

---

# Part 6 — Recovery and Trash

Review destructive-operation safety carefully.

Verify:

- checkpoint creation occurs before destructive mutations where intended
- failed destructive operations do not create misleading checkpoints
- Trash restore preserves expected IDs/subtrees
- destination-title conflicts remain safe
- legacy duplicate subtrees remain recoverable
- checkpoint restore itself remains recoverable
- checkpoint limits are enforced correctly
- stale expansion metadata cannot corrupt Recovery
- restore failure leaves source recovery data intact

Look for scenarios where:

    user thinks recovery succeeded
    but data was silently dropped

These are high priority.

---

# Part 7 — Qt model correctness

Audit the CXX-Qt / QAbstractItemModel boundary.

Review:

- QModelIndex creation
- internalId usage
- index resolution
- parent()
- rowCount()
- data()
- beginInsertRows/endInsertRows
- beginRemoveRows/endRemoveRows
- beginMoveRows/endMoveRows
- beginResetModel/endResetModel
- dataChanged
- structureRevision
- search result indexes
- replacement of Documents

Look for:

- stale QModelIndex use
- row/index mismatch after move
- Qt begin/end calls with incorrect source/destination rows
- model mutation outside required begin/end notifications
- indexes surviving reset incorrectly
- parent indexes resolving to the wrong current row
- internalId assumptions that fail after deletion/reuse
- callbacks into QML during inconsistent model state

This section should be evidence-driven.

Qt model mistakes are subtle and deserve careful attention.

---

# Part 8 — QML state and signal review

Review all production QML.

Look for:

- binding loops
- state duplicated between Rust and QML
- stale cached QModelIndex properties
- timers surviving wrong state transitions
- signal-handler ordering assumptions
- focus traps
- unintended shortcut activation
- invisible controls retaining focus
- popup/dialog state leaking into main UI
- Search query/result desynchronisation
- Preview source desynchronisation
- expansion syncing loops
- drag/drop state remaining stale
- transient UI state surviving New/Open when it should reset

Distinguish real correctness issues from harmless presentation state.

---

# Part 9 — File handling and filesystem safety

Review all paths controlled by the user.

Inspect:

- CLI notebook path
- Open
- Save
- Save As
- extension handling
- overwrite confirmation
- temporary/replacement writes if used
- symlinks
- relative paths
- Unicode paths
- spaces
- nonexistent parent directories
- permission failures
- read-only files
- directories passed as files

Look for:

- unintended overwrite
- truncation before validation
- TOCTOU-style mistakes that matter in a desktop context
- file-path assumptions that break cross-platform
- Linux-only path handling outside cfg-gated code

Do not overstate theoretical local-attacker models that are irrelevant to a
single-user notes application.

---

# Part 10 — Markdown security and correctness

Review the complete Markdown pipeline.

Pay special attention to:

- MarkdownNoHTML handling
- detached QTextDocument
- image/resource conversion to alt text
- remote resource blocking
- local file blocking
- serialized rich-text output
- link activation
- allowed URL schemes
- javascript:
- data:
- file:
- malformed URLs
- mailto:
- Unicode/escaped URL handling
- current double-parse hard-break detection
- pathological Markdown input

Verify there is no route from note contents to:

- automatic network requests
- arbitrary local-file reads
- script execution
- command execution

Assess whether the hard-break analysis can:

- panic
- become pathologically expensive
- misalign blocks and alter content
- break on unusual Unicode

Do not propose WebEngine or another Markdown library merely for aesthetic
reasons.

---

# Part 11 — Theme / Omarchy integration

Review:

- XDG_STATE_HOME handling
- HOME fallback
- Linux cfg gates
- colors.toml parser
- required/optional keys
- malformed colour handling
- polling
- two-poll stability logic
- theme disappearance/replacement
- system light/dark fallback
- semantic role mapping

Look for:

- user-controlled theme data causing panic
- excessive logging
- unbounded polling work
- cross-platform compile hazards
- accidental document mutation
- stale theme state after provider disappearance

The 500 ms file poll is an accepted design choice unless a concrete defect
exists.

Do not report "polling is inelegant" as a bug.

---

# Part 12 — Search review

Inspect:

- case folding
- Unicode handling
- snippet generation
- breadcrumbs
- tree order
- debounce
- stale-result activation
- Search query lifecycle
- Search result IDs
- persistent Search header

Look for:

- invalid UTF-8 slicing
- stale NodeId resolving to another note
- pathological query behaviour
- accidental quadratic work at ordinary sizes
- Search changing document state

An O(n) notebook scan is intentionally accepted for v0.1.

---

# Part 13 — Expansion/view-state review

Review:

- expanded NodeId ownership
- view_dirty
- save behaviour
- legacy migration
- Save As
- moves
- delete/restore
- Search reveal
- recursive expansion
- model reset restoration
- stale NodeId reconciliation

Check the recent move-into-collapsed-parent fix specifically.

Look for any mismatch where Document and TreeView can permanently disagree.

Expansion state is UI metadata and should not become content dirty.

---

# Part 14 — Keyboard, focus and shortcut review

Review the accumulated keyboard system.

Check:

- Ctrl+N
- Ctrl+Shift+N
- Ctrl+F
- Ctrl+E
- Ctrl+S
- Ctrl+Shift+S
- Ctrl+O
- F1
- F2
- Delete
- Alt+Up / Alt+Down
- Ctrl+Left / Ctrl+Right
- Shift+F10
- Menu key
- Escape behaviour

Look for:

- conflicting Shortcut scopes
- editor shortcuts being intercepted
- hidden controls owning shortcuts/focus
- platform-reserved conflicts
- actions firing twice
- new-note Edit reset regression
- Escape focus-transfer regression

Do not redesign shortcuts unless there is an actual correctness issue.

---

# Part 15 — Cross-platform readiness

OmaTree is developed on Omarchy/Linux but intends to support:

1. Omarchy
2. other Linux
3. macOS
4. Windows

Audit compile/runtime assumptions for:

- filesystem paths
- separators
- HOME/XDG assumptions
- environment variables
- native dialogs
- fonts
- Qt Basic style
- path URLs
- executable naming
- UTF-8 filenames
- cfg(target_os)
- platform-specific APIs
- C++ compiler assumptions
- build.rs behaviour

Separate findings into:

- likely compile blocker
- likely runtime bug
- untested but reasonable assumption

Do not demand that Windows/macOS be fully supported before Linux v0.1.

---

# Part 16 — Build and release-mode audit

Inspect:

- debug-only hooks
- OMATREE_TEST_QML
- cfg(test)
- release binary contents where practical
- build.rs
- generated C++ integration
- QML resource embedding
- Cargo features
- bundled SQLite configuration
- environment-dependent build behaviour

Verify test-only mechanisms cannot be activated in release where they are
supposed to be disabled.

Look for development paths or absolute machine-specific paths accidentally
compiled into release artefacts.

Do not package anything.

---

# Part 17 — Dependencies

Review the current dependency set.

Identify:

- dependency no longer used
- unexpectedly large/default feature set
- duplicate functionality
- obsolete feature flags
- dependency with relevant known architectural risk

Do NOT update dependency versions in this ticket.

Do NOT perform a vulnerability scan requiring network access unless explicitly
available and already configured.

Licensing was already audited in Ticket 022.

Do not repeat the entire licensing audit.

---

# Part 18 — Performance sanity review

Do not optimise prematurely.

Look only for obvious problems.

Consider:

- large tree traversal
- sibling-title scan
- Search
- breadcrumbs
- tree-guide metadata
- full SQLite rewrite
- recovery snapshots
- Markdown rendering
- theme polling
- QML delegate work

Classify findings by realistic notebook size.

For example:

    O(n²), but only during explicit validation on a few hundred nodes

may be an Observation rather than a release blocker.

If a concern only matters at tens of thousands of notes, say so.

---

# Part 19 — Tests and blind spots

Review all 265+ existing tests and permanent integration scenarios at a high
level.

Do not enumerate them all.

Identify important behaviours that appear:

- untested
- falsely assumed tested
- only covered by brittle implementation-level tests
- only manually tested but high-risk

Pay special attention to:

- persistence failure paths
- corrupted databases
- native file dialog boundaries
- drag/drop
- Recovery
- Markdown resource safety
- stale QModelIndex safety
- release/debug differences

Recommend new tests only where they materially reduce release risk.

---

# Part 20 — Public-repository sanity

Do a local repository-content review only.

Look for:

- accidentally committed secrets
- tokens
- API keys
- passwords
- private paths
- usernames/home-directory paths
- screenshots containing private data
- temporary files
- generated build outputs
- personal notebook files
- test database files
- editor swap files

Inspect .gitignore for obvious gaps.

Do not rewrite Git history.

Do not modify files.

If something sensitive appears in Git history rather than the current tree,
report it clearly as requiring history cleanup before publication.

---

# Commands

Run the existing verification suite before reviewing so the baseline is known:

    cargo fmt --check
    cargo clippy --all-targets
    cargo test
    cargo test integration_tests
    cargo build
    cargo build --release

You may run additional read-only/local diagnostic commands.

Do not:

- modify source
- run dependency updates
- install packages
- publish anything
- package OmaTree
- alter user files
- alter Git history

---

# Final report structure

Produce the report in this order:

## Executive summary

State:

- whether you believe OmaTree is suitable to proceed toward v0.1 packaging
- number of findings by severity
- top three risks, if any

## Blocking findings

Critical and High findings only.

For each, provide the full evidence format.

## Medium findings

Full evidence format.

## Low findings

Keep concise.

Group related cleanup items where sensible.

## Observations / accepted trade-offs

Record unusual but reasonable design decisions so they are not repeatedly
rediscovered as "problems".

## Test gaps

Only meaningful gaps.

## Cross-platform notes

Separate:

- Linux/Omarchy
- generic Linux
- macOS
- Windows

## Release recommendation

Choose one:

    GO — no blocking findings
    GO AFTER FIXES — specific Critical/High fixes required
    HOLD — architecture/data-integrity problem requires deeper work

If recommending fixes, identify the smallest safe remediation set.

---

# Important constraints

DO NOT MODIFY ANY FILE.

DO NOT CREATE A FIX COMMIT.

DO NOT REFACTOR.

DO NOT UPDATE DEPENDENCIES.

DO NOT START PACKAGING.

DO NOT START ANOTHER TICKET.

This ticket ends with the audit report only.