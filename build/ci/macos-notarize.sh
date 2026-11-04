#!/usr/bin/env bash
# Notarize, staple and package the signed macOS app in the release workflow (B-164).
#
#   macos-notarize.sh install-key                      write the API key from ASC_KEY_P8 (base64) to a private file
#                                                      under RUNNER_TEMP and print its path
#   macos-notarize.sh submit <file>                    submit to Apple's notary service without waiting; prints the id
#   macos-notarize.sh wait <submission id> <minutes>   poll until Apple decides. Exit 0 Accepted, 1 rejected (the
#                                                      notary log is printed), 2 still in progress after <minutes>
#   macos-notarize.sh dmg <Borealis.app> <tauri bundle dir> <out.dmg>
#                                                      build the DMG around the (stapled) app with the create-dmg
#                                                      script Tauri wrote to <bundle>/dmg, using Tauri's default layout
#   macos-notarize.sh verify <Borealis.app> <dmg>      signature, hardened runtime, stapled tickets, Gatekeeper
#   macos-notarize.sh finish <Borealis.app> <tauri bundle dir> <out.dmg> <minutes>
#                                                      after the app was accepted: staple it, build the DMG, sign,
#                                                      notarize (waiting up to <minutes>) and staple the DMG, verify
#
# Env: ASC_KEY_PATH, ASC_KEY_ID, ASC_ISSUER_ID (App Store Connect API key; notarytool authenticates with it);
# SIGNING_IDENTITY (finish: the Developer ID Application identity that signs the DMG).
# Apple's first notarization of a new app can take many hours; `wait` returning 2 is not a failure at Apple, the
# submission keeps going and the release workflow can finish it later (macos_resume_run, see release.yml).
set -euo pipefail

POLL_SECONDS=60

notary() {
  xcrun notarytool "$@" --key "$ASC_KEY_PATH" --key-id "$ASC_KEY_ID" --issuer "$ASC_ISSUER_ID"
}

cmd_install_key() {
  : "${ASC_KEY_P8:?ASC_KEY_P8 is empty: is the secret set in the release environment?}"
  local dir="${RUNNER_TEMP:?}/asc"
  mkdir -p "$dir"
  chmod 700 "$dir"
  printf '%s' "$ASC_KEY_P8" | base64 --decode > "$dir/AuthKey.p8"
  chmod 600 "$dir/AuthKey.p8"
  printf '%s\n' "$dir/AuthKey.p8"
}

cmd_submit() {
  local file=${1:?usage: submit <file>}
  local out
  out=$(notary submit "$file" --no-wait --output-format json)
  jq -er '.id' <<< "$out"
}

cmd_wait() {
  local id=${1:?usage: wait <submission id> <minutes>} minutes=${2:?usage: wait <submission id> <minutes>}
  local deadline=$(( $(date +%s) + minutes * 60 )) status=""
  while :; do
    # A failed status query (network) is retried until the deadline.
    status=$(notary info "$id" --output-format json 2>/dev/null | jq -r '.status // empty' || true)
    case "$status" in
      Accepted)
        echo "notarization $id: Accepted"
        return 0 ;;
      Invalid | Rejected)
        echo "::error::notarization $id: $status"
        notary log "$id" || true
        return 1 ;;
    esac
    if [ "$(date +%s)" -ge "$deadline" ]; then
      echo "notarization $id: ${status:-unknown} after $minutes min"
      return 2
    fi
    echo "notarization $id: ${status:-status unavailable}; checking again in ${POLL_SECONDS}s"
    sleep "$POLL_SECONDS"
  done
}

# Tauri's own DMG step (tauri-bundler macos/dmg) with its default layout: app at (180,170), the Applications link at
# (480,170), a 660x400 window. It runs the script from the app's folder with the app as the source.
cmd_dmg() {
  local app=${1:?usage: dmg <app> <bundle dir> <out.dmg>} bundle=${2:?} out=${3:?}
  bundle=$(cd "$bundle" && pwd)
  local script="$bundle/dmg/bundle_dmg.sh" icns name dir
  [ -f "$script" ] || { echo "::error::no create-dmg script at $script (run tauri build with the dmg bundle first)"; return 1; }
  icns=$(find "$bundle/dmg" -maxdepth 1 -name "*.icns" | head -n 1)
  name=$(basename "$out")
  dir=$(cd "$(dirname "$app")" && pwd)
  local args=(--volname Borealis --icon "$(basename "$app")" 180 170 --app-drop-link 480 170
    --window-size 660 400 --hide-extension "$(basename "$app")")
  [ -n "$icns" ] && args+=(--volicon "$icns")
  rm -f "$dir/$name"
  # The Finder layout step scripts Finder over AppleScript; if the runner refuses, build the plain image instead.
  if ! (cd "$dir" && bash "$script" "${args[@]}" "$name" "$(basename "$app")"); then
    echo "::warning::DMG window layout failed (Finder scripting); building the DMG without it"
    rm -f "$dir/$name" "$dir"/rw.*.dmg
    (cd "$dir" && bash "$script" "${args[@]}" --skip-jenkins "$name" "$(basename "$app")")
  fi
  mv "$dir/$name" "$out"
}

cmd_verify() {
  local app=${1:?usage: verify <app> <dmg>} dmg=${2:?usage: verify <app> <dmg>}
  codesign --verify --deep --strict "$app"
  echo "app signature: OK"
  codesign -dv "$app" 2>&1 | grep -q "Authority=Developer ID Application" \
    || { echo "::error::the app is not signed with a Developer ID Application certificate"; return 1; }
  codesign -dv "$app" 2>&1 | grep -qE "flags=.*runtime" \
    || { echo "::error::the app is not signed with the hardened runtime"; return 1; }
  echo "app: Developer ID, hardened runtime"
  codesign --verify --strict "$dmg"
  echo "dmg signature: OK"
  xcrun stapler validate "$app"
  xcrun stapler validate "$dmg"
  spctl -a -vvv -t exec "$app" 2>&1 | grep -q "source=Notarized Developer ID" \
    || { echo "::error::Gatekeeper does not accept the app as Notarized Developer ID"; spctl -a -vvv -t exec "$app" || true; return 1; }
  echo "Gatekeeper app: Notarized Developer ID"
  spctl -a -vvv -t open --context context:primary-signature "$dmg" 2>&1 | grep -q "source=Notarized Developer ID" \
    || { echo "::error::Gatekeeper does not accept the DMG as Notarized Developer ID"; return 1; }
  echo "Gatekeeper dmg: Notarized Developer ID"
  shasum -a 256 "$dmg"
}

cmd_finish() {
  local app=${1:?usage: finish <app> <bundle dir> <out.dmg> <minutes>} bundle=${2:?} out=${3:?} minutes=${4:?}
  : "${SIGNING_IDENTITY:?SIGNING_IDENTITY must name the Developer ID Application identity}"
  xcrun stapler staple "$app"
  cmd_dmg "$app" "$bundle" "$out"
  codesign --force --timestamp --sign "$SIGNING_IDENTITY" "$out"
  local id
  id=$(cmd_submit "$out")
  echo "DMG notarization submission: $id"
  local rc=0
  cmd_wait "$id" "$minutes" || rc=$?
  if [ "$rc" -eq 2 ]; then
    echo "::error::Apple has not finished notarizing the DMG (submission $id) after $minutes min; run the release again"
  fi
  [ "$rc" -eq 0 ] || return 1
  xcrun stapler staple "$out"
  cmd_verify "$app" "$out"
}

sub=${1:-}
shift || true
case "$sub" in
  install-key) cmd_install_key ;;
  submit) cmd_submit "$@" ;;
  wait) cmd_wait "$@" ;;
  dmg) cmd_dmg "$@" ;;
  verify) cmd_verify "$@" ;;
  finish) cmd_finish "$@" ;;
  *) echo "usage: macos-notarize.sh install-key|submit|wait|dmg|verify|finish ..." >&2; exit 64 ;;
esac
