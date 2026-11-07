#!/usr/bin/env bash
# CI (B-183): compile-check the app crate for a phone target, as the release build compiles it, without building it.
# Normal CI builds desktop only, so code under cfg(not(desktop)) / the phone target tables is otherwise first compiled
# by the release workflow (1.3.0's first release run failed on a phone-only import of a desktop-only item).
#
#   mobile-check.sh android     aarch64-linux-android (release.yml: tauri android build --apk --target aarch64)
#   mobile-check.sh ios         aarch64-apple-ios     (release.yml: tauri ios build --target aarch64); macOS only
#
# Same crate, features and profile the Tauri CLI uses for those builds: the lib target of `borealis` with the default
# features plus `tauri/custom-protocol` (the CLI adds it to every non-dev build), in the release profile.
# ui/dist must exist (the Tauri context embeds it at compile time); its contents do not matter to a check.
# Android needs the NDK: NDK_HOME (or ANDROID_NDK_HOME), the version release.yml installs. The C/C++ code the build
# scripts compile (llama.cpp, aws-lc) uses the NDK's clang for API level 24, the app's minSdk, as the Tauri CLI sets.
set -euo pipefail

platform=${1:?usage: mobile-check.sh <android|ios>}
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
readonly ANDROID_MIN_SDK=24

check() {
  local target=$1
  [ -d "$root/ui/dist" ] || { echo "::error::ui/dist is missing (build the UI or create a placeholder)" >&2; exit 1; }
  echo "cargo check -p borealis --lib --release --target $target --features tauri/custom-protocol (-D warnings)"
  # Warnings fail the check, as clippy -D warnings does on desktop: phone-only dead code or imports show up here.
  # (With --target, RUSTFLAGS reaches only the target's crates; dependencies' lints stay capped.)
  RUSTFLAGS="${RUSTFLAGS:-} -D warnings" cargo check --manifest-path "$root/Cargo.toml" -p borealis --lib --release --target "$target" \
    --features tauri/custom-protocol
}

android_env() {
  local ndk=${NDK_HOME:-${ANDROID_NDK_HOME:-}}
  [ -n "$ndk" ] && [ -d "$ndk" ] || { echo "::error::set NDK_HOME to the Android NDK" >&2; exit 1; }
  local prebuilt
  prebuilt=$(find "$ndk/toolchains/llvm/prebuilt" -mindepth 1 -maxdepth 1 -type d | head -n 1)
  [ -n "$prebuilt" ] || { echo "::error::no LLVM toolchain under $ndk/toolchains/llvm/prebuilt" >&2; exit 1; }
  local bin=$prebuilt/bin
  export NDK_HOME=$ndk ANDROID_NDK_HOME=$ndk
  # llama.cpp (llama-cpp-sys-2) reads ANDROID_NDK, not NDK_HOME (release.yml sets all three).
  export ANDROID_NDK=$ndk
  export CC_aarch64_linux_android=$bin/aarch64-linux-android$ANDROID_MIN_SDK-clang
  export CXX_aarch64_linux_android=$bin/aarch64-linux-android$ANDROID_MIN_SDK-clang++
  export AR_aarch64_linux_android=$bin/llvm-ar
  export RANLIB_aarch64_linux_android=$bin/llvm-ranlib
  export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=$CC_aarch64_linux_android
}

case "$platform" in
  android)
    android_env
    check aarch64-linux-android
    ;;
  ios)
    [ "$(uname -s)" = "Darwin" ] || { echo "::error::the iOS check needs macOS (Xcode's iPhoneOS SDK)" >&2; exit 1; }
    check aarch64-apple-ios
    ;;
  *)
    echo "usage: mobile-check.sh <android|ios>" >&2
    exit 2
    ;;
esac
