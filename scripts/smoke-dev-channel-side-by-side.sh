#!/usr/bin/env bash
set -euo pipefail

STABLE_BUNDLE_ID="app.alanworks.macos"
DEV_BUNDLE_ID="app.alanworks.macos.dev"
STABLE_APP="${ALAN_STABLE_APP:-$HOME/Applications/Alan.app}"
DEV_APP="${ALAN_DEV_APP:-$HOME/Applications/Alan Dev.app}"
DEV_CLI="${ALAN_DEV_CLI:-}"

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

info() {
    printf '%s\n' "$*"
}

require_command() {
    command -v "$1" >/dev/null 2>&1 || fail "required command '$1' was not found"
}

plist_value() {
    local app="$1"
    local key="$2"
    /usr/bin/plutil -extract "$key" raw "$app/Contents/Info.plist"
}

bundle_process_ids() {
    local bundle_id="$1"
    /usr/bin/osascript - "$bundle_id" <<'APPLESCRIPT'
on run argv
    set targetBundleID to item 1 of argv
    tell application "System Events"
        set matches to unix id of every process whose bundle identifier is targetBundleID
    end tell
    set AppleScript's text item delimiters to " "
    return matches as text
end run
APPLESCRIPT
}

frontmost_bundle_id() {
    /usr/bin/osascript <<'APPLESCRIPT'
tell application "System Events"
    return bundle identifier of first process whose frontmost is true
end tell
APPLESCRIPT
}

wait_for_bundle() {
    local bundle_id="$1"
    local label="$2"
    local attempt
    local pids

    for attempt in $(seq 1 20); do
        pids="$(bundle_process_ids "$bundle_id")"
        if [[ -n "$pids" ]]; then
            printf '%s' "$pids"
            return 0
        fi
        sleep 1
    done

    fail "$label did not appear as running bundle id $bundle_id"
}

wait_for_frontmost_bundle() {
    local bundle_id="$1"
    local label="$2"
    local attempt
    local frontmost

    for attempt in $(seq 1 20); do
        frontmost="$(frontmost_bundle_id)"
        if [[ "$frontmost" == "$bundle_id" ]]; then
            return 0
        fi
        sleep 1
    done

    fail "$label did not become frontmost; last frontmost bundle id was '$frontmost'"
}

wait_for_path() {
    local path="$1"
    local label="$2"
    local attempt

    for attempt in $(seq 1 20); do
        if [[ -e "$path" ]]; then
            return 0
        fi
        sleep 1
    done

    fail "$label was not created: $path"
}

pid_count() {
    local pids="$1"

    if [[ -z "$pids" ]]; then
        printf '0'
        return 0
    fi

    printf '%s\n' "$pids" | wc -w | tr -d ' '
}

assert_single_pid_set() {
    local pids="$1"
    local label="$2"
    local count

    count="$(pid_count "$pids")"
    [[ "$count" == "1" ]] || fail "$label expected exactly one process, got $count: '$pids'"
}

activate_finder() {
    /usr/bin/osascript -e 'tell application "Finder" to activate' >/dev/null
}

resolve_dev_cli() {
    if [[ -n "$DEV_CLI" ]]; then
        [[ -x "$DEV_CLI" ]] || fail "ALAN_DEV_CLI is not executable: $DEV_CLI"
        printf '%s' "$DEV_CLI"
        return 0
    fi

    if [[ -x "$HOME/.local/bin/alan-dev" ]]; then
        printf '%s' "$HOME/.local/bin/alan-dev"
        return 0
    fi

    if command -v alan-dev >/dev/null 2>&1; then
        command -v alan-dev
        return 0
    fi

    fail "alan-dev was not found; run just install-dev or set ALAN_DEV_CLI"
}

assert_bundle_metadata() {
    local app="$1"
    local expected_bundle_id="$2"
    local expected_display_name="$3"
    local actual_bundle_id
    local actual_display_name

    [[ -d "$app" ]] || fail "app bundle does not exist: $app"
    actual_bundle_id="$(plist_value "$app" CFBundleIdentifier)"
    actual_display_name="$(plist_value "$app" CFBundleDisplayName)"
    [[ "$actual_bundle_id" == "$expected_bundle_id" ]] ||
        fail "$app bundle id: expected $expected_bundle_id, got $actual_bundle_id"
    [[ "$actual_display_name" == "$expected_display_name" ]] ||
        fail "$app display name: expected $expected_display_name, got $actual_display_name"
}

assert_dev_system_store_isolated() {
    local fixture_root="$1"
    local fixture_home="$fixture_root/home"
    local memory_source="$fixture_root/explicit-memory"
    local dev_store="$fixture_home/Library/Application Support/Alan/System Store/dev"
    local stable_store="$fixture_home/Library/Application Support/Alan/System Store/stable"

    mkdir -p "$fixture_home" "$memory_source"
    printf '%s\n' 'side-by-side channel isolation smoke' >"$memory_source/MEMORY.md"

    HOME="$fixture_home" ALAN_INSTALL_CHANNEL=dev \
        "$DEV_CLI" host legacy-state import memory-store "$memory_source" \
        --name side-by-side-smoke

    [[ -f "$dev_store/services/memory/stores/side-by-side-smoke/MEMORY.md" ]] ||
        fail "dev Memory Store import was not created"
    [[ ! -e "$stable_store" ]] ||
        fail "dev import unexpectedly created stable System Store state"
    [[ ! -e "$fixture_home/.alan" && ! -e "$fixture_home/.alan-dev" ]] ||
        fail "dev import recreated a retired Alan home"
}

require_command osascript
require_command open
require_command plutil
require_command mktemp

DEV_CLI="$(resolve_dev_cli)"
assert_bundle_metadata "$STABLE_APP" "$STABLE_BUNDLE_ID" "Alan"
assert_bundle_metadata "$DEV_APP" "$DEV_BUNDLE_ID" "Alan Dev"

tmp_root="${TMPDIR:-/tmp}"
tmp_root="${tmp_root%/}"
stable_shell_dir="$tmp_root/alan-shell-control"
dev_shell_dir="$tmp_root/alan-dev-shell-control"
dev_shell_window_dir="$dev_shell_dir/window_main"
dev_shell_socket="$dev_shell_window_dir/shell.sock"

stable_pids_before="$(bundle_process_ids "$STABLE_BUNDLE_ID")"
if [[ -z "$stable_pids_before" ]]; then
    info "Launching stable Alan..."
    /usr/bin/open -g "$STABLE_APP"
    stable_pids_before="$(wait_for_bundle "$STABLE_BUNDLE_ID" "stable Alan")"
fi
assert_single_pid_set "$stable_pids_before" "stable Alan before dev launch"

dev_pids_before="$(bundle_process_ids "$DEV_BUNDLE_ID")"
if [[ -n "$dev_pids_before" ]]; then
    fail "Alan Dev is already running with PID(s) $dev_pids_before; quit it before running this smoke so current-launch namespace checks are meaningful"
fi

[[ "$stable_shell_dir" != "$dev_shell_dir" ]] || fail "stable and dev shell namespaces match"
case "$dev_shell_dir" in
    "$tmp_root"/alan-dev-shell-control) ;;
    *) fail "refusing to clean unexpected dev shell-control path: $dev_shell_dir" ;;
esac
rm -rf "$dev_shell_dir"

activate_finder
frontmost_before="$(frontmost_bundle_id)"

info "Launching Alan Dev while stable Alan is running..."
/usr/bin/open -g "$DEV_APP"
dev_pids_after="$(wait_for_bundle "$DEV_BUNDLE_ID" "Alan Dev")"
stable_pids_after="$(bundle_process_ids "$STABLE_BUNDLE_ID")"
assert_single_pid_set "$dev_pids_after" "Alan Dev after first launch"
assert_single_pid_set "$stable_pids_after" "stable Alan after dev launch"
wait_for_path "$dev_shell_window_dir" "dev shell-control window namespace"
wait_for_path "$dev_shell_socket" "dev shell-control socket"
frontmost_after="$(frontmost_bundle_id)"

[[ "$stable_pids_after" == "$stable_pids_before" ]] ||
    fail "stable Alan PID changed: before '$stable_pids_before', after '$stable_pids_after'"
[[ "$frontmost_after" != "$STABLE_BUNDLE_ID" ]] ||
    fail "launching Alan Dev activated stable Alan"

info "Launching Alan Dev again to verify dev singleton reuse and activation..."
activate_finder
frontmost_before_duplicate_dev="$(frontmost_bundle_id)"
/usr/bin/open "$DEV_APP"
wait_for_frontmost_bundle "$DEV_BUNDLE_ID" "duplicate Alan Dev launch"
frontmost_after_duplicate_dev="$(frontmost_bundle_id)"
dev_pids_second="$(bundle_process_ids "$DEV_BUNDLE_ID")"
assert_single_pid_set "$dev_pids_second" "Alan Dev after duplicate launch"
[[ "$dev_pids_second" == "$dev_pids_after" ]] ||
    fail "second Alan Dev launch changed dev PID set: before '$dev_pids_after', after '$dev_pids_second'"

info "Launching stable Alan again to verify stable singleton reuse while dev is running..."
activate_finder
frontmost_before_duplicate_stable="$(frontmost_bundle_id)"
/usr/bin/open "$STABLE_APP"
wait_for_frontmost_bundle "$STABLE_BUNDLE_ID" "duplicate stable Alan launch"
frontmost_after_duplicate_stable="$(frontmost_bundle_id)"
stable_pids_second="$(bundle_process_ids "$STABLE_BUNDLE_ID")"
dev_pids_after_stable_relaunch="$(bundle_process_ids "$DEV_BUNDLE_ID")"
assert_single_pid_set "$stable_pids_second" "stable Alan after duplicate launch"
assert_single_pid_set "$dev_pids_after_stable_relaunch" "Alan Dev after stable duplicate launch"
[[ "$stable_pids_second" == "$stable_pids_after" ]] ||
    fail "second stable Alan launch changed stable PID set: before '$stable_pids_after', after '$stable_pids_second'"
[[ "$dev_pids_after_stable_relaunch" == "$dev_pids_second" ]] ||
    fail "second stable Alan launch changed dev PID set: before '$dev_pids_second', after '$dev_pids_after_stable_relaunch'"

store_fixture="$(mktemp -d "$tmp_root/alan-dev-channel-system-store.XXXXXX")"
assert_dev_system_store_isolated "$store_fixture"

if "$DEV_CLI" init --help >/dev/null 2>&1; then
    fail "retired alan init command is still available"
fi
if "$DEV_CLI" workspace --help >/dev/null 2>&1; then
    fail "retired alan workspace command is still available"
fi

info "Dev channel side-by-side smoke passed."
info "  stable pid(s): $stable_pids_second"
info "  dev pid(s): $dev_pids_after"
info "  dev pid(s) after duplicate launch: $dev_pids_second"
info "  stable pid(s) after duplicate launch: $stable_pids_second"
info "  frontmost before dev launch: $frontmost_before"
info "  frontmost after dev launch: $frontmost_after"
info "  frontmost before duplicate dev launch: $frontmost_before_duplicate_dev"
info "  frontmost after duplicate dev launch: $frontmost_after_duplicate_dev"
info "  frontmost before duplicate stable launch: $frontmost_before_duplicate_stable"
info "  frontmost after duplicate stable launch: $frontmost_after_duplicate_stable"
info "  dev shell-control: $dev_shell_dir"
info "  isolated dev System Store fixture: $store_fixture/home/Library/Application Support/Alan/System Store/dev"
