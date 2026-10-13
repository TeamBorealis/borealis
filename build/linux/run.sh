#!/usr/bin/env bash
# Run a command inside the Borealis Linux build container with the repo bind-mounted at /src.
# Build caches live in named volumes so the macOS target/ and ui/node_modules are never touched.
#   PLATFORM=linux/arm64 (default) or linux/amd64 (emulated, slow).
#   build/linux/run.sh 'cargo test --workspace'
set -euo pipefail
PLATFORM="${PLATFORM:-linux/arm64}"
TAG="${PLATFORM##*/}"
APP_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
IMAGE="borealis-linux-build:${TAG}"
docker image inspect "$IMAGE" >/dev/null 2>&1 \
  || docker build --platform "$PLATFORM" -t "$IMAGE" "$APP_DIR/build/linux"
exec docker run --rm ${DOCKER_RUN_ARGS:-} --platform "$PLATFORM" \
  -v "$APP_DIR":/src \
  -v "borealis-linux-target-${TAG}":/target \
  -v "borealis-linux-node-modules-${TAG}":/src/ui/node_modules \
  -v borealis-linux-cargo-registry:/opt/cargo/registry \
  -v borealis-linux-npm-cache:/home/builder/.npm \
  -w /src "$IMAGE" bash -c "$*"
