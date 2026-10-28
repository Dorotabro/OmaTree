# Packaging the egui version (Linux)

This is the Linux packaging of `omatree-egui` (ticket E12b). The Qt application's packaging is in
[`PACKAGING.md`](PACKAGING.md) and is **not** changed by anything here; the two are independent until
the E13 cut-over. Nothing described here has been published, uploaded or installed into a normal system.

Three formats are built from one staging script, `packaging/egui/linux/stage.sh`:

| Format | File | Built in | Binary needs glibc |
|---|---|---|---|
| Arch / Omarchy | `omatree-egui-0.1.2-1-x86_64.pkg.tar.zst` | `makepkg` on the host (Arch, rolling) | ≥ 2.43 (host's 2.44) |
| Debian / Ubuntu | `omatree-egui_0.1.2_amd64.deb` | Debian 13 container (glibc 2.41) | ≥ 2.39 (`libc6 (>= 2.39)`) |
| AppImage | `OmaTree-egui-0.1.2-x86_64.AppImage` | Ubuntu 20.04 container (glibc 2.31) | ≥ 2.30 |

## What is verified, and how

"Executed" means the packaged binary was started and kept running with a window. Evidence is labelled
**native** (the real Omarchy/Hyprland Wayland session on the development machine), **container** (a
disposable Docker container with a software display) or **inspected** (contents and metadata only).

| Check | Arch | Debian | AppImage |
|---|---|---|---|
| Built from a clean snapshot with the locked dependencies | yes | yes | yes |
| Static verification (`packaging/egui/verify-package.sh`, ~70 checks) | passed | passed | passed |
| `desktop-file-validate`, `appstreamcli validate`, MIME XML, icons | passed | passed | passed |
| Installed into an isolated root and removed again | **yes**, `pacman -r` into a private root: 15 files in, `pacman -Qk` clean, removal leaves nothing (hooks do not run in a private root, so the icon/MIME caches were refreshed by hand) | **yes**, `apt install` and `apt purge` in clean `debian:13` and `ubuntu:24.04` containers; no package-owned file remains | not installed (an AppImage has no installation); extracted and mounted |
| Executed, native Wayland | **yes** (from the private root, through the desktop entry) | no | yes (from the mounted image and from the extracted AppDir) |
| Executed, container | no | **yes**: Debian 13 and Ubuntu 24.04, X11 (Xvfb) and Wayland (headless Weston), Mesa software rendering | **yes**: Ubuntu 20.04 and Debian 12, X11 (Xvfb) |
| `lintian` | n/a | 0 errors; 1 warning (`no-manual-page`) and 1 info (`hardening-no-fortify-functions`, Rust) | n/a |
| `namcap` | **not run: not installed on the development machine** | n/a | n/a |

What is **not** verified: native X11 (only Xwayland and Xvfb were available), hi-DPI, a screen reader,
Fedora/openSUSE or any other distribution, and Wayland inside the older-distribution containers.

### The installed application (Arch package, native Wayland)

From the isolated install, launched exactly as a file manager does (`gio launch` on the installed
desktop file with a path), with isolated `HOME`, `XDG_CONFIG_HOME`, `XDG_STATE_HOME`, `XDG_DATA_HOME`
and `XDG_DATA_DIRS`, using disposable notebooks:

- launch with a notebook whose name has spaces and non-ASCII (`Poznámky č. 1 ✓.omatree`): opens, title
  and recent list correct, file bytes unchanged;
- launch with no notebook: empty window;
- search (`Ctrl+F`), Recovery (`Ctrl+Shift+R`), Compact Notebook (File menu, by mouse), File menu itself;
- native **Save As** through the xdg-desktop-portal GTK dialog (a name with `✓`), then **Cancel** with
  Escape (the app is unaffected);
- **pasting a clipboard image** (`wl-copy`), saving, closing, reopening: the image is still there and the
  notebook holds the attachment;
- **PDF export** through the native dialog: a valid one-page PDF titled with the note;
- with no D-Bus session bus (no portal): `Ctrl+O` shows "Waiting for the system file dialog…" with a
  **Type a path instead…** button, and the typed path (spaces, Unicode) opens;
- themes: default dark, a light fixture and the user's Kanagawa palette (a read-only copy), applied live;
- window class on the native Wayland window and under Xwayland is `io.github.Dorotabro.OmaTreeEgui`;
- launcher icon and the `application/x-omatree` file icon both resolve through GTK's icon theme to the
  installed icon (see "Icons").

Not exercised by hand on the installed package: the Markdown/ZIP/folder exports (covered by the
automated tests), Open by native dialog (Save As and Export were), `Ctrl+Z` after save and similar
editor behaviour (covered by `cargo test`, unchanged by packaging).

## Identity

| What | Value |
|---|---|
| Product name | OmaTree (desktop entry `Name=OmaTree (egui)` while both versions exist) |
| Application ID | `io.github.Dorotabro.OmaTreeEgui` |
| Binary | `/usr/bin/omatree-egui` |
| Package names | `omatree-egui` (Arch, Debian); `OmaTree-egui-VERSION-x86_64.AppImage` |
| MIME type | `application/x-omatree` (unchanged), glob `*.omatree`, no magic |

The ID is one string, `omatree_egui::APP_ID`, and is the window's Wayland `app_id` and X11 class, the
desktop file's name and `StartupWMClass`, the icon name, the AppStream ID and the MIME icon. A test
(`packaging_tests.rs`) and `packaging/egui/tests/verify-sources.sh` keep them equal.

**Checked, not assumed:** on the native Wayland session and under Xwayland, `hyprctl clients` reports the
window's class as exactly `io.github.Dorotabro.OmaTreeEgui`. The Qt application is
`io.github.Dorotabro.OmaTree`, so the two do not collide in any file path, ID, package or window class.
The only shared name is the MIME type (and the `*.omatree` glob): each package ships its own XML file
under its own name, the definitions are identical, and `update-mime-database` merges them; whichever
desktop entry the user chooses is the default.

**At E13**, the cut-over renames, in one change: `APP_ID` to the canonical
`io.github.Dorotabro.OmaTree`; the desktop, metainfo, MIME and icon files to that name; the binary and
package to `omatree` (the egui package then `Replaces`/`Conflicts` with the Qt one, which is removed);
`Name=` to `OmaTree`. `verify-sources.sh` is the list of places that must change together.

### Window icon

`window_icon()` embeds the 256 px PNG and gives it to the window. Wayland ignores window icons: the
compositor and bars take the icon from the desktop file that matches the `app_id`, which is why the
desktop file name and `StartupWMClass` matter. Under X11 the embedded icon is used.

## Desktop entry and file association

`Exec=omatree-egui %f`: one file, passed to the program as a single argv element by the launcher, never
through a shell (checked with `gio launch` and a file named `$(touch PWNED); \`touch PWNED2\` & x.omatree`:
one argument, nothing ran). `Terminal=false`, `Categories=Office;`, `MimeType=application/x-omatree;`,
`StartupWMClass` set. `StartupNotify` is not set because the application does not implement the
startup-notification protocol.

`omatree-egui` accepts at most one path, refuses a path that does not exist (it does not create it) and
refuses an option-like argument, with a message in the window. There is no single-instance IPC: opening
a second notebook from the file manager starts a second window (an E13-or-later feature).

The MIME XML matches only `*.omatree`. There is no magic matching, so ordinary SQLite files are not
claimed (verified in an isolated MIME database with GLib: a `.db` and a `.sqlite` file containing a SQLite header are
not identified as `application/x-omatree` (the `.db` one is `application/octet-stream`)).

## Installing and removing

Nothing below was run against the real system by this ticket.

```bash
# Arch / Omarchy
sudo pacman -U omatree-egui-0.1.2-1-x86_64.pkg.tar.zst
sudo pacman -R omatree-egui

# Debian 13+ / Ubuntu 24.04+
sudo apt install ./omatree-egui_0.1.2_amd64.deb
sudo apt remove omatree-egui          # or purge; there are no configuration files

# Any x86_64 Linux with glibc 2.30+ and a Wayland or X11 desktop
chmod +x OmaTree-egui-0.1.2-x86_64.AppImage
./OmaTree-egui-0.1.2-x86_64.AppImage [notebook.omatree]
```

The Qt package (`omatree`) can stay installed. Both then appear in the launcher ("OmaTree" and
"OmaTree (egui)"). The packages ship no `mimeapps.list` and the Debian package has no maintainer script (checked), so
installing cannot write the user's preferred application for `.omatree` files; with two registered applications
the desktop decides which it offers or defaults to (not tried on a real installation). Removing it removes only its own files. Notebooks are ordinary files and are
never touched; the recent-notebooks list stays in the user's configuration (below).

The AppImage does not install itself; integrate it with your desktop's AppImage tool for a menu entry and file
association. It contains the same desktop entry, icons and MIME data.

## Where things are stored

- Recent notebooks: `$XDG_CONFIG_HOME/omatree/recent.txt` (else `~/.config/omatree/recent.txt`), one path
  per line, newest first. Never inside a notebook.
- Nothing else is written outside the notebook itself (and its export targets).

## File dialogs and the desktop portal

File dialogs go through the XDG desktop portal over D-Bus (`libdbus-1`, opened when the first dialog is
requested; the toolkit's own dialogs are not used and no GTK or Qt is linked). The packages recommend
`xdg-desktop-portal` and a backend; with none, or without a session bus, the dialog shows "Waiting for the
system file dialog…" with **Type a path instead…** (verified for Open; Save As and Export use the same mechanism; the File menu also has **Open by path…**
and **Save As by path…**) and the typed path works the same. `OMATREE_FILE_DIALOG=typed`
forces typed dialogs. Known limits, carried to E12c: the native dialogs are not parented to the OmaTree
window, and a portal that fails looks like a cancel in some cases.

## Accessibility

The egui binary is built with AccessKit. On Linux its AT-SPI tree exists only when the session asks for
accessibility (the `org.a11y.Status IsEnabled` flag, which a screen reader such as Orca sets); it is
off by default on most desktops. The packages recommend `at-spi2-core`. Operation with a real screen reader
is **unverified** (see the E12a record).

## Runtime libraries

The executable's dynamic section lists only `libc`, `libm`, `libgcc_s` and the loader (and, on older glibc,
`libdl`, `libpthread`). Everything graphical is opened at run time, so a missing library is an error at
start-up, not at install. Measured with `LD_DEBUG=libs` on native Wayland, on Xwayland, and in the
containers, the libraries actually loaded are:

| Purpose | Libraries | Arch package | Debian package |
|---|---|---|---|
| Wayland | `libwayland-client`, `libwayland-egl` | `wayland` | `libwayland-client0`, `libwayland-egl1` |
| Keyboard | `libxkbcommon`, `libxkbcommon-x11` | `libxkbcommon`, `libxkbcommon-x11` | `libxkbcommon0`, `libxkbcommon-x11-0` |
| GL / EGL | `libEGL`, `libGL`/`libGLX` (glvnd) and a driver | `libglvnd`; `mesa` optional | `libegl1`, `libgl1`; `libgl1-mesa-dri` recommended |
| X11 | `libX11`, `libxcb`, `libXcursor`, `libXi` | `libx11`, `libxcb`, `libxcursor`, `libxi` | `libx11-6`, `libxcb1`, `libxcursor1`, `libxi6` |
| Fonts | `libfontconfig` (system fonts; egui's bundled fonts remain) | `fontconfig` | `libfontconfig1` |
| Portal | `libdbus-1` (on first dialog) | `dbus` | `libdbus-1-3` |

SQLite is compiled in (3.53.2, `libsqlite3-sys` bundled). No Qt, CXX-Qt or QML library or string is in the
executable (`verify_binary` checks the dynamic section and the symbols), and `cargo tree` shows no Qt crate.
Clipboard text and images use the Wayland data-control/clipboard protocols or X11 selections through crates
that are compiled in; no extra library is loaded for them. The measured set is what the Debian `Depends`
lists; the Arch list is the same set under Arch names.

With a library absent the failure is a clean exit 1 and a message, not a hang or crash (tested by
making `libwayland-client`, `libwayland-egl` and `libxkbcommon` unreadable under `bwrap`, and in the
Ubuntu 20.04 and Debian 12 containers with the X11 libraries not installed). The message names the
toolkit's internals (`WinitEventLoop(... NoWaylandLib)`) rather than a plain-language instruction, and it
embeds the *build* path of the dependency source, which the packages remap to `/build/cargo/...`
(no `/home/...` path is in any packaged binary). Plain-language start-up errors are an E12c item.

**Glibc.** The ABI floor is set by the build machine. Built on the host (glibc 2.44) the binary needs 2.43;
built in Debian 13, 2.39; built in Ubuntu 20.04, 2.30. So the `.deb` does not install on Debian 12 or Ubuntu
22.04 (apt refuses on `libc6 (>= 2.39)`), and the **AppImage is the portable format**: it was run on Ubuntu
20.04 and Debian 12. Do not claim that portability for other distributions: it was not tried, and the
AppImage deliberately bundles **no** library (windowing, GL, font and D-Bus libraries must be the host's,
as AppImage practice requires for graphics), so a host without them cannot run it.

Building the AppImage on a rolling distribution would raise that floor to the rolling glibc, which is why it
is built in the Ubuntu 20.04 container (pinned by digest) and not on the host.

## Third-party licences

`THIRD_PARTY.md` has an egui section with a table generated from the resolved dependency graph (300 crates,
direct ones in bold) and the licensing decisions; `packaging/egui/third-party.py` generates it and
`verify-sources.sh` fails if it is stale. Every package ships `THIRD_PARTY.md` and
`third-party-licenses.txt` (every crate's licence and notice files from its own source, each distinct text
once). The compiled fonts' licences (SIL OFL-1.1, Ubuntu Font Licence) are in it.

Two things need the maintainer, and are flagged in `THIRD_PARTY.md`:

1. the **editor crate** (`crates/editor`, since P2 part of this repository) is declared `MIT OR Apache-2.0` like
   the rest of OmaTree on the maintainer's stated intent; the basis is recorded there, and the maintainer
   still has to confirm the copyright line and the status of AI-assisted code. Its Liberation fonts (SIL OFL-1.1)
   and their notice are in the bundle;
2. sixteen crates (egui, eframe and their family, accesskit and its family, a few others) declare a licence
   but do not ship the licence file inside the published crate. E12c copied the upstream licence texts
   (`packaging/egui/licences/`, with `SOURCES.md` giving repository, tag or branch) and the bundle includes them
   marked as upstream copies. That supplies the notice text; it does not make their copyright lines
   crate-authoritative, and some of the sources are branch references (krilla, profiling, gl-rs), not tags.

## Building

Prerequisites: Rust 1.99.0 (the packaging uses a pinned rustup in the containers), `git`, `python3`; for
Arch `makepkg`; for Debian and AppImage Docker or Podman. The build never uses `target/release/omatree`
(the Qt binary): every script builds `-p omatree-egui --locked`.

```bash
packaging/egui/arch/build-package.sh        # dist/omatree-egui-0.1.2-1-x86_64.pkg.tar.zst
packaging/egui/debian/build-package.sh      # dist/omatree-egui_0.1.2_amd64.deb  (+ lintian.txt)
packaging/egui/appimage/build-package.sh    # dist/OmaTree-egui-0.1.2-x86_64.AppImage
```

Each snapshots the working tree (tracked and new, non-ignored files), builds from the snapshot with the
committed `Cargo.lock`, and writes to the ignored `dist/`. A release package is to be built from the tagged
archive instead (see the top of `PKGBUILD.in`); `packaging/release-build.sh` is the Qt flow and was not
changed.

**The editor is part of the source tree** (P2). It used to be a Git dependency on the private repository
`Dorotabro/omatree-egui-editor-spike`, which needed `seed-cargo-git.sh` to copy a cached checkout into the
containers; that script and the requirement are gone. Every build now needs only the source snapshot and the
public crates on crates.io (`--locked`). Use a fresh `OMATREE_CACHE` directory to prove that no earlier cache
supplies anything. Earlier sections of this document and the E12b record describe the private-repository
arrangement and are historical.

### Verifying

```bash
packaging/egui/tests/verify-sources.sh                     # source-level checks, no root, no display
packaging/egui/verify-package.sh dist/omatree-egui-0.1.2-1-x86_64.pkg.tar.zst
packaging/egui/verify-package.sh dist/omatree-egui_0.1.2_amd64.deb
packaging/egui/verify-package.sh dist/OmaTree-egui-0.1.2-x86_64.AppImage
packaging/egui/debian/test-install.sh dist/omatree-egui_0.1.2_amd64.deb debian:13   # also ubuntu:24.04
packaging/egui/appimage/test-container.sh dist/OmaTree-egui-0.1.2-x86_64.AppImage ubuntu:20.04   # also debian:12
cargo test -p omatree-egui packaging_tests
```

The package verifiers need `bsdtar`, `readelf`, `objdump`, `xmllint`; they use `desktop-file-validate`,
`appstreamcli`, `update-mime-database`, `identify`, `rsvg-convert` when present and print `skip` when not.
The launch checks need a display and either `cargo` (to write a fixture notebook) or `OMATREE_FIXTURE=file`;
without them they print `skip`, never `ok`. None needs root.

`verify-sources.sh` covers: desktop entry validity and exact `Exec`; MIME XML and its isolated
registration with a Unicode file name; a hostile file name launched through `gio`; one application
identity across all files; the binary built is `omatree-egui`; icons; the installed layout and its
removal; version consistency (Qt crate, egui crate, core, AppStream, Debian changelog); that the editor is the workspace crate `crates/editor` with
no Git source in `Cargo.lock` and no sibling-checkout reference; `--locked` everywhere; no Qt crate in the dependency tree; the
licence table is current; and that the user's own MIME, `mimeapps.list`, desktop-entry and OmaTree
configuration are byte-for-byte unchanged afterwards.

## Reproducibility

Recorded for the development machine: Rust 1.99.0 (b940084d7 2026-09-28), cargo 1.99.0 (5f94df478 2026-08-27),
makepkg 7.1.0, Docker 29.9.2, bsdtar 3.8.10, AppStream 1.2.1, desktop-file-validate 0.28, glibc 2.44 (host).
Containers: Debian 13 (dpkg-dev 1.22.22, debhelper 13.24.2, gcc 14.2.0, glibc 2.41) and Ubuntu 20.04
(glibc 2.31), both pinned by digest in the Dockerfiles with a SHA-256-checked `rustup-init`; the AppImage tools
(appimagetool 1.9.1, type2-runtime 20251108) are pinned by SHA-256. Build flags: `--release --locked`, with
`--remap-path-prefix` for the source tree, the Cargo home and `$HOME`.

**Result.** Each format was built twice from the same source in equivalent clean environments (a fresh
`makepkg --cleanbuild` directory; a fresh container each time). The `omatree-egui` executable was
**bit-identical** between the two builds for all three formats (SHA-256 `8da31cd8…` Arch, `fdddd500…`
Debian, `2252186f…` AppImage; the three differ from each other because the build environments differ).
The other installed files were identical too, except `README.md`, which I edited between the two runs. The
package *containers* are not bit-reproducible: `.PKGINFO`/`.BUILDINFO` carry a build date and the `.deb` and
AppImage embed timestamps, so the package files' own hashes differ run to run; only the installed contents were
compared. This is "repeatable, with identical binaries on one machine and one toolchain", not a claim of
reproducibility across machines or toolchains, which was not tried. The final packages were then rebuilt after
a `rustfmt`-only reformatting of `main.rs` and the test files (the line numbers in `main.rs` changed, so the
binaries differ from the pair above; the final ones are `ba36fda9…` Arch and `f0a93840…` Debian) and
re-verified; they were built once.

## Known limitations and E12c handoff

- **Private editor repository**: resolved by P2 (the editor is in the workspace).
- **Licence of the editor crate** (above): maintainer confirmation. The sixteen crates' notices are supplied (E12c).
- **No `namcap` run** (not installed); the Arch package is otherwise verified. No man page (lintian warning).
- **Debian 12 / Ubuntu 22.04**: the `.deb` is refused (glibc ≥ 2.39 needed). Build the `.deb` in an older
  container if those are wanted; the AppImage covers them.
- **Not tried**: a screen reader, a real input method, a physical Hi-DPI monitor, Fedora/openSUSE/other glibc distributions, AppImage on
  Wayland, `appimaged`/AppImageLauncher integration, update information in the AppImage.
- **No single-instance IPC**: opening a second notebook from the file manager starts a second process.
- **Start-up errors** (E12c): no display, no Wayland library and no X11 library each print a plain sentence, then the
  toolkit's full text under `Details:`, exit status 1. Other failures (for example GL) still show only the toolkit's text.
- **Native dialogs are parented** (E12c: wire-level `parent_window` `wayland:<handle>` / `x11:<xid>` seen on the portal
  call). A missing portal is still shown as a cancel (rfd returns `None` for both). **Recovery timestamps** are local time
  with the offset (`+01:00`); stored values stay UTC.
- **Windows and macOS** packaging is not implemented; see `docs/EGUI_MIGRATION_PLAN.md`, "E12c record", for the checklists.
- **Release signing/checksums**: no `SHA256SUMS` for the egui packages beyond the local
  `dist/SHA256SUMS-egui-arch`; the Qt release flow was deliberately not extended.
