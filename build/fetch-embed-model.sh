#!/usr/bin/env bash
# Download the T2 / T3 embedding model (EmbeddingGemma 2 Q8_0) into a folder, from the same pinned sources the app
# uses (crates/borealis-embed/src/models.rs: the Hugging Face commit URL, then the Borealis mirror), and verify its
# compiled SHA-256 and size. An already present file that verifies is kept (CI caches the folder). Never commit the
# model file.
#   fetch-embed-model.sh <folder>      prints the model path on the last line
set -euo pipefail
dir=${1:?usage: fetch-embed-model.sh <folder>}
models=$(dirname "$0")/../crates/borealis-embed/src/models.rs
# The Q8_0 entry of the model table: `pub const Q8_0: ModelFile = ModelFile { ... };`.
entry=$(awk '/^pub const Q8_0: ModelFile = ModelFile \{/,/^\};/' "$models")
field() { printf '%s\n' "$entry" | grep -E "^[[:space:]]*$1:" | head -n 1 | sed -E 's/^[^:]*:[[:space:]]*//; s/,$//; s/^"//; s/"$//'; }
file=$(field file_name)
sha=$(field sha256)
bytes=$(field bytes | tr -d '_')
hf=$(printf '%s\n' "$entry" | grep -oE 'https://huggingface\.co/[^"]+' | head -n 1)
mirror_base=$(grep -E '^pub const MIRROR_RELEASE_BASE' -A 1 "$models" | grep -oE 'https://[^"]+' | head -n 1)
for v in file sha bytes hf mirror_base; do
  if [ -z "${!v}" ]; then echo "could not read $v from $models" >&2; exit 1; fi
done
mkdir -p "$dir"
path="$dir/$file"

sha256_of() {
  if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi
}
size_of() { wc -c <"$1" | tr -d ' '; }
verified() { [ -f "$1" ] && [ "$(size_of "$1")" = "$bytes" ] && [ "$(sha256_of "$1")" = "$sha" ]; }

if verified "$path"; then
  echo "model: cached $file verifies"
else
  rm -f "$path"
  for url in "$hf" "$mirror_base$file"; do
    echo "model: downloading $file from $(printf '%s' "$url" | sed -E 's#^(https://[^/]+).*#\1#')"
    if curl -fsSL --retry 3 --retry-delay 5 -o "$path.part" "$url" && verified "$path.part"; then
      mv "$path.part" "$path"
      break
    fi
    echo "model: that source failed or the file did not verify" >&2
    rm -f "$path.part"
  done
  verified "$path" || { echo "model: no source gave a file with the compiled SHA-256" >&2; exit 1; }
  echo "model: $file verifies (SHA-256 and $bytes bytes)"
fi
echo "$path"
