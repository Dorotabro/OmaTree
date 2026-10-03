# Ticket 022 — Release prerequisites

## Goal

Prepare OmaTree for packaging without actually packaging or releasing it yet.

This ticket has three narrowly defined jobs:

1. fix the live expansion-state inconsistency discovered by Ticket 021
2. establish OmaTree's source-code license
3. audit and document third-party/runtime licensing needed for later packaging

Do not add product features.

Do not create installers or distro packages.

Do not invent an application icon. The icon will be supplied separately before
the packaging ticket.

---

## Part 1 — Fix live expansion state after moving a subtree

Ticket 021 found this product inconsistency:

- a node is expanded
- it is moved/reparented into a collapsed parent
- the document correctly remembers that the moved node itself is expanded
- after the destination path is revealed, the moved node may nevertheless
  appear collapsed in the live TreeView
- save/reopen restores it correctly

The persisted state is therefore correct but the live TreeView can temporarily
disagree with it.

Fix this.

## Required behaviour

Expansion state keyed by NodeId remains authoritative.

After a move/reparent:

- if the moved node was expanded before the move, it remains expanded once it
  is visible at its new location
- expanded descendants retain their existing expansion state
- collapsed descendants remain collapsed
- revealing/expanding the new ancestor path must not accidentally collapse the
  moved subtree
- no restart/save/reopen should be required to make the visual state agree
  with Document expansion state

Do not blindly recursively expand the moved subtree.

Restore exactly the expansion state already recorded in Document.

## Move into collapsed parent

If an expanded node is moved into a currently collapsed destination parent:

- the destination parent may remain collapsed if normal UX dictates that
- when that path is subsequently revealed/expanded, the moved node must appear
  with its recorded expansion state

If current move-selection behaviour automatically reveals the destination:

- apply the correct expansion state during that reveal

Avoid relying on stale row numbers immediately after model moves.

Ticket 018 already encountered TreeView row lag after model structure changes;
reuse the robust post-structure restoration mechanism rather than inventing a
parallel solution.

## Expansion invariants

The fix must not:

- change persisted NodeIds
- set document-content dirty state
- create checkpoints
- affect Trash
- trigger autosave solely because expansion was re-applied
- recursively expand nodes whose recorded state is collapsed

No schema change.

Remain on schema v3.

## Tests for the bug

Add permanent regression coverage to the Ticket 021 integration suite.

Cover at least:

1. create an expanded subtree
2. create a collapsed destination parent
3. move/reparent the expanded subtree into that parent
4. reveal/expand the destination path
5. moved node is expanded immediately
6. its previously expanded descendants are expanded
7. previously collapsed descendants remain collapsed
8. Document expansion metadata remains unchanged
9. document content dirty state is unchanged by restoration
10. save/reopen produces the same visible state

Do not use a temporary QML harness.

This regression belongs permanently in the integration suite.

---

# Part 2 — OmaTree source license

License OmaTree under:

    MIT OR Apache-2.0

This is dual licensing "at the recipient's option".

A recipient may choose to use OmaTree under either:

- MIT License
- Apache License, Version 2.0

It does NOT mean they must comply with both simultaneously.

## Cargo metadata

Set:

    license = "MIT OR Apache-2.0"

in the `[package]` section of Cargo.toml.

Do not use the old:

    MIT/Apache-2.0

shorthand.

Use the current SPDX expression.

Do not invent:

- homepage
- repository URL
- author email
- company name

If those are already correctly present, leave them alone.

## License files

Add the standard unmodified license texts:

    LICENSE-MIT
    LICENSE-APACHE

`LICENSE-APACHE` must contain the standard Apache License 2.0 text.

`LICENSE-MIT` must contain the standard MIT License text.

For the MIT copyright line use:

    Copyright (c) 2026 OmaTree contributors

unless the repository already contains an explicit authoritative copyright
holder that should be preserved.

Do not add custom restrictions.

Do not add a Commons Clause.

Do not add non-commercial terms.

Do not modify either license to prohibit proprietary forks, commercial use or
redistribution.

The intention is genuinely permissive open source.

## README

Add a concise License section such as:

    ## License

    OmaTree is licensed under either of:

    - Apache License, Version 2.0
    - MIT License

    at your option.

Link/reference the two repository license files appropriately.

Do not add a long legal essay to README.

---

# Part 3 — Third-party licensing audit

Prepare for binary distribution by auditing what OmaTree actually links,
embeds or distributes.

This is an inventory task.

Do not guess.

Inspect:

- Cargo.toml
- Cargo.lock
- CXX-Qt dependencies
- Qt modules actually linked by the current build
- bundled/native libraries brought in by Rust crates
- SQLite configuration, including the bundled SQLite feature
- TOML parser and other runtime crates

## Create third-party documentation

Add:

    THIRD_PARTY.md

Keep it readable.

At minimum document:

### Qt

Record:

- exact Qt major/minor version currently used for development
- Qt modules OmaTree actually links/uses
- the open-source license applicable to each relevant module, based on
  authoritative Qt/project metadata
- whether OmaTree is dynamically or statically linked to Qt in the normal
  development/release build
- packaging obligations that must be satisfied later

Do not claim OmaTree is fully compliant merely because this file exists.

This ticket prepares the information required for compliance.

For LGPL-covered Qt libraries, explicitly record packaging TODOs such as:

- ship/provide the applicable LGPL license text
- prominently acknowledge use of Qt
- preserve the recipient's LGPL rights
- ensure the distribution method does not prevent replacement/relinking of the
  LGPL libraries where required
- provide the required Qt corresponding source/source offer mechanism as
  required by the applicable license

Do not copy random legal advice from blogs.

Prefer official Qt license information and the license files shipped with the
actual Qt installation/source.

If any Qt module used by OmaTree is GPL-only rather than LGPL-available:

- stop and report it clearly
- do not silently continue under an assumption

### Rust dependencies

Inventory licenses for direct runtime/build dependencies used by OmaTree.

At minimum inspect the licensing of:

- cxx
- cxx-qt
- cxx-qt-lib
- rusqlite
- libsqlite3-sys / bundled SQLite
- toml

and any other direct dependency in Cargo.toml.

Do not manually enumerate hundreds of transitive crates in prose unless needed.

A concise table of direct dependencies plus any transitive dependency requiring
special notice/action is enough.

Flag:

- copyleft dependencies
- non-standard licenses
- NOTICE-file requirements
- dependencies lacking machine-readable license metadata

Do not modify dependencies merely because their license is different from
OmaTree's.

## SQLite

Determine what the bundled SQLite build means for redistribution.

Document its actual licensing/public-domain status based on authoritative
SQLite or crate metadata.

Do not invent an attribution requirement if none exists.

## NOTICE

Do not create a NOTICE file merely because OmaTree is Apache-2.0 licensed.

Only add NOTICE or additional attribution files if:

- an actually distributed dependency requires one, or
- upstream material incorporated into OmaTree requires it

Report the decision.

---

# Part 4 — Packaging identity preparation

Record the packaging identity we already know.

Do not implement packages yet.

The current canonical values are:

    Product display name: OmaTree
    Executable name: omatree
    Notebook extension: .omatree

Determine and document a proposed Linux MIME type.

Preferred if there is no conflict:

    application/x-omatree

Do not register it with the system in this ticket.

## Icon naming

Reserve the conventional icon resource name:

    omatree

but DO NOT create a placeholder graphic.

Do not generate an icon.

Do not use an emoji.

Do not download stock artwork.

Packaging must be able to consume real icon assets later under this name.

## Desktop application ID

Inspect whether the project already has an established reverse-DNS application
ID.

If none exists:

- do not invent ownership of a domain
- do not claim a GitHub organization/user that is not present in repository
  metadata
- document that final desktop/application ID remains to be selected during
  packaging

A simple desktop filename can be chosen later if needed.

---

# Part 5 — Release metadata sanity check

Inspect Cargo/package metadata for obvious release blockers.

Check:

- package name
- version
- edition
- license
- description if already present
- binary name

Do not change the application version purely for this ticket unless it is
clearly an accidental placeholder incompatible with the planned v0.1 release.

If metadata is incomplete but cannot be filled without guessing, report it as
a packaging TODO.

Do not invent author/repository/homepage values.

---

# Documentation

Add a short release/packaging note if useful, for example:

    docs/PACKAGING.md

It may record:

- executable name
- .omatree extension
- proposed MIME type
- icon resource name expected from the future icon work
- unresolved application ID
- third-party licensing checklist
- commands used to build/test release readiness

Keep it practical.

Do not write distro-specific installation instructions yet.

---

# Verification

Run:

- cargo fmt --check
- cargo clippy --all-targets
- cargo test
- cargo build
- cargo build --release

Run:

    cargo test integration_tests

at least once after the expansion-state fix.

Verify:

- all permanent integration tests pass
- the new move/expansion regression passes
- release binary builds
- no QML warnings/binding loops
- no schema change
- no production behaviour changed except the intended expansion-state fix

Inspect the produced release binary sufficiently to determine whether Qt is
being linked dynamically or statically for the third-party audit.

Do not package or install it.

---

# Completion report

Report:

- files modified
- expansion-state bug root cause
- exact fix
- permanent regression scenario added
- total Rust/integration test count
- Cargo license metadata
- license files added
- README license wording
- Qt modules detected
- Qt version detected
- dynamic/static Qt linkage finding
- Qt licensing obligations recorded for packaging
- direct Rust dependency license findings
- SQLite licensing finding
- whether NOTICE is required and why
- proposed MIME type
- icon resource name reserved
- application-ID status
- unresolved information needed for packaging
- fmt/clippy/test/build/release-build results

Do not begin packaging.

Do not generate an icon.

Do not begin another ticket.