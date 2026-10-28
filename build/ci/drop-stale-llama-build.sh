#!/usr/bin/env bash
# CI: drop a llama.cpp build (llama-cpp-sys-2) that the Rust cache restored from before a llama.cpp CMake setting
# changed. cmake does not reconfigure an existing build, so the old CMake cache would still win. Stale when a cache
# has the x86 SIMD options on (before the SSE2 baseline, S3.3) or no CMAKE_PROJECT_INCLUDE (before the MSVC hybrid
# runtime, B-56; .cargo/config.toml). Also removes the DLLs llama-cpp-sys-2 hard-linked next to the build's binaries,
# which it never replaces.
#   drop-stale-llama-build.sh <target/debug | target/release>
set -euo pipefail
profile=${1:?usage: drop-stale-llama-build.sh <target/debug | target/release>}

stale=""
for cache in "$profile"/build/llama-cpp-sys-2-*/out/build/CMakeCache.txt; do
  [ -f "$cache" ] || continue
  if grep -q '^GGML_AVX2:BOOL=ON' "$cache"; then stale="$stale $cache(GGML_AVX2)"; fi
  if ! grep -q '^CMAKE_PROJECT_INCLUDE' "$cache"; then stale="$stale $cache(no-CMAKE_PROJECT_INCLUDE)"; fi
done
if [ -z "$stale" ]; then
  echo "no stale llama.cpp build under $profile"
  exit 0
fi
echo "stale llama.cpp build:$stale"

for dll in "$profile"/build/llama-cpp-sys-2-*/out/bin/*.dll; do
  [ -f "$dll" ] || continue
  name=$(basename "$dll")
  rm -fv "$profile/$name" "$profile/deps/$name" "$profile/examples/$name"
done
case "$profile" in
  */release | release) cargo clean --release -p llama-cpp-sys-2 ;;
  *) cargo clean -p llama-cpp-sys-2 ;;
esac
