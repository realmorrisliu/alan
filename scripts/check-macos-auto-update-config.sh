#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PROJECT_FILE="$REPO_ROOT/clients/apple/alan-macos.xcodeproj/project.pbxproj"
INFO_PLIST="$REPO_ROOT/clients/apple/alan-macos/Info.plist"
RELEASE_SECRETS_DIR="$REPO_ROOT/release-secrets"
APPCAST_URL="https://alanworks.app/appcast.xml"

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

require_pattern() {
    local pattern="$1"
    local path="$2"
    local message="$3"

    rg -n "$pattern" "$path" >/dev/null || fail "$message"
}

[[ -f "$PROJECT_FILE" ]] || fail "missing Xcode project file: $PROJECT_FILE"
[[ -f "$INFO_PLIST" ]] || fail "missing app Info.plist: $INFO_PLIST"
[[ -f "$RELEASE_SECRETS_DIR/README.md" ]] || fail "missing release secrets README"

require_pattern \
    'repositoryURL = "https://github\.com/sparkle-project/Sparkle";' \
    "$PROJECT_FILE" \
    "Sparkle package URL is not configured"

require_pattern \
    'productName = Sparkle;' \
    "$PROJECT_FILE" \
    "Sparkle package product dependency is not configured"

require_pattern \
    '/\* Sparkle in Frameworks \*/' \
    "$PROJECT_FILE" \
    "Sparkle is not linked in the app framework phase"

require_pattern \
    'AlanMacUpdateController\.swift in Sources' \
    "$PROJECT_FILE" \
    "Sparkle update controller is not compiled into the macOS app"

require_pattern \
    'AlanMacUpdatePolicy\.swift in Sources' \
    "$PROJECT_FILE" \
    "macOS update policy is not compiled into the macOS app"

plist_binding_count="$(
    rg -c 'INFOPLIST_FILE = "alan-macos/Info.plist";' "$PROJECT_FILE" || true
)"
if [[ "$plist_binding_count" -lt 2 ]]; then
    fail "Debug and Release app builds must use the explicit app Info.plist"
fi

generated_plist_count="$(
    rg -c 'GENERATE_INFOPLIST_FILE = NO;' "$PROJECT_FILE" || true
)"
if [[ "$generated_plist_count" -lt 2 ]]; then
    fail "Debug and Release app builds must not rely on generated Info.plist metadata"
fi

feed_url="$(plutil -extract SUFeedURL raw -o - "$INFO_PLIST" 2>/dev/null || true)"
if [[ "$feed_url" != "$APPCAST_URL" ]]; then
    fail "Sparkle feed URL must be pinned in app Info.plist"
fi

public_key="$(plutil -extract SUPublicEDKey raw -o - "$INFO_PLIST" 2>/dev/null || true)"
if [[ ! "$public_key" =~ ^[A-Za-z0-9+/]{43}=$ ]]; then
    fail "Sparkle public key must be a 44-character base64 EdDSA public key"
fi

automatic_checks="$(plutil -extract SUEnableAutomaticChecks raw -o - "$INFO_PLIST" 2>/dev/null || true)"
automatic_install="$(plutil -extract SUAutomaticallyUpdate raw -o - "$INFO_PLIST" 2>/dev/null || true)"
if [[ "$automatic_checks" != "false" || "$automatic_install" != "false" ]]; then
    fail "first auto-update version must keep Sparkle checks and installs user-initiated"
fi

require_pattern \
    'SPUStandardUpdaterController' \
    "$REPO_ROOT/clients/apple/alan-macos/App/AlanMacUpdateController.swift" \
    "macOS app must own Sparkle updater initialization"

require_pattern \
    'mayPerform updateCheck' \
    "$REPO_ROOT/clients/apple/alan-macos/App/AlanMacUpdateController.swift" \
    "macOS app must block Sparkle checks for unsupported install paths"

require_pattern \
    'Check for Updates\.\.\.' \
    "$REPO_ROOT/clients/apple/alan-macos/App/AlanMacShellCommands.swift" \
    "macOS app menu must expose Check for Updates..."

require_pattern \
    'brew upgrade --cask alan' \
    "$REPO_ROOT/clients/apple/alan-macos/Support/AlanMacUpdatePolicy.swift" \
    "Homebrew-managed update policy must point users at brew upgrade --cask alan"

require_pattern \
    'validate-release-version-metadata\.sh' \
    "$REPO_ROOT/scripts/release-check.sh" \
    "release-check must validate version metadata before release"

require_pattern \
    'generate-appcast\.sh' \
    "$REPO_ROOT/scripts/test-appcast-tools.sh" \
    "appcast generation must have focused tests"

require_pattern \
    '^release-secrets/\*$' \
    "$REPO_ROOT/.gitignore" \
    "release-secrets must be ignored"

require_pattern \
    '^!release-secrets/README\.md$' \
    "$REPO_ROOT/.gitignore" \
    "release-secrets README must stay tracked as the local key location marker"

require_pattern \
    'release-secrets/sparkle_ed25519_private\.pem' \
    "$RELEASE_SECRETS_DIR/README.md" \
    "release secrets README must document the Sparkle private key location"

if ! git -C "$REPO_ROOT" check-ignore -q -- release-secrets/sparkle_ed25519_private.pem; then
    fail "Sparkle private key path under release-secrets must be ignored"
fi

if [[ -e "$REPO_ROOT/target/local-secrets/sparkle_ed25519_private.pem" ]]; then
    fail "Sparkle private key must live under release-secrets/, not target/"
fi

tracked_release_secret_count="$(
    git -C "$REPO_ROOT" ls-files release-secrets |
        awk '$0 != "release-secrets/README.md" { count++ } END { print count + 0 }'
)"
if [[ "$tracked_release_secret_count" != "0" ]]; then
    fail "release-secrets must not track secret material"
fi

if git -C "$REPO_ROOT" ls-files | rg -i 'sparkle.*(private|secret)|ed25519.*private' >/dev/null; then
    fail "Sparkle private key material must not be tracked"
fi

printf 'macOS auto-update config check passed\n'
