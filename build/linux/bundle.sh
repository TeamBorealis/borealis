#!/usr/bin/env bash
# In-container release bundle (deb, rpm, AppImage): build/linux/run.sh build/linux/bundle.sh
# Under x86_64 emulation on Apple silicon (Docker Desktop + Rosetta) the AppImage tools fail with
# "Exec format error": Rosetta's binfmt rule rejects the AppImage magic bytes at ELF offset 8.
# Zeroing them (what Tauri already does for linuxdeploy itself) and re-bundling fixes it.
set -euo pipefail
cd "$(dirname "$0")/../.."
ui/node_modules/.bin/tauri build --ci "$@" && exit 0
shopt -s nullglob
tools=("$HOME"/.cache/tauri/*.AppImage)
[ ${#tools[@]} -gt 0 ] || exit 1
echo "bundle failed; zeroing AppImage magic in ~/.cache/tauri and retrying the AppImage"
for f in "${tools[@]}"; do printf '\0\0\0' | dd of="$f" bs=1 seek=8 conv=notrunc status=none; done
ui/node_modules/.bin/tauri bundle --bundles appimage --ci "$@"
