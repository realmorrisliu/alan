#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=scripts/install-channel.sh
source "$SCRIPT_DIR/install-channel.sh"

VALIDATE_CHANNEL="${ALAN_VALIDATE_CHANNEL:-${ALAN_INSTALL_CHANNEL:-stable}}"
if [[ -z "${ALAN_VALIDATE_CHANNEL:-}" && -n "${1:-}" ]]; then
    case "$1" in
        *"/Alan Dev.app"|*"Alan Dev.app")
            VALIDATE_CHANNEL="dev"
            ;;
    esac
fi
alan_install_channel_load "$VALIDATE_CHANNEL"

DERIVED_DATA="${ALAN_XCODE_DERIVED_DATA:-$REPO_ROOT/target/xcode-derived}"
APP_BUNDLE="${1:-$DERIVED_DATA/Build/Products/Release/$ALAN_APP_BUNDLE_NAME}"
MANIFEST="$APP_BUNDLE/Contents/Resources/alan-package-manifest.json"
ALAN_BIN="$APP_BUNDLE/Contents/Resources/bin/$ALAN_CLI_NAME"

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

require_executable() {
    local path="$1"
    [[ -x "$path" ]] || fail "expected executable at $path"
}

require_developer_id_signature() {
    local path="$1"
    local details

    details="$(codesign -dv --verbose=4 "$path" 2>&1)" || fail "codesign could not inspect $path"
    if printf '%s\n' "$details" | grep -q 'Signature=adhoc'; then
        fail "ad-hoc signature is not allowed for $path"
    fi
    if ! printf '%s\n' "$details" | grep -q 'Authority=Developer ID Application'; then
        fail "Developer ID Application signature is required for $path"
    fi
}

manifest_value() {
    local key="$1"

    sed -nE "s/.*\"$key\": \"([^\"]+)\".*/\\1/p" "$MANIFEST" | head -n 1
}

manifest_binary_sha256() {
    local binary="$1"

    awk -v binary="\"$binary\"" '
        $0 ~ binary "[[:space:]]*:" {
            in_binary = 1
            next
        }
        in_binary && /"sha256"[[:space:]]*:/ {
            value = $0
            sub(/^.*"sha256"[[:space:]]*:[[:space:]]*"/, "", value)
            sub(/".*$/, "", value)
            print value
            exit
        }
        in_binary && /^[[:space:]]*}/ {
            in_binary = 0
        }
    ' "$MANIFEST"
}

require_manifest_checksum() {
    local binary="$1"
    local path="$2"
    local expected
    local actual

    expected="$(manifest_binary_sha256 "$binary")"
    [[ -n "$expected" ]] || fail "manifest does not record sha256 for $binary"
    actual="$(shasum -a 256 "$path" | awk '{print $1}')"
    if [[ "$actual" != "$expected" ]]; then
        fail "manifest sha256 for $binary does not match embedded binary"
    fi
}

[[ -d "$APP_BUNDLE" ]] || fail "app bundle not found: $APP_BUNDLE"
require_executable "$APP_BUNDLE/Contents/MacOS/$ALAN_DISPLAY_NAME"
require_executable "$ALAN_BIN"
[[ -f "$MANIFEST" ]] || fail "package manifest not found: $MANIFEST"
if [[ -e "$APP_BUNDLE/Contents/Resources/bin/alan-tui" ||
    -e "$APP_BUNDLE/Contents/Resources/bin/alan-dev-tui" ]]; then
    fail "release app must not embed a standalone alan-tui binary"
fi
if grep -Eq 'alan(-dev)?-tui' "$MANIFEST"; then
    fail "package manifest must not record a standalone alan-tui binary"
fi

grep -q "\"install_channel\": \"$ALAN_CHANNEL_ID\"" "$MANIFEST" ||
    fail "manifest does not record $ALAN_CHANNEL_ID install channel"
grep -q "\"package\": \"$ALAN_APP_BUNDLE_NAME\"" "$MANIFEST" ||
    fail "manifest does not record $ALAN_APP_BUNDLE_NAME package name"
grep -q "\"bundle_identifier\": \"$ALAN_BUNDLE_ID\"" "$MANIFEST" ||
    fail "manifest does not record $ALAN_BUNDLE_ID bundle id"
grep -q "\"path\": \"Contents/Resources/bin/$ALAN_CLI_NAME\"" "$MANIFEST" ||
    fail "manifest does not record embedded $ALAN_CLI_NAME path"

manifest_version="$(manifest_value "version")"
repo_version="$(awk -F '"' '/^version = / { print $2; exit }' "$REPO_ROOT/Cargo.toml")"
[[ -n "$manifest_version" ]] || fail "manifest does not record package version"
if [[ "$manifest_version" != "$repo_version" ]]; then
    fail "manifest version $manifest_version does not match Cargo.toml version $repo_version"
fi
require_manifest_checksum "$ALAN_CLI_NAME" "$ALAN_BIN"

require_developer_id_signature "$ALAN_BIN"
require_developer_id_signature "$APP_BUNDLE"
codesign --verify --strict --verbose=2 "$APP_BUNDLE" >/dev/null

if [[ "${ALAN_VALIDATE_NOTARIZATION:-0}" == "1" ]]; then
    xcrun stapler validate "$APP_BUNDLE" >/dev/null
fi

printf 'Release app validation passed: %s\n' "$APP_BUNDLE"
