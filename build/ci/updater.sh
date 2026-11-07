#!/usr/bin/env bash
# In-app updates in the release workflow (#25, decision UP2).
#
#   updater.sh enabled
#       Prints "true" when updater artifacts can be made: the secret TAURI_SIGNING_PRIVATE_KEY is set AND
#       plugins.updater.pubkey in src-tauri/tauri.conf.json is a real minisign public key (not the placeholder).
#       Otherwise prints "false" and says why: every updater step is then skipped and the release is built as before.
#   updater.sh mac-archive <stapled Borealis.app> <out .app.tar.gz> <version>
#       Tar the stapled app (Tauri's own tarball is made before notarization, so it has no ticket) and sign the tarball
#       for <version>; writes <out>.sig. Env: TAURI_SIGNING_PRIVATE_KEY, TAURI_SIGNING_PRIVATE_KEY_PASSWORD.
#   updater.sh feed <tag> <version>
#       Build latest.json from the .sig files on the release <tag> and upload it there (refreshing SHA256SUMS.txt). Run
#       again whenever a signature joins the draft (the DMG job finishes later). Env: GH_TOKEN, GH_REPO.
#
# The private key and its password are never printed. The updater endpoint is
# https://github.com/TeamBorealis/borealis/releases/latest/download/latest.json, which resolves once the draft is
# published (Thursday 20:00, RL3).
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)

cmd_enabled() {
  local pubkey decoded
  pubkey=$(jq -r '.plugins.updater.pubkey // ""' "$root/src-tauri/tauri.conf.json")
  decoded=$(printf '%s' "$pubkey" | base64 --decode 2>/dev/null || true)
  if [ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
    echo "::notice::in-app updates: no TAURI_SIGNING_PRIVATE_KEY in the release environment; updater artifacts skipped" >&2
    echo false
  elif ! grep -q '^untrusted comment: minisign public key' <<< "$decoded"; then
    echo "::warning::in-app updates: TAURI_SIGNING_PRIVATE_KEY is set but plugins.updater.pubkey in tauri.conf.json is still the placeholder; updater artifacts skipped" >&2
    echo false
  else
    echo true
  fi
}

sign() {
  local file=$1 version=$2
  : "${TAURI_SIGNING_PRIVATE_KEY:?TAURI_SIGNING_PRIVATE_KEY is empty}"
  # The CLI reads the key and password from the environment; its output is the public signature only.
  node "$root/ui/node_modules/@tauri-apps/cli/tauri.js" signer sign --app-version "$version" "$file" > /dev/null
  [ -s "$file.sig" ] || { echo "::error::signing $file produced no $file.sig" >&2; exit 1; }
}

cmd_mac_archive() {
  local app=${1:?usage: mac-archive <Borealis.app> <out.app.tar.gz> <version>}
  local out=${2:?usage: mac-archive <Borealis.app> <out.app.tar.gz> <version>}
  local version=${3:?usage: mac-archive <Borealis.app> <out.app.tar.gz> <version>}
  xcrun stapler validate "$app" > /dev/null || { echo "::error::$app is not stapled" >&2; exit 1; }
  mkdir -p "$(dirname "$out")"
  rm -f "$out" "$out.sig"
  # The updater skips the archive's first path component and expects <Name>.app at its root. No AppleDouble
  # (._*) entries: the ticket and signature are files inside the bundle.
  COPYFILE_DISABLE=1 tar --no-mac-metadata -czf "$out" -C "$(dirname "$app")" "$(basename "$app")"
  local first
  first=$(tar -tzf "$out" | sed -n 1p)
  case "$first" in
    "$(basename "$app")"/*) ;;
    *) echo "::error::$out starts with '$first', not $(basename "$app")/" >&2; exit 1 ;;
  esac
  sign "$out" "$version"
  ls -l "$out" "$out.sig"
}

# latest.json platform keys for each signed asset (the updater looks for {os}-{arch}-{bundle} first, then {os}-{arch}).
# Linux names only the AppImage key, so a deb or rpm build can never match.
platforms_for() {
  local name=$1 version=$2
  case "$name" in
    "Borealis_${version}_x64-setup.exe") echo "windows-x86_64-nsis windows-x86_64" ;;
    "Borealis_${version}_amd64.AppImage") echo "linux-x86_64-appimage" ;;
    "Borealis_${version}_aarch64.app.tar.gz") echo "darwin-aarch64-app darwin-aarch64" ;;
    *) echo "" ;;
  esac
}

cmd_feed() {
  local tag=${1:?usage: feed <tag> <version>} version=${2:?usage: feed <tag> <version>}
  local assets platforms='{}' sig name key keys url
  work=$(mktemp -d)
  trap 'rm -rf "$work"' EXIT
  assets=$(gh release view "$tag" --json assets --jq '.assets[].name')
  if ! grep -q '\.sig$' <<< "$assets"; then
    echo "no updater signatures on $tag; latest.json not written"
    return 0
  fi
  gh release download "$tag" --pattern '*.sig' --dir "$work/sigs"
  for sig in "$work"/sigs/*.sig; do
    name=$(basename "$sig" .sig)
    grep -qxF "$name" <<< "$assets" || { echo "::error::$name.sig is on $tag without $name" >&2; exit 1; }
    keys=$(platforms_for "$name" "$version")
    [ -n "$keys" ] || { echo "::warning::$name.sig is not an updater artifact; ignored" >&2; continue; }
    url="https://github.com/$GH_REPO/releases/download/$tag/$name"
    for key in $keys; do
      platforms=$(jq -c --arg k "$key" --arg s "$(cat "$sig")" --arg u "$url" '. + {($k): {signature: $s, url: $u}}' <<< "$platforms")
    done
  done
  jq -n --arg v "$version" --arg d "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --argjson p "$platforms" \
    '{version: $v, notes: ("Borealis " + $v), pub_date: $d, platforms: $p}' > "$work/latest.json"
  jq -r '.platforms | keys | join(", ")' "$work/latest.json" | sed 's/^/latest.json platforms: /'
  bash "$here/release-attach.sh" "$tag" "$work/latest.json"
}

case "${1:-}" in
  enabled) cmd_enabled ;;
  mac-archive) shift; cmd_mac_archive "$@" ;;
  feed) shift; cmd_feed "$@" ;;
  *) echo "usage: updater.sh enabled | mac-archive <app> <out> <version> | feed <tag> <version>" >&2; exit 64 ;;
esac
