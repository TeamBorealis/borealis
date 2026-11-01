#!/usr/bin/env bash
# B-85: a Google Play bundle (.aab) can only be built as a store build. Asks Gradle to plan (dry run, nothing is
# built) three builds of src-tauri/gen/android and checks the guard in app/build.gradle.kts:
#   1. bundleUniversalRelease without BOREALIS_DISTRIBUTION=play   -> refused with the guard's message
#   2. bundleUniversalRelease with BOREALIS_DISTRIBUTION=play      -> planned
#   3. assembleUniversalRelease without it (the sideloaded APK)    -> planned
# Needs the Android SDK (ANDROID_HOME) and a project Tauri has prepared once (tauri.settings.gradle,
# app/tauri.build.gradle.kts), so run it after an `android build`. Exit 0 when all three hold.
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
android="$here/../../src-tauri/gen/android"
guard="must be a store build"
log="$(mktemp)"
trap 'rm -f "$log"' EXIT
fail=0

plan() { # <expected: ok|refused> <label> <env value or empty> <task>
  local want=$1 label=$2 value=$3 task=$4 code
  if [ -n "$value" ]; then
    (cd "$android" && BOREALIS_DISTRIBUTION="$value" ./gradlew --quiet --dry-run "$task") >"$log" 2>&1
  else
    (cd "$android" && env -u BOREALIS_DISTRIBUTION ./gradlew --quiet --dry-run "$task") >"$log" 2>&1
  fi
  code=$?
  if [ "$want" = refused ]; then
    if [ "$code" -ne 0 ] && grep -q "$guard" "$log"; then echo "ok: $label is refused"; return; fi
  elif [ "$code" -eq 0 ]; then
    echo "ok: $label is planned"; return
  fi
  echo "FAIL: $label (exit $code, wanted $want)"; tail -20 "$log"; fail=1
}

plan refused "a Play bundle without BOREALIS_DISTRIBUTION=play" "" bundleUniversalRelease
plan refused "a Play bundle with BOREALIS_DISTRIBUTION=github" "github" bundleUniversalRelease
plan ok "a Play bundle with BOREALIS_DISTRIBUTION=play" "play" bundleUniversalRelease
plan ok "the sideloaded APK without the switch" "" assembleUniversalRelease
exit "$fail"
