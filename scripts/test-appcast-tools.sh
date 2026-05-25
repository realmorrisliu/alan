#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
WORK_DIR="${TMPDIR:-/tmp}/alan-appcast-tools-test"
ARCHIVE="$WORK_DIR/alan-0.1.0-macos.zip"
APPCAST="$WORK_DIR/appcast.xml"
SIGNED_APPCAST="$WORK_DIR/appcast-signed-tool.xml"
STALE_APPCAST="$WORK_DIR/appcast-stale.xml"

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

write_app_info_plist() {
    local plist="$1"
    local version="$2"
    local build="$3"

    cat >"$plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleShortVersionString</key>
    <string>$version</string>
    <key>CFBundleVersion</key>
    <string>$build</string>
    <key>SUFeedURL</key>
    <string>https://alanworks.app/appcast.xml</string>
</dict>
</plist>
EOF
}

rm -rf "$WORK_DIR"
mkdir -p "$WORK_DIR"
printf 'fake release archive\n' >"$WORK_DIR/Alan.app"
ditto -c -k "$WORK_DIR/Alan.app" "$ARCHIVE"

ALAN_RELEASE_VERSION=0.1.0 \
ALAN_RELEASE_BUILD=1 \
ALAN_RELEASE_ARCHIVE="$ARCHIVE" \
ALAN_RELEASE_ARCHIVE_URL="https://github.com/realmorrisliu/alan/releases/download/v0.1.0/alan-0.1.0-macos.zip" \
ALAN_SPARKLE_ED_SIGNATURE="dGVzdC1zcGFya2xlLWVkLXNpZ25hdHVyZQ==" \
ALAN_APPCAST_OUTPUT="$APPCAST" \
    "$SCRIPT_DIR/generate-appcast.sh"

ALAN_EXPECTED_VERSION=0.1.0 \
ALAN_EXPECTED_BUILD=1 \
ALAN_EXPECTED_ARCHIVE_URL="https://github.com/realmorrisliu/alan/releases/download/v0.1.0/alan-0.1.0-macos.zip" \
ALAN_EXPECTED_ARCHIVE_PATH="$ARCHIVE" \
    "$SCRIPT_DIR/validate-appcast.sh" "$APPCAST"

OLD_APP="$WORK_DIR/old/Alan.app"
NEW_APP="$WORK_DIR/new/Alan.app"
mkdir -p "$OLD_APP/Contents" "$NEW_APP/Contents"
write_app_info_plist "$OLD_APP/Contents/Info.plist" 0.0.9 0
write_app_info_plist "$NEW_APP/Contents/Info.plist" 0.1.0 1

ALAN_OLD_APP="$OLD_APP" \
ALAN_NEW_APP="$NEW_APP" \
ALAN_APPCAST_PATH="$APPCAST" \
    "$SCRIPT_DIR/smoke-macos-auto-update.sh" >/dev/null

ALAN_RELEASE_VERSION=0.1.0 \
ALAN_RELEASE_BUILD=2 \
ALAN_RELEASE_ARCHIVE="$ARCHIVE" \
ALAN_RELEASE_ARCHIVE_URL="https://github.com/realmorrisliu/alan/releases/download/v0.1.0/alan-0.1.0-macos.zip" \
ALAN_SPARKLE_ED_SIGNATURE="dGVzdC1zcGFya2xlLWVkLXNpZ25hdHVyZQ==" \
ALAN_APPCAST_OUTPUT="$STALE_APPCAST" \
    "$SCRIPT_DIR/generate-appcast.sh"

if ALAN_OLD_APP="$OLD_APP" \
    ALAN_NEW_APP="$NEW_APP" \
    ALAN_APPCAST_PATH="$STALE_APPCAST" \
    "$SCRIPT_DIR/smoke-macos-auto-update.sh" >/dev/null 2>&1; then
    fail "auto-update smoke must reject an appcast whose build does not match NEW_APP"
fi

FAKE_SIGN_UPDATE="$WORK_DIR/sign_update"
FAKE_PRIVATE_KEY="$WORK_DIR/sparkle_ed25519_private.pem"
printf 'fake-private-key\n' >"$FAKE_PRIVATE_KEY"
cat >"$FAKE_SIGN_UPDATE" <<'SCRIPT'
#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" != "--ed-key-file" ]]; then
    printf 'expected --ed-key-file, got %s\n' "${1:-}" >&2
    exit 2
fi
[[ -f "${2:-}" ]] || exit 3
[[ -f "${3:-}" ]] || exit 4
printf 'sparkle:edSignature="ZmFrZS1zcGFya2xlLXNpZ25hdHVyZQ==" length="123"\n'
SCRIPT
chmod +x "$FAKE_SIGN_UPDATE"

ALAN_RELEASE_VERSION=0.1.0 \
ALAN_RELEASE_BUILD=1 \
ALAN_RELEASE_ARCHIVE="$ARCHIVE" \
ALAN_RELEASE_ARCHIVE_URL="https://github.com/realmorrisliu/alan/releases/download/v0.1.0/alan-0.1.0-macos.zip" \
ALAN_SPARKLE_SIGN_UPDATE="$FAKE_SIGN_UPDATE" \
ALAN_SPARKLE_PRIVATE_KEY="$FAKE_PRIVATE_KEY" \
ALAN_APPCAST_OUTPUT="$SIGNED_APPCAST" \
    "$SCRIPT_DIR/generate-appcast.sh"

ALAN_EXPECTED_VERSION=0.1.0 \
ALAN_EXPECTED_BUILD=1 \
ALAN_EXPECTED_ARCHIVE_URL="https://github.com/realmorrisliu/alan/releases/download/v0.1.0/alan-0.1.0-macos.zip" \
ALAN_EXPECTED_ARCHIVE_PATH="$ARCHIVE" \
    "$SCRIPT_DIR/validate-appcast.sh" "$SIGNED_APPCAST"

ALAN_RELEASE_TAG=v0.1.0 \
ALAN_RELEASE_ARCHIVE="$ARCHIVE" \
ALAN_APPCAST_PATH="$APPCAST" \
ALAN_PREVIOUS_SPARKLE_VERSION=0 \
    "$SCRIPT_DIR/validate-release-version-metadata.sh"

printf 'appcast tool tests passed\n'
