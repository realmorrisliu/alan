#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# shellcheck source=scripts/release-env.sh
source "$SCRIPT_DIR/release-env.sh"
# shellcheck source=scripts/install-channel.sh
source "$SCRIPT_DIR/install-channel.sh"

alan_install_channel_load "${ALAN_INSTALL_CHANNEL:-stable}"

DERIVED_DATA="${ALAN_XCODE_DERIVED_DATA:-$REPO_ROOT/target/xcode-derived}"
ARTIFACT_DIR="${ALAN_RELEASE_ARTIFACT_DIR:-$REPO_ROOT/target/release-artifacts}"
STAGING_DIR="$ARTIFACT_DIR/staging"
APP_BUNDLE="$DERIVED_DATA/Build/Products/Release/$ALAN_APP_BUNDLE_NAME"
EMBEDDED_BIN_DIR="$APP_BUNDLE/Contents/Resources/bin"
MANIFEST_PATH="$APP_BUNDLE/Contents/Resources/alan-package-manifest.json"
TUI_ENTITLEMENTS="$REPO_ROOT/scripts/entitlements/alan-tui.entitlements"
SIGNING_IDENTITY="${ALAN_DEVELOPER_ID_APPLICATION:-${ALAN_SIGNING_IDENTITY:-}}"
NOTARIZE="${ALAN_NOTARIZE:-0}"
CREATE_ARCHIVE="${ALAN_CREATE_RELEASE_ARCHIVE:-$NOTARIZE}"
VERSION="$(awk -F '"' '/^version = / { print $2; exit }' "$REPO_ROOT/Cargo.toml")"
REVISION="$(git -C "$REPO_ROOT" rev-parse HEAD 2>/dev/null || printf 'unknown')"
DIRTY="false"

if ! git -C "$REPO_ROOT" diff --quiet --ignore-submodules -- 2>/dev/null ||
    ! git -C "$REPO_ROOT" diff --cached --quiet --ignore-submodules -- 2>/dev/null; then
    DIRTY="true"
fi

if alan_install_channel_is_dev && [[ -z "$SIGNING_IDENTITY" ]]; then
    SIGNING_IDENTITY="-"
fi

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

require_command() {
    if ! command -v "$1" >/dev/null 2>&1; then
        fail "required command '$1' was not found"
    fi
}

require_signing_identity() {
    if [[ "$SIGNING_IDENTITY" == "-" ]]; then
        return
    fi

    if [[ -z "$SIGNING_IDENTITY" ]]; then
        fail "Developer ID signing identity is required. Set ALAN_DEVELOPER_ID_APPLICATION='Developer ID Application: ...', ALAN_SIGNING_IDENTITY, or ALAN_RELEASE_ENV_FILE."
    fi

    local identities
    local matched_identity
    local common_name

    identities="$(security find-identity -v -p codesigning)" ||
        fail "could not inspect codesigning identities in the current keychain"
    matched_identity="$(printf '%s\n' "$identities" | grep -F "$SIGNING_IDENTITY" | head -n 1 || true)"
    if [[ -z "$matched_identity" ]]; then
        fail "Developer ID signing identity is configured, but no valid codesigning identity in the current keychain matches it. Run: security find-identity -v -p codesigning"
    fi

    common_name="$(printf '%s\n' "$matched_identity" | sed -E 's/^[[:space:]]*[0-9]+\\)[[:space:]]+[A-Fa-f0-9]+[[:space:]]+"(.*)"$/\1/')"
    if [[ -n "$common_name" && "$common_name" != "$matched_identity" ]]; then
        SIGNING_IDENTITY="$common_name"
    fi
}

json_escape() {
    printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'
}

sha256() {
    shasum -a 256 "$1" | awk '{print $1}'
}

sign_path() {
    local path="$1"
    local args=(--force --options runtime --sign "$SIGNING_IDENTITY")
    if [[ "$SIGNING_IDENTITY" != "-" ]]; then
        args+=(--timestamp)
    fi
    codesign "${args[@]}" "$path"
}

sign_path_with_entitlements() {
    local path="$1"
    local entitlements="$2"
    local args=(--force --options runtime --entitlements "$entitlements" --sign "$SIGNING_IDENTITY")
    if [[ "$SIGNING_IDENTITY" != "-" ]]; then
        args+=(--timestamp)
    fi
    codesign "${args[@]}" "$path"
}

if ! alan_install_channel_is_stable && [[ "$NOTARIZE" == "1" || "$CREATE_ARCHIVE" == "1" ]]; then
    fail "Dev channel builds are local-only and cannot create public release archives or notarization submissions."
fi

require_command cargo
require_command bun
require_command xcodebuild
require_command codesign
require_command ditto
require_command shasum
if [[ "$SIGNING_IDENTITY" != "-" ]]; then
    require_command security
fi
require_signing_identity

[[ -f "$TUI_ENTITLEMENTS" ]] || fail "alan-tui entitlements file not found: $TUI_ENTITLEMENTS"

mkdir -p "$STAGING_DIR" "$ARTIFACT_DIR"

printf 'Building release alan CLI for %s channel...\n' "$ALAN_CHANNEL_ID"
cargo build --release -p alan

printf 'Building standalone %s...\n' "$ALAN_TUI_NAME"
(
    cd "$REPO_ROOT/clients/tui"
    bun install --frozen-lockfile 2>/dev/null || bun install
    bun run build:js
    ALAN_TUI_BINARY_OUTFILE="$STAGING_DIR/$ALAN_TUI_NAME" bun run build:standalone
)
chmod +x "$STAGING_DIR/$ALAN_TUI_NAME"

if [[ -e "$APP_BUNDLE" ]]; then
    printf 'Removing stale Release %s build product...\n' "$ALAN_APP_BUNDLE_NAME"
    rm -rf "$APP_BUNDLE"
fi

printf 'Building Release %s...\n' "$ALAN_APP_BUNDLE_NAME"
xcodebuild \
    -project "$REPO_ROOT/clients/apple/alan-macos.xcodeproj" \
    -scheme alan-macos \
    -configuration Release \
    -destination generic/platform=macOS \
    -derivedDataPath "$DERIVED_DATA" \
    PRODUCT_BUNDLE_IDENTIFIER="$ALAN_BUNDLE_ID" \
    PRODUCT_NAME="$ALAN_DISPLAY_NAME" \
    INFOPLIST_KEY_CFBundleDisplayName="$ALAN_DISPLAY_NAME" \
    CODE_SIGNING_ALLOWED=NO \
    build

if [[ ! -d "$APP_BUNDLE" ]]; then
    fail "Release build did not produce $APP_BUNDLE"
fi

printf 'Embedding CLI and TUI into %s...\n' "$ALAN_APP_BUNDLE_NAME"
mkdir -p "$EMBEDDED_BIN_DIR"
cp "$REPO_ROOT/target/release/alan" "$EMBEDDED_BIN_DIR/$ALAN_CLI_NAME"
cp "$STAGING_DIR/$ALAN_TUI_NAME" "$EMBEDDED_BIN_DIR/$ALAN_TUI_NAME"
chmod +x "$EMBEDDED_BIN_DIR/$ALAN_CLI_NAME" "$EMBEDDED_BIN_DIR/$ALAN_TUI_NAME"

ASSEMBLED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
printf 'Signing embedded binaries...\n'
sign_path "$EMBEDDED_BIN_DIR/$ALAN_CLI_NAME"
sign_path_with_entitlements "$EMBEDDED_BIN_DIR/$ALAN_TUI_NAME" "$TUI_ENTITLEMENTS"

printf 'Recording signed embedded binary checksums...\n'
ALAN_SHA="$(sha256 "$EMBEDDED_BIN_DIR/$ALAN_CLI_NAME")"
TUI_SHA="$(sha256 "$EMBEDDED_BIN_DIR/$ALAN_TUI_NAME")"

cat >"$MANIFEST_PATH" <<EOF
{
  "schema_version": 1,
  "install_channel": "$(json_escape "$ALAN_CHANNEL_ID")",
  "package": "$(json_escape "$ALAN_APP_BUNDLE_NAME")",
  "bundle_identifier": "$(json_escape "$ALAN_BUNDLE_ID")",
  "version": "$(json_escape "$VERSION")",
  "git_revision": "$(json_escape "$REVISION")",
  "git_dirty": $DIRTY,
  "assembled_at_utc": "$(json_escape "$ASSEMBLED_AT")",
  "embedded_binaries": {
    "$(json_escape "$ALAN_CLI_NAME")": {
      "path": "Contents/Resources/bin/$(json_escape "$ALAN_CLI_NAME")",
      "sha256": "$(json_escape "$ALAN_SHA")"
    },
    "$(json_escape "$ALAN_TUI_NAME")": {
      "path": "Contents/Resources/bin/$(json_escape "$ALAN_TUI_NAME")",
      "sha256": "$(json_escape "$TUI_SHA")"
    }
  }
}
EOF

printf 'Signing app bundle...\n'
sign_path "$APP_BUNDLE"
codesign --verify --strict --verbose=2 "$APP_BUNDLE"

ZIP_PATH=""
if [[ "$CREATE_ARCHIVE" == "1" ]]; then
    ZIP_PATH="$ARTIFACT_DIR/alan-$VERSION-macos.zip"
    rm -f "$ZIP_PATH"
    ditto -c -k --keepParent "$APP_BUNDLE" "$ZIP_PATH"
    shasum -a 256 "$ZIP_PATH" >"$ZIP_PATH.sha256"
fi

if [[ "$NOTARIZE" == "1" ]]; then
    require_command xcrun
    if [[ -n "${ALAN_NOTARY_KEYCHAIN_PROFILE:-}" ]]; then
        "$SCRIPT_DIR/ensure-notary-profile.sh"
    fi

    if [[ -z "$ZIP_PATH" ]]; then
        ZIP_PATH="$ARTIFACT_DIR/alan-$VERSION-macos.zip"
        rm -f "$ZIP_PATH"
        ditto -c -k --keepParent "$APP_BUNDLE" "$ZIP_PATH"
    fi

    printf 'Submitting release archive for notarization...\n'
    if [[ -z "${ALAN_NOTARY_KEYCHAIN_PROFILE:-}" ]]; then
        fail "notarization requires ALAN_NOTARY_KEYCHAIN_PROFILE"
    fi
    xcrun notarytool submit "$ZIP_PATH" \
        --keychain-profile "$ALAN_NOTARY_KEYCHAIN_PROFILE" \
        --wait
    xcrun stapler staple "$APP_BUNDLE"
    xcrun stapler validate "$APP_BUNDLE"

    rm -f "$ZIP_PATH"
    ditto -c -k --keepParent "$APP_BUNDLE" "$ZIP_PATH"
    shasum -a 256 "$ZIP_PATH" >"$ZIP_PATH.sha256"
fi

printf '\nRelease app assembled:\n'
printf '  app: %s\n' "$APP_BUNDLE"
printf '  manifest: %s\n' "$MANIFEST_PATH"
if [[ -n "$ZIP_PATH" ]]; then
    printf '  archive: %s\n' "$ZIP_PATH"
    printf '  checksum: %s.sha256\n' "$ZIP_PATH"
fi
