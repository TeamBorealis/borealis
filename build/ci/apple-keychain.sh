#!/usr/bin/env bash
# Import a signing certificate (.p12) into a throwaway keychain on a macOS runner (release workflow, B-164).
#
#   apple-keychain.sh import <identity prefix>   reads P12_BASE64 and P12_PASSWORD from the environment, prints the
#                                                full name of the codesigning identity that starts with the prefix
#   apple-keychain.sh delete                     removes the keychain again
#
# The keychain gets a random password, joins the user search list (so codesign and Tauri find the identity) and lets
# Apple's tools use the key without a prompt. Nothing secret is printed.
set -euo pipefail

KEYCHAIN="${RUNNER_TEMP:?RUNNER_TEMP is not set}/release-signing.keychain-db"

cmd_import() {
  local prefix=${1:?usage: import <identity prefix>}
  : "${P12_BASE64:?P12_BASE64 is empty: is the secret set in the release environment?}"
  : "${P12_PASSWORD:?P12_PASSWORD is empty: is the secret set in the release environment?}"
  local dir pass identity
  dir=$(mktemp -d)
  chmod 700 "$dir"
  trap 'rm -rf "$dir"' RETURN
  pass=$(openssl rand -hex 24)
  printf '%s' "$P12_BASE64" | base64 --decode > "$dir/cert.p12"
  if [ ! -f "$KEYCHAIN" ]; then
    security create-keychain -p "$pass" "$KEYCHAIN"
    security set-keychain-settings -lut 21600 "$KEYCHAIN"
    printf '%s' "$pass" > "$RUNNER_TEMP/release-signing.pass"
    chmod 600 "$RUNNER_TEMP/release-signing.pass"
  else
    pass=$(cat "$RUNNER_TEMP/release-signing.pass")
  fi
  security unlock-keychain -p "$pass" "$KEYCHAIN"
  security import "$dir/cert.p12" -k "$KEYCHAIN" -f pkcs12 -P "$P12_PASSWORD" \
    -T /usr/bin/codesign -T /usr/bin/productsign -T /usr/bin/security > /dev/null
  security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$pass" "$KEYCHAIN" > /dev/null
  # Keep the existing search list (login and system keychains) and put this one first.
  local current=()
  while IFS= read -r line; do
    line=${line//\"/}
    line=${line#"${line%%[![:space:]]*}"}
    if [ -n "$line" ] && [ "$line" != "$KEYCHAIN" ]; then current+=("$line"); fi
  done < <(security list-keychains -d user)
  security list-keychains -d user -s "$KEYCHAIN" ${current[@]+"${current[@]}"}
  identity=$(security find-identity -v -p codesigning "$KEYCHAIN" \
    | sed -nE "s/^ *[0-9]+\) [0-9A-F]{40} \"(${prefix}[^\"]*)\"\$/\1/p" | head -n 1)
  [ -n "$identity" ] || { echo "::error::no valid '$prefix' codesigning identity in the certificate" >&2; return 1; }
  printf '%s\n' "$identity"
}

cmd_delete() {
  security delete-keychain "$KEYCHAIN" 2> /dev/null || true
  rm -f "$RUNNER_TEMP/release-signing.pass"
}

case "${1:-}" in
  import) shift; cmd_import "$@" ;;
  delete) cmd_delete ;;
  *) echo "usage: apple-keychain.sh import <identity prefix> | delete" >&2; exit 64 ;;
esac
