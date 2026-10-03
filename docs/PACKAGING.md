# Packaging notes

Facts packaging can rely on, and what is still open. Nothing here is packaged
or registered yet.

## Identity

| Item | Value |
|---|---|
| Display name | OmaTree |
| Executable | `omatree` |
| Notebook extension | `.omatree` (a SQLite database) |
| Proposed MIME type | `application/x-omatree` (no conflict in the locally installed shared-mime-info database; not registered anywhere) |
| Icon resource name | `omatree` (reserved; the artwork will be supplied separately, none exists yet) |
| Desktop file name | to be chosen with the application ID |

## Unresolved (do not guess)

- **Application ID.** None is established. `org.omatree` is only the QML
  module URI used inside the code; it does not mean the project owns that
  domain. A reverse-DNS ID, and the desktop file name that follows from it,
  must be decided from something the project really controls.
- **Cargo metadata missing:** `description`, `repository`, `homepage` and
  `authors` are not set. The Git remote is `https://github.com/Dorotabro/OmaTree.git`,
  but it has not been declared as the project's repository, so it is not
  written into `Cargo.toml`.
- **Version** is `0.1.0`, edition 2021, binary `omatree`; nothing there blocks
  a v0.1 release.
- **Copyright holder:** `LICENSE-MIT` names "OmaTree contributors".
- **MIME registration, icon, desktop entry, file association** all wait for
  the packaging ticket.

## Licensing

See [`THIRD_PARTY.md`](../THIRD_PARTY.md): Qt 6.11.2 under LGPL-3.0, linked
dynamically; SQLite bundled (public domain); all Rust crates permissive; no
NOTICE required.

## Release-readiness commands

    cargo fmt --check
    cargo clippy --all-targets
    cargo test                        # unit and integration tests
    cargo test integration_tests      # integration tests only
    cargo build --release
    readelf -d target/release/omatree | grep NEEDED    # Qt linkage audit
