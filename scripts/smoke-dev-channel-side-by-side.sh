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

assert_no_stable_workspace_runtime_state() {
    local workspace="$1"

    [[ -d "$workspace/.alan/runtime/dev/sessions" ]] ||
        fail "dev sessions directory was not created"
    [[ -d "$workspace/.alan/runtime/dev/memory" ]] ||
        fail "dev memory directory was not created"
    [[ ! -e "$workspace/.alan/sessions" ]] ||
        fail "legacy stable sessions path was created by dev smoke"
    [[ ! -e "$workspace/.alan/memory" ]] ||
        fail "legacy stable memory path was created by dev smoke"
    [[ ! -e "$workspace/.alan/runtime/stable" ]] ||
        fail "stable runtime namespace was created by dev smoke"
}

require_command osascript
require_command open
require_command plutil
require_command mktemp

DEV_CLI="$(resolve_dev_cli)"
assert_bundle_metadata "$STABLE_APP" "$STABLE_BUNDLE_ID" "Alan"
assert_bundle_metadata "$DEV_APP" "$DEV_BUNDLE_ID" "Alan Dev"

stable_pids_before="$(bundle_process_ids "$STABLE_BUNDLE_ID")"
if [[ -z "$stable_pids_before" ]]; then
    info "Launching stable Alan..."
    /usr/bin/open -g "$STABLE_APP"
    stable_pids_before="$(wait_for_bundle "$STABLE_BUNDLE_ID" "stable Alan")"
fi

dev_pids_before="$(bundle_process_ids "$DEV_BUNDLE_ID")"

/usr/bin/osascript -e 'tell application "Finder" to activate' >/dev/null
frontmost_before="$(frontmost_bundle_id)"

info "Launching Alan Dev while stable Alan is running..."
/usr/bin/open -g "$DEV_APP"
dev_pids_after="$(wait_for_bundle "$DEV_BUNDLE_ID" "Alan Dev")"
stable_pids_after="$(bundle_process_ids "$STABLE_BUNDLE_ID")"
frontmost_after="$(frontmost_bundle_id)"

[[ "$stable_pids_after" == "$stable_pids_before" ]] ||
    fail "stable Alan PID changed: before '$stable_pids_before', after '$stable_pids_after'"
if [[ -n "$dev_pids_before" && "$dev_pids_after" != "$dev_pids_before" ]]; then
    fail "duplicate Alan Dev launch changed dev PID set: before '$dev_pids_before', after '$dev_pids_after'"
fi
[[ "$frontmost_after" != "$STABLE_BUNDLE_ID" ]] ||
    fail "launching Alan Dev activated stable Alan"

tmp_root="${TMPDIR:-/tmp}"
tmp_root="${tmp_root%/}"
stable_shell_dir="$tmp_root/alan-shell-control"
dev_shell_dir="$tmp_root/alan-dev-shell-control"
[[ "$stable_shell_dir" != "$dev_shell_dir" ]] || fail "stable and dev shell namespaces match"
[[ -d "$dev_shell_dir" ]] || fail "dev shell-control namespace was not created: $dev_shell_dir"

workspace="$(mktemp -d "$tmp_root/alan-dev-channel-smoke-workspace.XXXXXX")"
workspace_alias="alan-dev-channel-smoke-$(basename "$workspace")"
"$DEV_CLI" init --path "$workspace" --name "$workspace_alias" --silent
assert_no_stable_workspace_runtime_state "$workspace"
[[ -f "$HOME/.alan-dev/registry.json" ]] ||
    fail "dev registry was not written under ~/.alan-dev"

info "Dev channel side-by-side smoke passed."
info "  stable pid(s): $stable_pids_after"
info "  dev pid(s): $dev_pids_after"
info "  frontmost before dev launch: $frontmost_before"
info "  frontmost after dev launch: $frontmost_after"
info "  dev shell-control: $dev_shell_dir"
info "  dev workspace state: $workspace/.alan/runtime/dev"
