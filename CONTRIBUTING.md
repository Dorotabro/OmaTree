# Contributing

Issues are welcome.

**Bug reports** are most useful with: the OmaTree version, how you installed it
(Arch package, `.deb`, AppImage or source), your distribution and desktop (and
Wayland or X11), and the steps that reproduce the problem. Please do not attach
notebooks that contain private notes.

**Features:** OmaTree is deliberately small (see [`VISION.md`](VISION.md)). Please
open an issue to discuss anything larger than a fix before you write it; many good
ideas will not fit, and that is by design.

**Pull requests:** keep them focused, and before sending one run

    cargo fmt --check
    cargo clippy --all-targets
    cargo test

`cargo test` includes the integration tests, which run the real application
headless. Technology choices and rules are in [`AGENTS.md`](AGENTS.md) and
[`ARCHITECTURE.md`](ARCHITECTURE.md).

Contributions are accepted under the project's licence, `MIT OR Apache-2.0`.
