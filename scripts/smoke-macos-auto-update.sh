#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

OLD_APP="${ALAN_OLD_APP:-${1:-}}"
NEW_APP="${ALAN_NEW_APP:-${2:-}}"
APPCAST="${ALAN_APPCAST_PATH:-${3:-}}"

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

plist_value() {
    local plist="$1"
    local key="$2"
    plutil -extract "$key" raw -o - "$plist" 2>/dev/null || true
}

[[ -n "$OLD_APP" && -d "$OLD_APP" ]] ||
    fail "set ALAN_OLD_APP or pass an older signed Alan.app path"
[[ -n "$NEW_APP" && -d "$NEW_APP" ]] ||
    fail "set ALAN_NEW_APP or pass a newer signed Alan.app path"
[[ -n "$APPCAST" && -f "$APPCAST" ]] ||
    fail "set ALAN_APPCAST_PATH or pass a matching appcast.xml path"

"$SCRIPT_DIR/validate-appcast.sh" "$APPCAST" >/dev/null

old_info="$OLD_APP/Contents/Info.plist"
new_info="$NEW_APP/Contents/Info.plist"
[[ -f "$old_info" ]] || fail "old app Info.plist missing"
[[ -f "$new_info" ]] || fail "new app Info.plist missing"

old_build="$(plist_value "$old_info" CFBundleVersion)"
new_build="$(plist_value "$new_info" CFBundleVersion)"
feed_url="$(plist_value "$old_info" SUFeedURL)"

[[ -n "$old_build" && -n "$new_build" ]] ||
    fail "old and new app bundles must have CFBundleVersion"
awk -v old="$old_build" -v new="$new_build" 'BEGIN { exit !(new + 0 > old + 0) }' ||
    fail "new app build $new_build must be greater than old app build $old_build"
[[ "$feed_url" == "https://alanworks.app/appcast.xml" ]] ||
    fail "old app must be configured with the stable Sparkle feed"

if [[ "${ALAN_ALLOW_INTERACTIVE_UPDATE_SMOKE:-0}" != "1" ]]; then
    printf 'Preflight passed. Set ALAN_ALLOW_INTERACTIVE_UPDATE_SMOKE=1 to launch the old app for a manual Sparkle update check.\n'
    exit 0
fi

open -n "$OLD_APP"
printf 'Launched old app. Use Check for Updates... and verify it installs build %s from %s.\n' "$new_build" "$APPCAST"
