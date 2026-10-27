# Disposable build environment for the egui AppImage.
#
# Build userspace: Ubuntu 20.04 (glibc 2.31), pinned by digest. An AppImage can run on nothing
# older than the userspace it was built in, so the oldest userspace that builds OmaTree sets the
# compatibility floor (the measured result is in docs/PACKAGING.md). There is no Qt here, and no
# library is bundled: the executable links only glibc, libm and libgcc_s, and the windowing,
# GL, font and D-Bus libraries are the host's, opened at run time (AppImage practice: graphics
# libraries must come from the host).
#
# Packaging tools are pinned by exact release and SHA-256 and are never "latest".
FROM ubuntu:20.04@sha256:8feb4d8ca5354def3d8fce243717141ce31e2c428701f6682bd2fafe15388214

RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        build-essential curl ca-certificates pkg-config file desktop-file-utils python3 git \
    && rm -rf /var/lib/apt/lists/*

# Rust: pinned, checksummed rustup-init.
ENV RUSTUP_HOME=/opt/rustup
RUN curl -sSfo /tmp/rustup-init https://static.rust-lang.org/rustup/archive/1.28.2/x86_64-unknown-linux-gnu/rustup-init \
    && echo '20a06e644b0d9bd2fbdbfd52d42540bdde820ea7df86e92e533c073da0cdd43c  /tmp/rustup-init' | sha256sum -c - \
    && chmod +x /tmp/rustup-init \
    && CARGO_HOME=/opt/cargo /tmp/rustup-init -y --profile minimal --default-toolchain 1.99.0 --no-modify-path \
    && rm /tmp/rustup-init
ENV PATH=/opt/rustup/toolchains/1.99.0-x86_64-unknown-linux-gnu/bin:$PATH

# AppImage tooling, exact releases, SHA-256 verified (the same files as the Qt package uses).
#   appimagetool   1.9.1
#   type2-runtime  20251108
RUN set -e; mkdir /opt/tools && cd /opt/tools \
    && curl -sSLo appimagetool.AppImage https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage \
    && curl -sSLo runtime-x86_64 https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64 \
    && echo 'ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0  appimagetool.AppImage' | sha256sum -c - \
    && echo '2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d  runtime-x86_64' | sha256sum -c - \
    && chmod +x appimagetool.AppImage \
    # No FUSE in a container: unpack the tool once and run it from its AppDir.
    && ./appimagetool.AppImage --appimage-extract >/dev/null && mv squashfs-root appimagetool && rm appimagetool.AppImage \
    && ln -s /opt/tools/appimagetool/AppRun /usr/local/bin/appimagetool
