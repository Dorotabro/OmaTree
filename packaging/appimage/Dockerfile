# Disposable build environment for the AppImage (Ticket 032).
#
# Build userspace: Ubuntu 20.04 (glibc 2.31), pinned by digest. An AppImage can
# run on nothing older than the userspace it was built in, so the oldest
# userspace that builds OmaTree and Qt reliably sets the compatibility floor.
# (The measured result is in docs/PACKAGING.md.) Ubuntu 20.04's own Qt is far
# too old for OmaTree (its QML needs a Qt newer than 6.4), so the Qt that is bundled is the
# official Qt 6.8.3 binary release (LGPL). A first stage downloads it with a
# pinned aqtinstall (which needs a newer Python than 20.04 has); only the
# resulting Qt directory is copied into the build image.
#
# Packaging tools are pinned by exact release and SHA-256 and are never "latest".
FROM ubuntu:22.04@sha256:5ec03bb3441e8b0bf3b4f9cd4629a1ae763010dc3035bb8da3ae6cf026486401 AS qt
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends python3-pip \
    && rm -rf /var/lib/apt/lists/*
# Only what OmaTree needs: base, declarative (Qt Quick and its QML modules), the
# Wayland client plugins, and the ICU libraries that this Qt build links.
RUN pip install --no-cache-dir aqtinstall==3.3.0 \
    && aqt install-qt linux desktop 6.8.3 linux_gcc_64 --archives icu qtbase qtdeclarative qtwayland -O /opt/qt

FROM ubuntu:20.04@sha256:8feb4d8ca5354def3d8fce243717141ce31e2c428701f6682bd2fafe15388214

RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        build-essential curl ca-certificates pkg-config file patchelf xz-utils desktop-file-utils \
        libgl1-mesa-dev libvulkan-dev libxkbcommon-dev \
        libfontconfig1 libfreetype6 libegl1 libglib2.0-0 libdbus-1-3 \
        libxkbcommon-x11-0 libx11-xcb1 libxcb-glx0 libxcb-cursor0 libxcb-icccm4 libxcb-image0 \
        libxcb-keysyms1 libxcb-randr0 libxcb-render-util0 libxcb-shape0 libxcb-sync1 \
        libxcb-xfixes0 libxcb-xinerama0 libxcb-xkb1 libxcb-util1 \
        libwayland-client0 libwayland-cursor0 libwayland-egl1 \
    && rm -rf /var/lib/apt/lists/*

# Rust: pinned, checksummed rustup-init.
ENV RUSTUP_HOME=/opt/rustup
RUN curl -sSfo /tmp/rustup-init https://static.rust-lang.org/rustup/archive/1.28.2/x86_64-unknown-linux-gnu/rustup-init \
    && echo '20a06e644b0d9bd2fbdbfd52d42540bdde820ea7df86e92e533c073da0cdd43c  /tmp/rustup-init' | sha256sum -c - \
    && chmod +x /tmp/rustup-init \
    && CARGO_HOME=/opt/cargo /tmp/rustup-init -y --profile minimal --default-toolchain 1.99.0 --no-modify-path \
    && rm /tmp/rustup-init
ENV PATH=/opt/rustup/toolchains/1.99.0-x86_64-unknown-linux-gnu/bin:$PATH

COPY --from=qt /opt/qt /opt/qt
ENV QT_ROOT=/opt/qt/6.8.3/gcc_64

# The ICU licence (ICU 73.2 is part of the official Qt binaries and is bundled).
RUN mkdir -p /opt/licenses && curl -sSLo /opt/licenses/ICU-73.2-LICENSE https://raw.githubusercontent.com/unicode-org/icu/release-73-2/icu4c/LICENSE \
    && echo 'f3005e195ff74d8812cc1f182a1c446fab678d70a10e3dada497585befee5416  /opt/licenses/ICU-73.2-LICENSE' | sha256sum -c -

# AppImage tooling, exact releases, SHA-256 verified.
#   linuxdeploy             1-alpha-20250213-2
#   linuxdeploy-plugin-qt   1-alpha-20250213-1
#   appimagetool            1.9.1
#   type2-runtime           20251108
# (appimagetool and the runtime match the digests GitHub publishes for the
# release assets; linuxdeploy and its Qt plugin publish none, so their hashes
# are the ones of the files that were reviewed and tested.)
RUN set -e; mkdir /opt/tools && cd /opt/tools \
    && curl -sSLo linuxdeploy.AppImage https://github.com/linuxdeploy/linuxdeploy/releases/download/1-alpha-20250213-2/linuxdeploy-x86_64.AppImage \
    && curl -sSLo linuxdeploy-plugin-qt.AppImage https://github.com/linuxdeploy/linuxdeploy-plugin-qt/releases/download/1-alpha-20250213-1/linuxdeploy-plugin-qt-x86_64.AppImage \
    && curl -sSLo appimagetool.AppImage https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage \
    && curl -sSLo runtime-x86_64 https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64 \
    && echo '4648f278ab3ef31f819e67c30d50f462640e5365a77637d7e6f2ad9fd0b4522a  linuxdeploy.AppImage' | sha256sum -c - \
    && echo '15106be885c1c48a021198e7e1e9a48ce9d02a86dd0a1848f00bdbf3c1c92724  linuxdeploy-plugin-qt.AppImage' | sha256sum -c - \
    && echo 'ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0  appimagetool.AppImage' | sha256sum -c - \
    && echo '2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d  runtime-x86_64' | sha256sum -c - \
    && chmod +x *.AppImage \
    # No FUSE in a container: unpack each tool once and run it from its AppDir.
    && for t in linuxdeploy linuxdeploy-plugin-qt appimagetool; do \
         ./$t.AppImage --appimage-extract >/dev/null && mv squashfs-root $t && rm $t.AppImage; done \
    && ln -s /opt/tools/linuxdeploy/AppRun /usr/local/bin/linuxdeploy \
    && ln -s /opt/tools/linuxdeploy-plugin-qt/AppRun /usr/local/bin/linuxdeploy-plugin-qt \
    && ln -s /opt/tools/appimagetool/AppRun /usr/local/bin/appimagetool
