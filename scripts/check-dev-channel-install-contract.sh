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

display_name_plist_value="$(
    plutil -extract CFBundleDisplayName raw -o - \
        "$REPO_ROOT/clients/apple/alan-macos/Info.plist" 2>/dev/null || true
)"
expected_display_name_plist_value='$(PRODUCT_NAME)'
if [[ "$display_name_plist_value" != "$expected_display_name_plist_value" ]]; then
    fail "app Info.plist CFBundleDisplayName must expand PRODUCT_NAME so Alan Dev builds display as Alan Dev"
fi

require_pattern "justfile" "^install-dev:" "justfile must expose install-dev"
require_pattern "justfile" "^uninstall-dev:" "justfile must expose uninstall-dev"
require_pattern "justfile" "^dev-channel-smoke:" "justfile must expose dev-channel-smoke"
require_pattern "scripts/assemble-release-app.sh" "ALAN_APP_BUNDLE_NAME" "assembly must use channel app bundle name"
require_pattern "scripts/assemble-release-app.sh" "ALAN_CARGO_TARGET_DIR" "assembly must allow repo-local cargo target override"
require_pattern "scripts/assemble-release-app.sh" "aarch64-apple-darwin" "assembly must build the embedded CLI for Apple Silicon"
require_pattern "scripts/assemble-release-app.sh" 'CARGO_RELEASE_BIN' "assembly must copy the target-specific Cargo release binary"
require_pattern "scripts/assemble-release-app.sh" 'PRODUCT_BUNDLE_IDENTIFIER="\$ALAN_BUNDLE_ID"' "assembly must override channel bundle id"
require_pattern "scripts/assemble-release-app.sh" 'ALAN_APP_PRODUCT_NAME="\$ALAN_DISPLAY_NAME"' "assembly must override app product name without changing helper product name"
require_pattern "scripts/assemble-release-app.sh" 'INFOPLIST_KEY_CFBundleDisplayName="\$ALAN_DISPLAY_NAME"' "assembly must override channel display name"
require_pattern "scripts/install-dev.sh" 'ALAN_BUNDLE_VERSION=.*date -u' "dev install must generate a fresh bundle version for helper update registration"
require_pattern "scripts/assemble-release-app.sh" 'CURRENT_PROJECT_VERSION="\$ALAN_BUNDLE_VERSION"' "assembly must pass explicit bundle versions into Xcode when provided"
require_pattern "scripts/assemble-release-app.sh" '"bundle_version":' "assembly manifest must record the app bundle version"
reject_pattern "scripts/assemble-release-app.sh" 'local args=\(--force --options runtime --sign "\$SIGNING_IDENTITY"\)' "dev ad-hoc signing must not always enable hardened runtime"
require_pattern "scripts/assemble-release-app.sh" 'args=\(--force --sign "\$SIGNING_IDENTITY"\)' "assembly must start codesign args without hardened runtime for dev ad-hoc signing"
require_pattern "scripts/assemble-release-app.sh" 'args\+=\(--options runtime --timestamp\)' "assembly must keep hardened runtime for Developer ID signing"
reject_pattern "scripts/assemble-release-app.sh" "ALAN_TUI_NAME|clients/tui|\\bbun\\b" "assembly must not build a standalone TUI"
require_pattern "scripts/assemble-release-app.sh" "Dev channel builds are local-only" "assembly must block dev public release artifacts"
require_pattern "scripts/install.sh" "ALAN_CLI_NAME" "install script must link channel CLI name"
require_pattern "scripts/install-channel.sh" "ALAN_OS_HOST_NAME" "install channel must name the dedicated Alan OS Host"
reject_pattern "scripts/install.sh" "ALAN_TUI_NAME|alan-tui" "install script must not link standalone TUI name"
require_pattern "scripts/uninstall.sh" "ALAN_CLI_NAME" "uninstall script must remove channel CLI name"
reject_pattern "scripts/uninstall.sh" "ALAN_TUI_NAME|alan-tui" "uninstall script must not remove standalone TUI name"
require_pattern "scripts/smoke-dev-channel-side-by-side.sh" "app.alanworks.macos.dev" "side-by-side smoke must identify dev bundle id"
require_pattern "scripts/smoke-dev-channel-side-by-side.sh" "System Store/dev" "side-by-side smoke must verify dev System Store isolation"
require_pattern "packaging/homebrew/Casks/alan.rb.template" "app \"Alan\\.app\"" "Homebrew cask must remain stable-only"
require_pattern "packaging/homebrew/Casks/alan.rb.template" "target: \"alan\"" "Homebrew cask must keep stable CLI target"
reject_pattern "packaging/homebrew/Casks/alan.rb.template" "Alan Dev|alan-dev|alan-tui" "Homebrew cask must not publish dev or standalone TUI artifacts"

printf 'Dev channel install contract checks passed.\n'
