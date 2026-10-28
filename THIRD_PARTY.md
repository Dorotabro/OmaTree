# Third-party software

This is an inventory prepared for binary distribution. It records what OmaTree
uses and what a package must do. It is **not** legal advice.

OmaTree currently ships as two applications from this repository, and this file covers both:

- **the egui version** (`omatree-egui`, the migration target; no Qt): the next section, with a
  crate table generated from the resolved dependency graph;
- **the legacy Qt version** (`omatree`, until the E13 cut-over): every section from "Legacy Qt
  application" on. Those sections describe Qt packaging only and do **not** apply to the egui
  packages.

OmaTree itself is `MIT OR Apache-2.0` (see `LICENSE-MIT`, `LICENSE-APACHE`).

## The egui version (`omatree-egui`)

Evidence: `cargo tree --locked -p omatree-egui --target x86_64-unknown-linux-gnu -e normal,build`
for the compiled set, `cargo metadata --locked` for each crate's declared licence expression, and
each crate's own source for its licence files, all read by `packaging/egui/third-party.py`
(OmaTree 0.1.2, Rust 1.99, Linux x86_64). The table below is generated and
`packaging/egui/tests/verify-sources.sh` fails if it is stale; the packages additionally ship
`third-party-licenses.txt`, built by the same script, which holds every crate's licence and notice
files (each distinct text once, with the crates it came from).

- **Linkage.** The executable is dynamically linked only to `libc`, `libm`, `libgcc_s` and the
  loader. Wayland, X11/XCB, xkbcommon, EGL/OpenGL, fontconfig and `libdbus-1` are opened at run time
  (`dlopen`) from the system, so no copy of them is distributed and none is a licence matter of
  these packages. **No Qt, CXX-Qt or QML component is linked, bundled or required.**
- **SQLite** is compiled in (`rusqlite` with `bundled`, SQLite 3.53.2 inside `libsqlite3-sys`), public
  domain, no attribution text required.
- **Fonts compiled into the binary** (`epaint_default_fonts`, licence expression
  `(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0`): Ubuntu-Light, Hack, Noto Emoji and the egui
  and emoji icon fonts. Their texts (`OFL.txt`, `UFL.txt`, the Hack and emoji-icon-font licences)
  are in `third-party-licenses.txt`. The SIL OFL and the Ubuntu Font Licence both ask that their text
  accompany redistributed copies of the fonts, which is why that bundle is installed in every package.
- **Licences with an attached-notice duty in binary form:** MIT, BSD-2/3-Clause, ISC, Zlib (no duty
  for binaries), Apache-2.0 (licence text; `subsetter` ships a `NOTICE` file, which is in the
  bundle, and no other compiled crate does), Unicode-3.0 (the `icu_*` and `unicode-ident` family: licence text must accompany),
  OFL-1.1 and Ubuntu-font-1.0 (above). All are in `third-party-licenses.txt`.
- **Dual or alternative licences are taken under their permissive option:** `Apache-2.0 OR
  GPL-2.0-only` (apply Apache-2.0), `MIT OR Apache-2.0 OR Zlib`, `Unlicense OR MIT` and similar. No
  crate in the compiled set is GPL/LGPL/AGPL-only.

**Flagged for manual review (not resolved by this inventory):**

1. `omatree-egui-editor-spike` 0.1.0 (the pinned editor library, repository
   `Dorotabro/omatree-egui-editor-spike`, rev `30a95dd43064232dc1ac04315c8020e2307a223c`): its
   `Cargo.toml` has **no `license` field and the repository has no licence file**. It is the project
   maintainer's own code, so it is presumably under the same terms as OmaTree, but no file says so;
   the maintainer should add a licence to that repository (a new pin, outside E12b). Nothing is
   assumed in the package.
2. Sixteen crates declare a licence expression but ship no licence file inside the published crate (the
   egui family: `egui`, `eframe`, `epaint`, `emath`, `ecolor`, `egui-winit`, `egui_glow`,
   `egui_system_font_provider`; the `accesskit` family: `accesskit`, `accesskit_consumer`,
   `accesskit_atspi_common`, `accesskit_unix`, `accesskit_winit`; `krilla`, `profiling`, `gl_generator`,
   `khronos_api`). **Status after E12c: notice material supplied.** The upstream repositories' own licence
   texts were copied unmodified into `packaging/egui/licences/` (provenance, refs and hashes-by-source in
   `SOURCES.md`) and are included in `third-party-licenses.txt`, marked as upstream copies. For egui (tag
   `0.37.0`, the locked version) and accesskit (tag `accesskit-v0.25.1`) the ref is the release; for
   `krilla`, `profiling` and `gl-rs` no matching tag was found, so the default branch's text was used and
   its copyright line may differ from the published crate's. AccessKit's MIT file carries no copyright
   line upstream either. This is attribution material, not a legal conclusion.

The crates directly depended on by `omatree-egui` that matter most: `eframe`/`egui` 0.37 (UI, with
the `accesskit` feature added in E12a: AccessKit, AT-SPI through `zbus`/`atspi`), `omatree_editor`
(the pinned editor), `rfd` 0.17 (file dialogs through the XDG desktop portal, `xdg-portal` + `pollster`,
no GTK), `image` 0.25 (JPEG), `rusqlite` 0.40 (via `omatree-core`), `krilla` and `zip` (PDF and ZIP
export, via `omatree-core`).

<!-- BEGIN GENERATED: egui-crates -->
301 crates are compiled into `omatree-egui` (excluding `omatree-egui` and `omatree-core`, which are OmaTree's own); 6 are direct dependencies (**bold**).

| Crate | Version | License expression | Source |
|---|---|---|---|
| accesskit | 0.25.1 | MIT OR Apache-2.0 | crates.io |
| accesskit_atspi_common | 0.21.0 | MIT OR Apache-2.0 | crates.io |
| accesskit_consumer | 0.39.1 | MIT OR Apache-2.0 | crates.io |
| accesskit_unix | 0.24.0 | MIT OR Apache-2.0 | crates.io |
| accesskit_winit | 0.34.1 | Apache-2.0 | crates.io |
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 | crates.io |
| ahash | 0.8.12 | MIT OR Apache-2.0 | crates.io |
| arboard | 3.7.0 | MIT OR Apache-2.0 | crates.io |
| arrayref | 0.3.9 | BSD-2-Clause | crates.io |
| arrayvec | 0.7.8 | MIT OR Apache-2.0 | crates.io |
| as-raw-xcb-connection | 1.0.1 | MIT OR Apache-2.0 | crates.io |
| async-broadcast | 0.7.2 | MIT OR Apache-2.0 | crates.io |
| async-channel | 2.5.0 | Apache-2.0 OR MIT | crates.io |
| async-executor | 1.14.0 | Apache-2.0 OR MIT | crates.io |
| async-io | 2.6.0 | Apache-2.0 OR MIT | crates.io |
| async-lock | 3.4.2 | Apache-2.0 OR MIT | crates.io |
| async-process | 2.5.0 | Apache-2.0 OR MIT | crates.io |
| async-recursion | 1.2.0 | MIT OR Apache-2.0 | crates.io |
| async-signal | 0.2.14 | Apache-2.0 OR MIT | crates.io |
| async-task | 4.7.1 | Apache-2.0 OR MIT | crates.io |
| async-trait | 0.1.92 | MIT OR Apache-2.0 | crates.io |
| atomic-waker | 1.1.2 | Apache-2.0 OR MIT | crates.io |
| atspi | 0.29.0 | Apache-2.0 OR MIT | crates.io |
| atspi-common | 0.13.0 | Apache-2.0 OR MIT | crates.io |
| atspi-proxies | 0.13.0 | Apache-2.0 OR MIT | crates.io |
| autocfg | 1.5.1 | Apache-2.0 OR MIT | crates.io |
| base64 | 0.22.1 | MIT OR Apache-2.0 | crates.io |
| bitflags | 2.13.2 | MIT OR Apache-2.0 | crates.io |
| blocking | 1.7.0 | Apache-2.0 OR MIT | crates.io |
| bumpalo | 3.20.3 | MIT OR Apache-2.0 | crates.io |
| bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT | crates.io |
| bytemuck_derive | 1.12.1 | Zlib OR Apache-2.0 OR MIT | crates.io |
| byteorder-lite | 0.1.0 | Unlicense OR MIT | crates.io |
| calloop | 0.13.0 | MIT | crates.io |
| calloop | 0.14.5 | MIT | crates.io |
| calloop-wayland-source | 0.3.0 | MIT | crates.io |
| calloop-wayland-source | 0.4.1 | MIT | crates.io |
| cc | 1.5.1 | MIT OR Apache-2.0 | crates.io |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | crates.io |
| cfg_aliases | 0.2.2 | MIT | crates.io |
| **chrono** | 0.4.45 | MIT OR Apache-2.0 | crates.io |
| color | 0.3.3 | Apache-2.0 OR MIT | crates.io |
| color_quant | 1.1.0 | MIT | crates.io |
| concurrent-queue | 2.5.0 | Apache-2.0 OR MIT | crates.io |
| core_maths | 0.1.1 | MIT | crates.io |
| crc32fast | 1.5.3 | MIT OR Apache-2.0 | crates.io |
| crossbeam-utils | 0.8.23 | MIT OR Apache-2.0 | crates.io |
| cursor-icon | 1.2.0 | MIT OR Apache-2.0 OR Zlib | crates.io |
| displaydoc | 0.2.7 | MIT OR Apache-2.0 | crates.io |
| dlib | 0.5.3 | MIT | crates.io |
| document-features | 0.2.12 | MIT OR Apache-2.0 | crates.io |
| downcast-rs | 1.2.1 | MIT/Apache-2.0 | crates.io |
| dpi | 0.1.2 | Apache-2.0 AND MIT | crates.io |
| ecolor | 0.37.0 | MIT OR Apache-2.0 | crates.io |
| **eframe** | 0.37.0 | MIT OR Apache-2.0 | crates.io |
| egui | 0.37.0 | MIT OR Apache-2.0 | crates.io |
| egui-winit | 0.37.0 | MIT OR Apache-2.0 | crates.io |
| egui_glow | 0.37.0 | MIT OR Apache-2.0 | crates.io |
| egui_system_font_provider | 0.37.0 | MIT OR Apache-2.0 | crates.io |
| either | 1.19.0 | MIT OR Apache-2.0 | crates.io |
| emath | 0.37.0 | MIT OR Apache-2.0 | crates.io |
| emojis | 0.9.1 | (MIT OR Apache-2.0) AND Unicode-3.0 | crates.io |
| endi | 1.1.1 | MIT | crates.io |
| enumflags2 | 0.7.12 | MIT OR Apache-2.0 | crates.io |
| enumflags2_derive | 0.7.12 | MIT OR Apache-2.0 | crates.io |
| epaint | 0.37.0 | MIT OR Apache-2.0 | crates.io |
| epaint_default_fonts | 0.37.0 | (MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0 | crates.io |
| equivalent | 1.0.2 | Apache-2.0 OR MIT | crates.io |
| errno | 0.3.14 | MIT OR Apache-2.0 | crates.io |
| euclid | 0.22.14 | MIT OR Apache-2.0 | crates.io |
| event-listener | 5.4.2 | Apache-2.0 OR MIT | crates.io |
| event-listener-strategy | 0.5.4 | Apache-2.0 OR MIT | crates.io |
| fallible-iterator | 0.3.0 | MIT/Apache-2.0 | crates.io |
| fallible-streaming-iterator | 0.1.9 | MIT/Apache-2.0 | crates.io |
| fastrand | 2.5.0 | Apache-2.0 OR MIT | crates.io |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 | crates.io |
| fearless_simd | 0.4.1 | Apache-2.0 OR MIT | crates.io |
| find-msvc-tools | 0.1.14 | MIT OR Apache-2.0 | crates.io |
| fixedbitset | 0.5.7 | MIT OR Apache-2.0 | crates.io |
| flate2 | 1.1.10 | MIT OR Apache-2.0 | crates.io |
| float-cmp | 0.9.0 | MIT | crates.io |
| foldhash | 0.1.5 | Zlib | crates.io |
| foldhash | 0.2.0 | Zlib | crates.io |
| font-types | 0.11.3 | MIT OR Apache-2.0 | crates.io |
| font-types | 0.12.7 | MIT OR Apache-2.0 | crates.io |
| fontique | 0.11.1 | Apache-2.0 OR MIT | crates.io |
| form_urlencoded | 1.2.2 | MIT OR Apache-2.0 | crates.io |
| futures-core | 0.3.34 | MIT OR Apache-2.0 | crates.io |
| futures-io | 0.3.34 | MIT OR Apache-2.0 | crates.io |
| futures-lite | 2.6.1 | Apache-2.0 OR MIT | crates.io |
| futures-macro | 0.3.34 | MIT OR Apache-2.0 | crates.io |
| futures-task | 0.3.34 | MIT OR Apache-2.0 | crates.io |
| futures-util | 0.3.34 | MIT OR Apache-2.0 | crates.io |
| gethostname | 1.1.0 | Apache-2.0 | crates.io |
| getrandom | 0.3.4 | MIT OR Apache-2.0 | crates.io |
| getrandom | 0.4.3 | MIT OR Apache-2.0 | crates.io |
| gif | 0.14.2 | MIT OR Apache-2.0 | crates.io |
| gl_generator | 0.14.0 | Apache-2.0 | crates.io |
| glifo | 0.3.0 | Apache-2.0 OR MIT | crates.io |
| glow | 0.17.0 | MIT OR Apache-2.0 OR Zlib | crates.io |
| glutin | 0.32.3 | Apache-2.0 | crates.io |
| glutin-winit | 0.5.0 | MIT | crates.io |
| glutin_egl_sys | 0.7.1 | Apache-2.0 | crates.io |
| glutin_glx_sys | 0.6.1 | Apache-2.0 | crates.io |
| guillotiere | 0.7.0 | MIT/Apache-2.0 | crates.io |
| harfrust | 0.12.0 | MIT | crates.io |
| hashbrown | 0.15.5 | MIT OR Apache-2.0 | crates.io |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 | crates.io |
| hashlink | 0.12.2 | MIT OR Apache-2.0 | crates.io |
| hex | 0.4.3 | MIT OR Apache-2.0 | crates.io |
| iana-time-zone | 0.1.65 | MIT OR Apache-2.0 | crates.io |
| icu_collections | 2.4.0 | Unicode-3.0 | crates.io |
| icu_locale_core | 2.4.0 | Unicode-3.0 | crates.io |
| icu_normalizer | 2.4.0 | Unicode-3.0 | crates.io |
| icu_normalizer_data | 2.4.0 | Unicode-3.0 | crates.io |
| icu_properties | 2.4.0 | Unicode-3.0 | crates.io |
| icu_properties_data | 2.4.0 | Unicode-3.0 | crates.io |
| icu_provider | 2.4.0 | Unicode-3.0 | crates.io |
| idna | 1.1.0 | MIT OR Apache-2.0 | crates.io |
| idna_adapter | 1.2.2 | Apache-2.0 OR MIT | crates.io |
| **image** | 0.25.10 | MIT OR Apache-2.0 | crates.io |
| image-webp | 0.2.4 | MIT OR Apache-2.0 | crates.io |
| imagesize | 0.14.0 | MIT | crates.io |
| indexmap | 2.14.2 | Apache-2.0 OR MIT | crates.io |
| itertools | 0.15.0 | MIT OR Apache-2.0 | crates.io |
| itoa | 1.0.18 | MIT OR Apache-2.0 | crates.io |
| khronos_api | 3.1.0 | Apache-2.0 | crates.io |
| krilla | 0.8.2 | MIT OR Apache-2.0 | crates.io |
| kurbo | 0.13.1 | Apache-2.0 OR MIT | crates.io |
| libc | 0.2.189 | MIT OR Apache-2.0 | crates.io |
| libloading | 0.8.9 | ISC | crates.io |
| libm | 0.2.16 | MIT | crates.io |
| libsqlite3-sys | 0.38.2 | MIT | crates.io |
| linebender_resource_handle | 0.1.1 | Apache-2.0 OR MIT | crates.io |
| linux-raw-sys | 0.12.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | crates.io |
| linux-raw-sys | 0.4.15 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | crates.io |
| litemap | 0.8.3 | Unicode-3.0 | crates.io |
| litrs | 1.0.1 | MIT OR Apache-2.0 | crates.io |
| lock_api | 0.4.14 | MIT OR Apache-2.0 | crates.io |
| log | 0.4.34 | MIT OR Apache-2.0 | crates.io |
| memchr | 2.8.3 | Unlicense OR MIT | crates.io |
| memmap2 | 0.9.12 | MIT OR Apache-2.0 | crates.io |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 | crates.io |
| miniz_oxide | 0.9.1 | MIT OR Zlib OR Apache-2.0 | crates.io |
| moxcms | 0.8.1 | BSD-3-Clause OR Apache-2.0 | crates.io |
| nohash-hasher | 0.2.0 | Apache-2.0 OR MIT | crates.io |
| nom | 8.0.0 | MIT | crates.io |
| num-traits | 0.2.19 | MIT OR Apache-2.0 | crates.io |
| **omatree-egui-editor-spike** | 0.1.0 | MISSING | git (pinned rev) |
| once_cell | 1.21.4 | MIT OR Apache-2.0 | crates.io |
| ordered-stream | 0.2.0 | MIT OR Apache-2.0 | crates.io |
| os_pipe | 1.2.3 | MIT | crates.io |
| parking | 2.2.1 | Apache-2.0 OR MIT | crates.io |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 | crates.io |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 | crates.io |
| parlance | 0.1.1 | Apache-2.0 OR MIT | crates.io |
| pdf-writer | 0.15.0 | MIT OR Apache-2.0 | crates.io |
| peniko | 0.6.1 | Apache-2.0 OR MIT | crates.io |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 | crates.io |
| petgraph | 0.8.3 | MIT OR Apache-2.0 | crates.io |
| phf | 0.13.1 | MIT | crates.io |
| phf | 0.14.0 | MIT | crates.io |
| phf_generator | 0.14.0 | MIT | crates.io |
| phf_macros | 0.14.0 | MIT | crates.io |
| phf_shared | 0.13.1 | MIT | crates.io |
| phf_shared | 0.14.0 | MIT | crates.io |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT | crates.io |
| piper | 0.2.5 | MIT OR Apache-2.0 | crates.io |
| pkg-config | 0.3.34 | MIT OR Apache-2.0 | crates.io |
| png | 0.18.1 | MIT OR Apache-2.0 | crates.io |
| poll-promise | 0.3.0 | MIT OR Apache-2.0 | crates.io |
| polling | 3.11.0 | Apache-2.0 OR MIT | crates.io |
| pollster | 0.4.0 | Apache-2.0/MIT | crates.io |
| polycool | 0.4.0 | MIT OR Apache-2.0 | crates.io |
| potential_utf | 0.1.6 | Unicode-3.0 | crates.io |
| proc-macro-crate | 3.5.0 | MIT OR Apache-2.0 | crates.io |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | crates.io |
| profiling | 1.0.18 | MIT OR Apache-2.0 | crates.io |
| pulldown-cmark | 0.13.5 | MIT | crates.io |
| pxfm | 0.1.30 | BSD-3-Clause OR Apache-2.0 | crates.io |
| quick-error | 2.0.1 | MIT/Apache-2.0 | crates.io |
| quick-xml | 0.41.0 | MIT | crates.io |
| quote | 1.0.47 | MIT OR Apache-2.0 | crates.io |
| **raw-window-handle** | 0.6.2 | MIT OR Apache-2.0 OR Zlib | crates.io |
| read-fonts | 0.39.2 | MIT OR Apache-2.0 | crates.io |
| read-fonts | 0.41.0 | MIT OR Apache-2.0 | crates.io |
| **rfd** | 0.17.2 | MIT | crates.io |
| rusqlite | 0.40.2 | MIT | crates.io |
| rustc-hash | 2.1.3 | Apache-2.0 OR MIT | crates.io |
| rustix | 0.38.44 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | crates.io |
| rustix | 1.1.5 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | crates.io |
| rustybuzz | 0.20.1 | MIT | crates.io |
| ryu | 1.0.23 | Apache-2.0 OR BSL-1.0 | crates.io |
| scoped-tls | 1.0.1 | MIT/Apache-2.0 | crates.io |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 | crates.io |
| self_cell | 1.3.0 | Apache-2.0 OR GPL-2.0-only | crates.io |
| serde | 1.0.229 | MIT OR Apache-2.0 | crates.io |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | crates.io |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | crates.io |
| serde_repr | 0.1.21 | MIT OR Apache-2.0 | crates.io |
| serde_spanned | 1.1.1 | MIT OR Apache-2.0 | crates.io |
| shlex | 2.0.1 | MIT OR Apache-2.0 | crates.io |
| signal-hook-registry | 1.4.8 | MIT OR Apache-2.0 | crates.io |
| simd-adler32 | 0.3.10 | MIT | crates.io |
| siphasher | 1.0.4 | MIT OR Apache-2.0 | crates.io |
| skrifa | 0.42.1 | MIT OR Apache-2.0 | crates.io |
| skrifa | 0.44.0 | MIT OR Apache-2.0 | crates.io |
| slab | 0.4.12 | MIT | crates.io |
| smallvec | 1.16.2 | MIT OR Apache-2.0 | crates.io |
| smithay-client-toolkit | 0.19.2 | MIT | crates.io |
| smithay-client-toolkit | 0.20.0 | MIT | crates.io |
| smithay-clipboard | 0.7.3 | MIT | crates.io |
| smol_str | 0.2.2 | MIT OR Apache-2.0 | crates.io |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 | crates.io |
| static_assertions | 1.1.0 | MIT OR Apache-2.0 | crates.io |
| strict-num | 0.1.1 | MIT | crates.io |
| subsetter | 0.2.6 | MIT OR Apache-2.0 | crates.io |
| syn | 2.0.119 | MIT OR Apache-2.0 | crates.io |
| syn | 3.0.6 | MIT OR Apache-2.0 | crates.io |
| synstructure | 0.14.0 | MIT | crates.io |
| sys-locale | 0.3.2 | MIT OR Apache-2.0 | crates.io |
| thiserror | 1.0.69 | MIT OR Apache-2.0 | crates.io |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | crates.io |
| thiserror-impl | 1.0.69 | MIT OR Apache-2.0 | crates.io |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 | crates.io |
| tiny-skia-path | 0.12.0 | BSD-3-Clause | crates.io |
| tinystr | 0.8.5 | Unicode-3.0 | crates.io |
| toml | 1.1.6+spec-1.1.0 | MIT OR Apache-2.0 | crates.io |
| toml_datetime | 1.1.1+spec-1.1.0 | MIT OR Apache-2.0 | crates.io |
| toml_edit | 0.25.15+spec-1.1.0 | MIT OR Apache-2.0 | crates.io |
| toml_parser | 1.1.3+spec-1.1.0 | MIT OR Apache-2.0 | crates.io |
| tracing | 0.1.44 | MIT | crates.io |
| tracing-attributes | 0.1.31 | MIT | crates.io |
| tracing-core | 0.1.36 | MIT | crates.io |
| tree_magic_mini | 3.2.2 | MIT | crates.io |
| ttf-parser | 0.25.1 | MIT OR Apache-2.0 | crates.io |
| typed-path | 0.13.0 | MIT OR Apache-2.0 | crates.io |
| unicase | 2.10.0 | MIT OR Apache-2.0 | crates.io |
| unicode-bidi-mirroring | 0.4.0 | MIT/Apache-2.0 | crates.io |
| unicode-ccc | 0.4.0 | MIT/Apache-2.0 | crates.io |
| unicode-general-category | 1.1.0 | Apache-2.0 | crates.io |
| unicode-ident | 1.0.26 | (MIT OR Apache-2.0) AND Unicode-3.0 | crates.io |
| unicode-properties | 0.1.4 | MIT/Apache-2.0 | crates.io |
| unicode-script | 0.5.8 | MIT OR Apache-2.0 | crates.io |
| unicode-segmentation | 1.13.3 | MIT OR Apache-2.0 | crates.io |
| url | 2.5.8 | MIT OR Apache-2.0 | crates.io |
| utf8_iter | 1.0.4 | Apache-2.0 OR MIT | crates.io |
| uuid | 1.29.0 | Apache-2.0 OR MIT | crates.io |
| vcpkg | 0.2.15 | MIT/Apache-2.0 | crates.io |
| vello_common | 0.2.0 | Apache-2.0 OR MIT | crates.io |
| vello_cpu | 0.2.0 | Apache-2.0 OR MIT | crates.io |
| version_check | 0.9.5 | MIT/Apache-2.0 | crates.io |
| wayland-backend | 0.3.17 | MIT | crates.io |
| wayland-client | 0.31.15 | MIT | crates.io |
| wayland-csd-frame | 0.3.0 | MIT | crates.io |
| wayland-cursor | 0.31.14 | MIT | crates.io |
| wayland-protocols | 0.32.13 | MIT | crates.io |
| wayland-protocols-experimental | 20250721.0.1 | MIT | crates.io |
| wayland-protocols-misc | 0.3.12 | MIT | crates.io |
| wayland-protocols-plasma | 0.3.12 | MIT | crates.io |
| wayland-protocols-wlr | 0.3.12 | MIT | crates.io |
| wayland-scanner | 0.31.11 | MIT | crates.io |
| wayland-sys | 0.31.11 | MIT | crates.io |
| web-time | 1.1.0 | MIT OR Apache-2.0 | crates.io |
| webbrowser | 1.2.5 | MIT OR Apache-2.0 | crates.io |
| weezl | 0.1.12 | MIT OR Apache-2.0 | crates.io |
| winit | 0.30.13 | Apache-2.0 | crates.io |
| winnow | 1.0.4 | MIT | crates.io |
| wl-clipboard-rs | 0.9.4 | MIT OR Apache-2.0 | crates.io |
| write-fonts | 0.48.1 | MIT OR Apache-2.0 | crates.io |
| writeable | 0.6.4 | Unicode-3.0 | crates.io |
| x11-dl | 2.21.0 | MIT | crates.io |
| x11rb | 0.13.2 | MIT OR Apache-2.0 | crates.io |
| x11rb-protocol | 0.13.2 | MIT OR Apache-2.0 | crates.io |
| xcursor | 0.3.11 | MIT | crates.io |
| xkbcommon-dl | 0.4.2 | MIT | crates.io |
| xkeysym | 0.2.1 | MIT OR Apache-2.0 OR Zlib | crates.io |
| xml-rs | 0.8.29 | MIT | crates.io |
| xmp-writer | 0.3.3 | MIT OR Apache-2.0 | crates.io |
| yeslogic-fontconfig-sys | 6.0.1 | MIT | crates.io |
| yoke | 0.8.4 | Unicode-3.0 | crates.io |
| yoke-derive | 0.8.4 | Unicode-3.0 | crates.io |
| zbus | 5.19.0 | MIT | crates.io |
| zbus-lockstep | 0.5.2 | MIT | crates.io |
| zbus-lockstep-macros | 0.5.2 | MIT | crates.io |
| zbus_macros | 5.19.0 | MIT | crates.io |
| zbus_names | 4.3.5 | MIT | crates.io |
| zbus_xml | 5.2.1 | MIT | crates.io |
| zcheapstr | 1.1.0 | MIT | crates.io |
| zerocopy | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | crates.io |
| zerofrom | 0.1.8 | Unicode-3.0 | crates.io |
| zerofrom-derive | 0.1.8 | Unicode-3.0 | crates.io |
| zerotrie | 0.2.5 | Unicode-3.0 | crates.io |
| zerovec | 0.11.9 | Unicode-3.0 | crates.io |
| zerovec-derive | 0.11.7 | Unicode-3.0 | crates.io |
| zip | 9.1.0 | MIT | crates.io |
| zune-core | 0.5.3 | MIT OR Apache-2.0 OR Zlib | crates.io |
| zune-jpeg | 0.5.15 | MIT OR Apache-2.0 OR Zlib | crates.io |
| zvariant | 5.15.1 | MIT | crates.io |
| zvariant_derive | 5.15.1 | MIT | crates.io |
| zvariant_utils | 4.2.0 | MIT | crates.io |
<!-- END GENERATED: egui-crates -->

# Legacy Qt application

Everything from here to the end of the file describes the Qt application (`omatree`) and its
packages. Facts below come from the installed Qt files' SPDX headers and license texts,
`Cargo.lock`/`cargo metadata`, and the crates' own sources, as of OmaTree 0.1.2 on the development
machine (Arch Linux). The macOS app bundle and Windows installer are covered at the end. None of
it applies to the egui packages.

## Qt

- **Version:** Qt 6.11.2 (Arch packages `qt6-base 6.11.2-3`, `qt6-declarative 6.11.2-1`).
- **Linkage:** dynamic. The release executable (`target/release/omatree`) has
  `NEEDED` entries for `libQt6Core`, `libQt6Gui`, `libQt6Qml` (plus
  `libstdc++`, `libgcc_s`, libm, libc). Qt Quick and the QML modules are
  loaded at run time through the QML engine. Nothing from Qt is statically
  linked or copied into the executable.
- **Modules OmaTree uses.** Directly linked: Core, Gui, Qml. Loaded at run
  time by the QML it imports (`QtQuick`, `QtQuick.Controls` with the Basic
  style, `QtQuick.Layouts`, `QtQuick.Dialogs`): Qt Quick, Quick Templates 2,
  Quick Controls 2 (+ Basic style and Impl), Quick Layouts, Quick Dialogs 2,
  QmlModels, QmlMeta, QmlWorkerScript. These pull in Network, DBus and
  OpenGL. `QtTest` is used only by the test suite and is not distributed.
- **Other libraries seen loaded on this machine** (Widgets, Svg,
  WaylandClient, the GTK3 platform theme, an input-method plugin) come from
  the desktop's Qt platform plugins, not from OmaTree's own dependencies; a
  package must decide separately which Qt platform plugins it ships or
  requires.
- **License of every module above:** the headers of each carry
  `LicenseRef-Qt-Commercial OR LGPL-3.0-only OR GPL-2.0-only OR GPL-3.0-only`,
  so each is available under **LGPL-3.0-only** (the license OmaTree would use;
  its text is in the Qt packages: `/usr/share/licenses/qt6-base/LGPL-3.0-only.txt`).
  Minor exceptions inside these modules are a few files under other permissive
  or LGPL-2.1+ terms (for example Unicode-3.0, BSD-3-Clause), and one QtQml
  *header* file licensed `GPL-3.0-only WITH Qt-GPL-exception-1.0`, which is a
  build-time development header, not part of the shipped libraries.
- **No GPL-only Qt module is used.** (Checked: none of the modules above is
  GPL-only; GPL-only Qt add-ons such as Charts or Virtual Keyboard are not used.)

### What a package that bundles Qt must satisfy

These follow from the LGPL-3.0 text and Qt's open-source licensing; verify
them against the official Qt licensing documentation for the exact release
when packaging.

- Provide the LGPL-3.0 license text (and the GPL-3.0 text it builds on) with
  the distribution, and say that OmaTree uses Qt, with a pointer to the
  license, in a place users can find (for example an About/credits text or
  the package documentation).
- Do not restrict recipients' rights under the LGPL: keep OmaTree's own
  license terms compatible, and do not prohibit modification of the Qt
  libraries for the recipient's own use, or reverse engineering to debug such
  modifications.
- Keep Qt replaceable: link dynamically (as now), ship or depend on separate
  Qt shared libraries, and do not use a packaging format that prevents a user
  from running OmaTree against their own build of Qt where the license
  requires it. A distro package depending on the distro's Qt does this
  naturally; a self-contained bundle, AppImage or Flatpak needs checking.
- Provide the corresponding source for any Qt libraries OmaTree itself
  distributes (written offer or a link to the exact source version), and keep
  any local patches to Qt available. Not needed if Qt is only a dependency
  installed from the distro.
- Do not use Qt's trademarks beyond acknowledging Qt.

### Qt in the AppImage

The Arch and Debian/Ubuntu packages use the system's Qt and contain none. The
**AppImage is the one format that redistributes Qt**, so for it the obligations
above apply, and are met as follows. This restates facts; it adds no legal
conclusion beyond the audit above.

- **Qt version and origin:** Qt 6.8.3, the official unmodified binary release
  from <https://download.qt.io/> (installed with a pinned `aqtinstall`), bundled
  with ICU 73.2 (Unicode license), which that Qt build links. Qt has no local
  patches.
- **Modules and plugins bundled:** listed in the AppImage's
  `usr/share/doc/omatree/QT-NOTICE.md` (source: `packaging/appimage/QT-NOTICE.md`).
  The AppImage build checks that every bundled `libQt6*` module is named there.
- **Dynamic linking:** unchanged. `omatree` links `libQt6Core`, `libQt6Gui` and
  `libQt6Qml` as shared libraries; nothing is static.
- **Licence texts:** `Qt-LGPL-3.0.txt`, `Qt-GPL-3.0.txt`, `ICU-73.2-LICENSE.txt`,
  and the copyright files of the other libraries copied from the build system
  (`third-party/`, with `BUNDLED-LIBRARIES.txt`), in `usr/share/licenses/omatree/`.
- **Replaceability and source:** the AppImage can be unpacked
  (`--appimage-extract`) and its Qt libraries replaced; the exact Qt source is at
  `https://download.qt.io/archive/qt/6.8/6.8.3/`. Both are explained in
  `QT-NOTICE.md`.

## Rust dependencies (Qt application)

Direct dependencies (all have machine-readable license metadata):

| Crate | Version | License | Notes |
|---|---|---|---|
| cxx | 1.0.202 | MIT OR Apache-2.0 | C++/Rust bridge |
| cxx-qt | 0.10.0 | MIT OR Apache-2.0 | |
| cxx-qt-lib | 0.10.0 | MIT OR Apache-2.0 | |
| cxx-qt-build (build only) | 0.10.0 | MIT OR Apache-2.0 | |
| rusqlite | 0.40.2 | MIT | brings `libsqlite3-sys` |
| toml | 1.1.6 | MIT OR Apache-2.0 | |

All 82 crates in `Cargo.lock` (excluding OmaTree) carry license metadata; none
is missing one, none is GPL/AGPL-only, and none is copyleft-only. Transitive
licenses present: MIT OR Apache-2.0 (61), MIT (8), `MIT/Apache-2.0` (3, the
old spelling), Unlicense OR MIT (3), Apache-2.0 OR MIT (2), Apache-2.0 (2),
Zlib (1), `MIT OR Apache-2.0 OR LGPL-2.1-or-later` (1), and
`(MIT OR Apache-2.0) AND Unicode-3.0` (1). Items worth a note:

- `codespan-reporting` (Apache-2.0 only; two versions): used by the C++
  bridge code generator at build time, not part of the executable.
- `foldhash` (Zlib): linked into the executable (through `hashbrown`, from
  `rusqlite`). The Zlib license asks for no attribution in binaries.
- `r-efi` (MIT OR Apache-2.0 OR LGPL-2.1-or-later): an alternative, the
  permissive options apply; it is not used on Linux.
- `unicode-ident` (`... AND Unicode-3.0`): a compile-time (proc-macro)
  dependency, not in the executable.
- The old `MIT/Apache-2.0` spelling in `vcpkg` (a build helper) is only a
  metadata spelling.

No crate in the dependency set ships a `NOTICE` file.

## SQLite

`rusqlite` is built with the `bundled` feature, so SQLite 3.53.2 (the
amalgamation inside `libsqlite3-sys 0.38.2`) is compiled into the OmaTree
executable. SQLite is in the public domain: its source begins with "The author
disclaims copyright to this source code" and a blessing in place of a legal
notice. No attribution or license text is required by SQLite, and none is
invented here. (`libsqlite3-sys` the Rust wrapper is MIT; the optional
SQLCipher and OpenSSL paths of that crate are not enabled.)

## NOTICE file

**Not required, none created.** OmaTree is Apache-2.0 *or* MIT at the
recipient's option, which does not in itself require a NOTICE, and no
dependency that would be distributed provides one.

## Distribution checklist

- [x] LGPL-3.0 text and Qt acknowledgement included (AppImage: `QT-NOTICE.md`
      and the licence texts; Arch and Debian ship no Qt).
- [x] The Qt libraries stay replaceable in the chosen package format (system Qt
      for Arch and Debian; an unpackable AppImage).
- [x] Source/offer for any Qt libraries that the package itself ships (AppImage:
      the `download.qt.io` link in `QT-NOTICE.md`).
- [x] Include `LICENSE-MIT`, `LICENSE-APACHE` and this file (Debian: in
      `/usr/share/doc/omatree/`, with `copyright`).
- [ ] Re-run the inventory (`cargo metadata`, `readelf -d`) for the release
      commit and the Qt version actually packaged.

## macOS app bundle

`OmaTree.app` (Apple Silicon) is the one package that **contains** Qt and the
libraries Qt needs. It is built with Homebrew's Qt 6.11.2 (the same release as
above) and carries its own notices, written from the finished bundle by
`packaging/macos/licenses.py`:

- `Contents/Resources/licenses/THIRD_PARTY_NOTICES.md`: for Qt and each
  bundled library, the version, the licence as declared by its Homebrew formula,
  home page, source archive and the files in the app, plus how Qt can be
  replaced (it is dynamically linked, under the LGPL-3.0);
- `Contents/Resources/licenses/third-party/<library>/`: the licence files that
  ship with each library;
- `Qt-LGPL-3.0.txt`, `Qt-GPL-3.0.txt`, this file, and OmaTree's own licences.

As of 0.1.1 the bundle contains, besides Qt (qtbase and qtdeclarative): libb2,
brotli, OpenSSL 3 (linked by QtNetwork, though OmaTree makes no network
connection), dbus, double-conversion, FreeType, glib, graphite2, HarfBuzz, ICU 78,
gettext (libintl), md4c, PCRE2, libpng and zstd. The licences range from MIT,
BSD-3-Clause, Unicode-3.0 and Apache-2.0 to the LGPL (Qt, glib, libintl,
graphite2 as one of its options) and dual licences (dbus, FreeType, zstd).
`packaging/macos/verify-app.sh` fails the build if a bundled library is missing
from the notices or has no licence text.

## Windows installer

The Windows installer (`packaging/windows/`, see `docs/PACKAGING.md`) **contains** Qt
and the Microsoft C++ runtime, like the macOS bundle, and its notices are written from
the finished staged tree by `packaging/windows/write-licenses.ps1`, not from this
file's inventory:

- Qt 6.8.3, the official unmodified Windows binaries (MSVC 2022, 64-bit; byte for byte
  what `windeployqt` copies), used under the LGPL-3.0 and dynamically linked: 19 `Qt6*.dll`
  modules, `platforms\qwindows.dll` and 14 QML plug-ins. For each module the notice gives the
  licence Qt's own SPDX SBOM states and lists the third-party code that SBOM bundles in or
  attributes to those modules (zlib, PCRE2, libpng, FreeType, HarfBuzz, md4c,
  double-conversion, tinycbor, libpsl and others).
- The licence texts of that inventory are Qt's own `LICENSES/` files at tag v6.8.3
  (`packaging/windows/licenses/`, 22 texts) and are installed under `licenses\texts\`.
- Five Microsoft C++ runtime DLLs (`MSVCP140`, `MSVCP140_1`, `MSVCP140_2`, `VCRUNTIME140`,
  `VCRUNTIME140_1`, 14.44), copied unmodified from the Visual Studio 2022 Build Tools'
  redistributable folder and installed next to the executable.
- `LICENSE-MIT`, `LICENSE-APACHE` and this file.

`packaging/windows/verify-package.ps1` fails the build if a bundled component is not
in the notice or a listed licence has no text. The notice is an inventory, not legal advice.
