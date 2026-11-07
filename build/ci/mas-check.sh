#!/usr/bin/env bash
# Check a Mac App Store build of Borealis (MAS1, B-190; specs/mac-app-store.md):
#
#   mas-check.sh <Borealis.app>
#
# - the signature carries exactly the sandbox entitlements of src-tauri/mas/Entitlements.plist (the signing step may
#   add the provisioning profile's application-identifier and team-identifier; nothing else);
# - Info.plist has LSApplicationCategoryType, ITSAppUsesNonExemptEncryption = false, LSMinimumSystemVersion 12.0 and
#   the local network keys;
# - Contents/Resources/container-migration.plist is present and valid, the AI profile is not bundled;
# - the executable is the build without the AI CLI integration. This is checked in the file and never by running
#   it: a sandboxed launch on a developer Mac creates the app container and moves that Mac's DMG data into it.
#
# Works on an unsigned-by-Apple build (ad-hoc signature). Exit 0 when every check passes, 1 otherwise.
set -euo pipefail

app="${1:?usage: mas-check.sh <Borealis.app>}"
plist="$app/Contents/Info.plist"
failures=0

fail() {
  echo "FAIL: $*" >&2
  failures=$((failures + 1))
}

ok() {
  echo "ok: $*"
}

[ -d "$app" ] || { echo "no app bundle at $app" >&2; exit 1; }

# Entitlements of the signed bundle, as XML.
entitlements="$(mktemp)"
trap 'rm -f "$entitlements"' EXIT
if ! codesign -d --entitlements - --xml "$app" >"$entitlements" 2>/dev/null || [ ! -s "$entitlements" ]; then
  fail "the app has no entitlements in its signature"
else
  for key in com.apple.security.app-sandbox com.apple.security.network.client com.apple.security.network.server \
    com.apple.security.files.user-selected.read-write com.apple.security.files.bookmarks.app-scope; do
    if [ "$(/usr/libexec/PlistBuddy -c "Print :$key" "$entitlements" 2>/dev/null)" = "true" ]; then
      ok "entitlement $key"
    else
      fail "entitlement $key is missing or not true"
    fi
  done
  allowed=" com.apple.security.app-sandbox com.apple.security.network.client com.apple.security.network.server \
com.apple.security.files.user-selected.read-write com.apple.security.files.bookmarks.app-scope \
com.apple.application-identifier com.apple.developer.team-identifier "
  while IFS= read -r key; do
    [ -n "$key" ] || continue
    case "$allowed" in
      *" $key "*) ;;
      *) fail "unexpected entitlement $key" ;;
    esac
  done < <(plutil -convert json -o - "$entitlements" | /usr/bin/python3 -c 'import json,sys; print("\n".join(json.load(sys.stdin)))')
fi

value() {
  /usr/libexec/PlistBuddy -c "Print :$1" "$plist" 2>/dev/null || true
}

expect() {
  local key="$1" want="$2" got
  got="$(value "$key")"
  if [ "$got" = "$want" ]; then ok "$key = $want"; else fail "$key is '$got', expected '$want'"; fi
}

expect CFBundleIdentifier app.borealis
expect LSApplicationCategoryType public.app-category.role-playing-games
expect ITSAppUsesNonExemptEncryption false
expect LSMinimumSystemVersion 12.0
expect NSBonjourServices:0 _borealis-link._tcp
if [ -n "$(value NSLocalNetworkUsageDescription)" ]; then ok "NSLocalNetworkUsageDescription"; else fail "NSLocalNetworkUsageDescription is missing"; fi

migration="$app/Contents/Resources/container-migration.plist"
if plutil -lint "$migration" >/dev/null 2>&1 \
  && [ "$(/usr/libexec/PlistBuddy -c 'Print :Move:0' "$migration" 2>/dev/null)" = '${ApplicationSupport}/app.borealis' ]; then
  ok "container-migration.plist moves the DMG data folder"
else
  fail "Contents/Resources/container-migration.plist is missing or does not move \${ApplicationSupport}/app.borealis"
fi

if [ -e "$app/Contents/Resources/ai-profile" ]; then fail "the AI profile is bundled"; else ok "no AI profile bundled"; fi
if [ -f "$app/Contents/Resources/content/core/pack.json" ]; then ok "bundled content present"; else fail "bundled content is missing"; fi

# Never run the executable here: a sandboxed launch creates the app.borealis container, and macOS then MOVES the DMG
# build's data folder into it (container-migration.plist) on whatever Mac runs this check. Inspect the file instead.
exe="$app/Contents/MacOS/$(value CFBundleExecutable)"
if grep -a -q "this build of Borealis has no MCP mode" "$exe"; then
  ok "the executable is the build without MCP mode"
else
  fail "the executable lacks the no-MCP-mode message: not built with --features mas?"
fi
if grep -a -q "osascript" "$exe"; then
  fail "the executable still contains the AI CLI setup (osascript)"
else
  ok "no AI CLI setup in the executable"
fi
# No in-app updater (#25, UP2): the overlay drops plugins.updater, so the update feed address is not in the binary.
if grep -a -q "releases/latest/download/latest.json" "$exe"; then
  fail "the executable carries the updater feed: was tauri.mas.conf.json applied?"
else
  ok "no in-app updater feed in the executable"
fi

if [ "$failures" -ne 0 ]; then
  echo "$failures check(s) failed" >&2
  exit 1
fi
echo "every Mac App Store check passed"
