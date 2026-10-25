# Borealis Linux build + smoke image (Ubuntu 24.04, Tauri 2 / webkit2gtk-4.1).
# Usage: see app/build/linux/run.sh and docs/build.md "Linux".
FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive \
    RUSTUP_HOME=/opt/rustup \
    CARGO_HOME=/opt/cargo \
    PATH=/opt/cargo/bin:/usr/local/bin:$PATH

# Tauri 2 build deps + bundler tools (deb/rpm/AppImage) + headless smoke tools. cmake + libclang build llama.cpp
# and its bindgen bindings (semantic search backend, B-12).
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential pkg-config curl wget file ca-certificates git xz-utils cmake libclang-dev \
      libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev \
      libssl-dev libxdo-dev patchelf rpm \
      xvfb xauth dbus-x11 x11-apps xdotool imagemagick \
      fonts-dejavu-core fontconfig \
    && rm -rf /var/lib/apt/lists/*

# Node 22 LTS (matches CI).
ARG NODE_VERSION=22.20.0
RUN ARCH=$(dpkg --print-architecture | sed 's/amd64/x64/') \
    && curl -fsSL https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-${ARCH}.tar.xz \
       | tar -xJ -C /usr/local --strip-components=1 \
    && node --version

# Rust stable (rust-toolchain.toml: channel = stable, rustfmt + clippy).
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable \
       --component rustfmt,clippy \
    && chmod -R a+w /opt/rustup /opt/cargo && rustc --version

# Docker Desktop VMs are small (8 GB): fewer parallel jobs and no debuginfo keep the linker from
# being OOM-killed. Override with `DOCKER_RUN_ARGS="-e CARGO_BUILD_JOBS=8"`.
ENV CARGO_TARGET_DIR=/target \
    CARGO_BUILD_JOBS=4 \
    CARGO_PROFILE_DEV_DEBUG=0 \
    CARGO_PROFILE_TEST_DEBUG=0 \
    APPIMAGE_EXTRACT_AND_RUN=1
# Build and test as an unprivileged user: root bypasses file permissions, which breaks the
# read-only-folder tests (and real apps never run as root).
RUN useradd -m -s /bin/bash builder \
    && mkdir -p /target /home/builder/.npm /src/ui/node_modules \
    && chown -R builder:builder /target /home/builder /src /opt/cargo /opt/rustup
USER builder
ENV USER=builder HOME=/home/builder
WORKDIR /src
