#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

require_pattern() {
    local file="$1"
    local pattern="$2"
    local message="$3"

    if ! rg -n --pcre2 "$pattern" "$REPO_ROOT/$file" >/dev/null; then
        fail "$message"
    fi
}

reject_pattern() {
    local file="$1"
    local pattern="$2"
    local message="$3"

    if rg -n --pcre2 "$pattern" "$REPO_ROOT/$file" >/dev/null; then
        fail "$message"
    fi
}

"$SCRIPT_DIR/test-install-channel-descriptor.sh" >/dev/null

require_pattern "justfile" "^install-dev:" "justfile must expose install-dev"
require_pattern "justfile" "^uninstall-dev:" "justfile must expose uninstall-dev"
require_pattern "justfile" "^dev-channel-smoke:" "justfile must expose dev-channel-smoke"
require_pattern "scripts/assemble-release-app.sh" "ALAN_APP_BUNDLE_NAME" "assembly must use channel app bundle name"
require_pattern "scripts/assemble-release-app.sh" "ALAN_CARGO_TARGET_DIR" "assembly must allow repo-local cargo target override"
require_pattern "scripts/assemble-release-app.sh" 'PRODUCT_BUNDLE_IDENTIFIER="\$ALAN_BUNDLE_ID"' "assembly must override channel bundle id"
require_pattern "scripts/assemble-release-app.sh" 'INFOPLIST_KEY_CFBundleDisplayName="\$ALAN_DISPLAY_NAME"' "assembly must override channel display name"
reject_pattern "scripts/assemble-release-app.sh" 'local args=\(--force --options runtime --sign "\$SIGNING_IDENTITY"\)' "dev ad-hoc signing must not always enable hardened runtime"
require_pattern "scripts/assemble-release-app.sh" 'args=\(--force --sign "\$SIGNING_IDENTITY"\)' "assembly must start codesign args without hardened runtime for dev ad-hoc signing"
require_pattern "scripts/assemble-release-app.sh" 'args\+=\(--options runtime --timestamp\)' "assembly must keep hardened runtime for Developer ID signing"
reject_pattern "scripts/assemble-release-app.sh" "ALAN_TUI_NAME|clients/tui|\\bbun\\b" "assembly must not build a standalone TUI"
require_pattern "scripts/assemble-release-app.sh" "Dev channel builds are local-only" "assembly must block dev public release artifacts"
require_pattern "scripts/install.sh" "ALAN_CLI_NAME" "install script must link channel CLI name"
reject_pattern "scripts/install.sh" "ALAN_TUI_NAME|alan-tui" "install script must not link standalone TUI name"
require_pattern "scripts/uninstall.sh" "ALAN_CLI_NAME" "uninstall script must remove channel CLI name"
reject_pattern "scripts/uninstall.sh" "ALAN_TUI_NAME|alan-tui" "uninstall script must not remove standalone TUI name"
require_pattern "scripts/smoke-dev-channel-side-by-side.sh" "app.alanworks.macos.dev" "side-by-side smoke must identify dev bundle id"
require_pattern "scripts/smoke-dev-channel-side-by-side.sh" ".alan/runtime/dev" "side-by-side smoke must verify dev workspace runtime state"
require_pattern "packaging/homebrew/Casks/alan.rb.template" "app \"Alan\\.app\"" "Homebrew cask must remain stable-only"
require_pattern "packaging/homebrew/Casks/alan.rb.template" "target: \"alan\"" "Homebrew cask must keep stable CLI target"
reject_pattern "packaging/homebrew/Casks/alan.rb.template" "Alan Dev|alan-dev|alan-tui" "Homebrew cask must not publish dev or standalone TUI artifacts"

printf 'Dev channel install contract checks passed.\n'
