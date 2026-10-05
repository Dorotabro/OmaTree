# Disposable build environment for the Debian/Ubuntu package (Ticket 032).
# Debian 13 (trixie), pinned by digest; its system Qt 6.8 is what the package is
# built against. (Debian 12's Qt 6.4 is too old: OmaTree's QML needs newer.)
# Debian's own Rust is far too old, so the toolchain comes from a pinned,
# checksummed rustup-init.
FROM debian:13@sha256:9cc080028c43b27d2074d63a5f9caf7166d731494965616c1a6d2827a004585c

RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        build-essential debhelper devscripts fakeroot lintian curl ca-certificates \
        pkg-config qt6-base-dev qt6-declarative-dev qt6-declarative-dev-tools \
        libgl1-mesa-dev libvulkan-dev \
    && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/opt/rustup
RUN curl -sSfo /tmp/rustup-init https://static.rust-lang.org/rustup/archive/1.28.2/x86_64-unknown-linux-gnu/rustup-init \
    && echo '20a06e644b0d9bd2fbdbfd52d42540bdde820ea7df86e92e533c073da0cdd43c  /tmp/rustup-init' | sha256sum -c - \
    && chmod +x /tmp/rustup-init \
    && CARGO_HOME=/opt/cargo /tmp/rustup-init -y --profile minimal --default-toolchain 1.99.0 --no-modify-path \
    && rm /tmp/rustup-init
ENV PATH=/opt/rustup/toolchains/1.99.0-x86_64-unknown-linux-gnu/bin:$PATH
