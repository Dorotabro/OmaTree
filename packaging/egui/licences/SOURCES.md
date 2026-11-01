# Upstream licence texts (ticket E12c)

Sixteen crates in the egui build declare a licence expression in their manifests but do **not** ship
the licence file inside the published crate (the editor library, a workspace crate since P2, declares
`MIT OR Apache-2.0` like OmaTree; confirming its ownership facts is still a maintainer decision: see `THIRD_PARTY.md`). So that the packages carry the notice, the texts
below were copied unmodified from the crates' own upstream repositories on 2026-10-27 and are added
to `third-party-licenses.txt` by `packaging/egui/third-party.py`. This supplies attribution material;
it is not a licensing decision and it does not replace checking the published crate.

| File here | Upstream file | Ref | Crates it stands in for |
|---|---|---|---|
| `egui-LICENSE-MIT`, `egui-LICENSE-APACHE` | `emilk/egui` `LICENSE-MIT`, `LICENSE-APACHE` | tag `0.37.0` (the version in `Cargo.lock`) | egui, eframe, epaint, emath, ecolor, egui-winit, egui_glow, egui_system_font_provider |
| `accesskit-LICENSE-MIT`, `accesskit-LICENSE-APACHE` | `AccessKit/accesskit` `LICENSE-MIT`, `LICENSE-APACHE` | tag `accesskit-v0.25.1` | accesskit, accesskit_consumer, accesskit_atspi_common, accesskit_unix, accesskit_winit (declared `Apache-2.0` only: its Apache text applies) |
| `krilla-LICENSE_MIT`, `krilla-LICENSE_APACHE` | `LaurenzV/krilla` `LICENSE_MIT`, `LICENSE_APACHE` | branch `main` (no tag matching 0.8.2 was found) | krilla |
| `profiling-LICENSE-MIT`, `profiling-LICENSE-APACHE` | `aclysma/profiling` `LICENSE-MIT`, `LICENSE-APACHE` | branch `master` (no matching tag found) | profiling |
| `gl-rs-LICENSE` | `brendanzab/gl-rs` `LICENSE` (Apache-2.0) | branch `master` | gl_generator, khronos_api |

Where the ref is a branch rather than the tag of the locked version, the text is the repository's current
one, which may differ from the published crate's in the copyright line; that is recorded here rather than
assumed away. A licence file inside a published crate, where there is one, always takes precedence.

## Verification against the exact published commits (R3, 2026-10-31)

The rows above that name a branch were re-checked against the commit that each locked crate was published from.
A published crate records it in its own `.cargo_vcs_info.json` (read from the crate sources in the Cargo home, as
resolved by `Cargo.lock`), and the licence files were fetched from GitHub at that commit and compared byte for byte
(ignoring line endings) with the copies in this directory:

| Crate (locked version) | Commit recorded by the crate | Files compared | Result |
|---|---|---|---|
| krilla 0.8.2 | `3ffdf0588cf98050aad6edba51ca70162e1fb5b5` (`crates/krilla`) | `LICENSE_MIT`, `LICENSE_APACHE` | identical |
| profiling 1.0.18 | `8271551172eb6fa4cba47369aedd93790c623df9` (`profiling`) | `LICENSE-MIT`, `LICENSE-APACHE` | identical |
| gl_generator 0.14.0 | `ea503e8d5fb6d73c6030e6191ce738cd3bf3433e` | `LICENSE` | identical |
| khronos_api 3.1.0 | `f150967b1c44ae888e6676f93f639ebc82771bdc` | `LICENSE` | identical |

Nothing in this directory was replaced. The copyright lines are therefore those of the repositories at the commits the
crates were built from (krilla: "Copyright (c) 2024 Laurenz Stampfl"; profiling: "Copyright (c) 2020 Philip Degarmo and
other contributors"). The published crates still do not contain these files themselves, so they remain upstream copies,
now tied to an exact commit instead of a branch head.
