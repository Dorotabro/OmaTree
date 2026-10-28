# Upstream licence texts (ticket E12c)

Sixteen crates in the egui build declare a licence expression in their manifests but do **not** ship
the licence file inside the published crate (the editor library, a seventeenth, has no licence at all
and is a maintainer decision: see `THIRD_PARTY.md`). So that the packages carry the notice, the texts
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
