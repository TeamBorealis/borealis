#!/usr/bin/env bash
# RL8 / B-236: sign (when Gradle has not) and check the Google Play bundle that release.yml built. Play App Signing
# takes only bundles signed with the Android release key (alias borealis, the key that signs the GitHub APK), so this
# fails unless the bundle has exactly one signer with the certificate SHA-1 below and the expected versionCode. The
# same check as the manual fallback ~/.borealis-signing/play-export/sign-play-aab.sh.
#
# Usage: build/ci/play-bundle.sh <built.aab> <out.aab> <versionCode>
# Env: ANDROID_KEYSTORE_PATH, ANDROID_KEYSTORE_PASSWORD (the APK's secrets); TAURI_PROPERTIES (default: the Gradle
# project's tauri.properties, which carries the versionCode the bundle was built with). Prints no secret.
set -euo pipefail

EXPECTED_SHA1="DB:B4:54:EF:DE:7E:90:43:6F:5A:55:05:42:0C:1A:40:61:EE:13:FA"
ALIAS=borealis

if [ "$#" -ne 3 ]; then
  echo "usage: $0 <built.aab> <out.aab> <versionCode>" >&2
  exit 2
fi
in=$1
out=$2
code=$3
props=${TAURI_PROPERTIES:-src-tauri/gen/android/app/tauri.properties}

fail() {
  echo "::error::$1"
  rm -f "$out"
  exit 1
}

[ -f "$in" ] || fail "Play bundle not found at $in"
[ -n "${ANDROID_KEYSTORE_PATH:-}" ] && [ -n "${ANDROID_KEYSTORE_PASSWORD:-}" ] ||
  fail "ANDROID_KEYSTORE_PATH and ANDROID_KEYSTORE_PASSWORD must be set"

built_code=$(sed -n 's/^tauri\.android\.versionCode=//p' "$props" 2>/dev/null | tr -d '[:space:]')
[ "$built_code" = "$code" ] || fail "the bundle was built with versionCode '${built_code}', expected $code ($props)"

cp "$in" "$out"
if ! jarsigner -verify "$out" | grep -q "^jar verified"; then
  echo "Gradle left the bundle unsigned: signing it with jarsigner (alias $ALIAS)"
  jarsigner -keystore "$ANDROID_KEYSTORE_PATH" -storepass:env ANDROID_KEYSTORE_PASSWORD \
    -keypass:env ANDROID_KEYSTORE_PASSWORD -sigalg SHA256withRSA -digestalg SHA-256 "$out" "$ALIAS" >/dev/null
fi

verify=$(jarsigner -verify "$out")
grep -q "^jar verified" <<<"$verify" || fail "jarsigner does not verify $out"
if grep -qi "unsigned entries" <<<"$verify"; then
  fail "$out has entries the signature does not cover"
fi

certs=$(keytool -printcert -jarfile "$out")
signers=$(grep -c "^Signer #" <<<"$certs" || true)
[ "$signers" = 1 ] || fail "$out has $signers signers; Play expects exactly one (the release key)"
sha1=$(awk '/SHA1:/{print $2; exit}' <<<"$certs")
sha256=$(awk '/SHA256:/{print $2; exit}' <<<"$certs")
[ "$sha1" = "$EXPECTED_SHA1" ] || fail "WRONG KEY: bundle signed with SHA-1 ${sha1:-none}, Play expects $EXPECTED_SHA1"

echo "OK: $out, versionCode $code, signed with the release key (SHA-1 $sha1, SHA-256 $sha256)"
