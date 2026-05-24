#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
DEFAULT_APP_PATH="$REPO_ROOT/debug/xcode-derived/alan-macos-build/Build/Products/Debug/Alan.app"
APP_PATH="${ALAN_APP_INTENTS_APP_PATH:-$DEFAULT_APP_PATH}"
ACTIONS_DATA="$APP_PATH/Contents/Resources/Metadata.appintents/extract.actionsdata"

fail() {
    printf 'check-shell-app-intents-metadata: %s\n' "$*" >&2
    exit 1
}

json_get() {
    local key_path="$1"
    plutil -extract "$key_path" raw -o - "$ACTIONS_DATA" 2>/dev/null \
        || fail "missing metadata key: $key_path"
}

expect_value() {
    local key_path="$1"
    local expected="$2"
    local actual
    actual="$(json_get "$key_path")"
    if [[ "$actual" != "$expected" ]]; then
        fail "expected $key_path to be '$expected', got '$actual'"
    fi
}

expect_action() {
    local intent="$1"
    local title="$2"
    local description="$3"
    local parameters="$4"
    local index=0
    local parameter

    expect_value "actions.$intent.title.key" "$title"
    expect_value "actions.$intent.descriptionMetadata.descriptionText.key" "$description"
    expect_value "actions.$intent.availabilityAnnotations.LNPlatformNameMACOS.introducedVersion" "13.0"
    expect_value "actions.$intent.visibilityMetadata.isDiscoverable" "true"

    IFS='|' read -r -a parameter_names <<<"$parameters"
    for parameter in "${parameter_names[@]}"; do
        [[ -n "$parameter" ]] || continue
        expect_value "actions.$intent.parameters.$index.title.key" "$parameter"
        index=$((index + 1))
    done
}

command -v plutil >/dev/null 2>&1 || fail "plutil is required"
[[ -f "$ACTIONS_DATA" ]] || fail "missing App Intents metadata: $ACTIONS_DATA; build alan-macos first or set ALAN_APP_INTENTS_APP_PATH"

expect_action "AlanCreateTerminalTabIntent" \
    "Create Terminal Tab" \
    "Create a terminal tab in alan." \
    "Space|Title|Working Directory"
expect_action "AlanCreateAlanTabIntent" \
    "Create Alan Tab" \
    "Create an alan tab in alan." \
    "Space|Title|Working Directory"
expect_action "AlanSplitPaneIntent" \
    "Split Shell Pane" \
    "Split an alan shell pane." \
    "Pane|Direction"
expect_action "AlanFocusPaneIntent" \
    "Focus Shell Pane" \
    "Focus an alan shell pane." \
    "Pane"
expect_action "AlanClosePaneIntent" \
    "Close Shell Pane" \
    "Close an alan shell pane." \
    "Pane"
expect_action "AlanCloseTabIntent" \
    "Close Shell Tab" \
    "Close an alan shell tab." \
    "Tab"
expect_action "AlanSendTextToPaneIntent" \
    "Send Text To Shell Pane" \
    "Send text to an alan shell pane." \
    "Pane|Text"
expect_action "AlanReadPaneSummaryIntent" \
    "Read Shell Pane Summary" \
    "Read safe metadata for an alan shell pane." \
    "Pane"
expect_action "AlanOpenAttentionItemIntent" \
    "Open Shell Attention Item" \
    "Open an alan shell attention item." \
    "Attention Item"

expect_value "entities.AlanShellWindowEntity.displayTypeName.key" "Shell Window"
expect_value "entities.AlanShellSpaceEntity.displayTypeName.key" "Shell Space"
expect_value "entities.AlanShellTabEntity.displayTypeName.key" "Shell Tab"
expect_value "entities.AlanShellPaneEntity.displayTypeName.key" "Shell Pane"
expect_value "entities.AlanShellAttentionItemEntity.displayTypeName.key" "Shell Attention Item"

if grep -Eq 'SECRET|secret_|pane_[0-9]|pane_secret|alan-shell-control|/tmp/alan|binding file|socket path|controlPath|visibleExcerpt' "$ACTIONS_DATA"; then
    fail "generated App Intents metadata contains private terminal/debug strings"
fi

printf 'Shell App Intents metadata checks passed.\n'
