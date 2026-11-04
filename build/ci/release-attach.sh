#!/usr/bin/env bash
# Add (or replace) assets on an existing GitHub release and refresh their lines in its SHA256SUMS.txt
# (release workflow: the signed DMG, and the APK in attach_to_existing mode).
# Usage: release-attach.sh <tag> <file>...   Env: GH_TOKEN, GH_REPO. Needs sha256sum (Linux runners).
set -euo pipefail
tag=${1:?usage: release-attach.sh <tag> <file>...}
shift
[ "$#" -ge 1 ] || { echo "usage: release-attach.sh <tag> <file>..." >&2; exit 64; }

state=$(gh release view "$tag" --json isDraft --jq 'if .isDraft then "draft" else "published" end') \
  || { echo "::error::no release for $tag; the draft is created by the release workflow's build"; exit 1; }
echo "release $tag ($state)"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
gh release download "$tag" --pattern SHA256SUMS.txt --dir "$work" || : > "$work/SHA256SUMS.txt"
cp "$work/SHA256SUMS.txt" "$work/sums"
for file in "$@"; do
  name=$(basename "$file")
  line=$(cd "$(dirname "$file")" && sha256sum -- "$name")
  { grep -v -F "  $name" "$work/sums" || true; echo "$line"; } > "$work/sums.new"
  mv "$work/sums.new" "$work/sums"
done
LC_ALL=C sort -k2 "$work/sums" > "$work/SHA256SUMS.txt"
cat "$work/SHA256SUMS.txt"
gh release upload "$tag" "$@" "$work/SHA256SUMS.txt" --clobber
