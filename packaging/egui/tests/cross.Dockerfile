# Cross-check image (ticket E12c): the egui Debian 13 build image plus the Windows GNU target and the
# MinGW-w64 C compiler that rusqlite's bundled SQLite needs. Compile checks only; see cross-check.sh.
FROM omatree-egui-build-debian13
RUN apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq --no-install-recommends gcc-mingw-w64-x86-64 \
    && rm -rf /var/lib/apt/lists/* \
    && RUSTUP_HOME=/opt/rustup /opt/cargo/bin/rustup target add x86_64-pc-windows-gnu aarch64-apple-darwin x86_64-pc-windows-msvc
