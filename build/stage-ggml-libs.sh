#!/usr/bin/env bash
# B-49: put llama.cpp's shared libraries and CPU backend modules from an x86_64 Windows / Linux release build
# (llama-cpp-sys-2 `dynamic-backends`) into a stage folder and write the Tauri config overlay that bundles them:
#   cargo build --release -p borealis            # builds llama.cpp (the Tauri build reuses it)
#   bash build/stage-ggml-libs.sh target/release target/ggml-bundle
#   tauri build --config target/ggml-bundle/tauri.conf.json
# Run from anywhere; both folders are relative to the repository root. Layout (src-tauri/src/semantic_backend.rs):
#   Windows: llama.dll, ggml.dll, ggml-base.dll next to borealis.exe; ggml/ggml-cpu-<level>.dll
#   Linux:   ggml/ with libllama.so, libggml.so, libggml-base.so (by SONAME, unversioned: .cargo/config.toml; run path $ORIGIN)
#            and libggml-cpu-<level>.so, in the bundle's resource folder (/usr/lib/Borealis/ggml)
# plus ggml/LICENSE-llama.cpp.txt (MIT; NOTICE section 8). Prints the staged files and their total size.
set -euo pipefail
cd "$(dirname "$0")/.."
profile=${1:?usage: stage-ggml-libs.sh <target/release> <stage folder>}
stage=${2:?usage: stage-ggml-libs.sh <target/release> <stage folder>}
case "$stage" in /* | *..*) echo "the stage folder must be inside the repository: $stage" >&2; exit 1 ;; esac

# The newest llama-cpp-sys-2 build that installed CPU backend modules.
backends=$(ls -td "$profile"/build/llama-cpp-sys-2-*/out/backends 2>/dev/null | head -n 1 || true)
if [ -z "$backends" ] || [ -z "$(ls "$backends" 2>/dev/null)" ]; then
  echo "no llama.cpp CPU backend modules under $profile/build (an x86_64 Windows / Linux release build first)" >&2
  exit 1
fi
out=$(dirname "$backends")
echo "llama.cpp build: $out"

rm -rf "$stage"
mkdir -p "$stage/ggml"
if ls "$out"/bin/*.dll > /dev/null 2>&1; then
  windows=1
  cp "$out"/bin/*.dll "$stage/"
  cp "$backends"/ggml-cpu-*.dll "$stage/ggml/"
else
  windows=0
  command -v patchelf > /dev/null || { echo "patchelf is needed (apt-get install patchelf)" >&2; exit 1; }
  # out/lib/libX.so are the names the build linked (-lX); the binary needs them by SONAME (the same, unversioned).
  for so in "$out"/lib/lib*.so; do
    soname=$(objdump -p "$so" | awk '$1 == "SONAME" { print $2 }')
    cp -L "$so" "$stage/ggml/${soname:-$(basename "$so")}"
  done
  cp "$backends"/libggml-cpu-*.so "$stage/ggml/"
  # Each library finds the others in its own folder.
  for f in "$stage"/ggml/*.so*; do patchelf --set-rpath '$ORIGIN' "$f"; done
fi

# The licence of the bundled libraries: llama.cpp's LICENSE in the llama-cpp-sys-2 source cargo used.
manifest=$(cargo metadata --format-version 1 --locked 2> /dev/null | jq -r '.packages[] | select(.name == "llama-cpp-sys-2") | .manifest_path' | head -n 1)
if command -v cygpath > /dev/null; then manifest=$(cygpath -u "$manifest"); fi
license="$(dirname "$manifest")/llama.cpp/LICENSE"
[ -f "$license" ] || { echo "llama.cpp LICENSE not found next to $manifest" >&2; exit 1; }
cp "$license" "$stage/ggml/LICENSE-llama.cpp.txt"

# The Tauri overlay: resource paths are relative to src-tauri/.
{
  printf '{\n  "bundle": {\n    "resources": {\n'
  if [ "$windows" = 1 ]; then
    for f in "$stage"/*.dll; do
      name=$(basename "$f")
      printf '      "../%s/%s": "%s",\n' "$stage" "$name" "$name"
    done
  fi
  printf '      "../%s/ggml": "ggml"\n    }\n  }\n}\n' "$stage"
} > "$stage/tauri.conf.json"

cat "$stage/tauri.conf.json"
ls -l "$stage" "$stage/ggml"
echo "staged library bytes: $(find "$stage" -type f \( -name '*.dll' -o -name '*.so*' \) -exec cat {} + | wc -c | tr -d ' ')"
