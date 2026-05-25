#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
WORK_DIR="${TMPDIR:-/tmp}/alan-appcast-tools-test"
ARCHIVE="$WORK_DIR/alan-0.1.0-macos.zip"
APPCAST="$WORK_DIR/appcast.xml"
SIGNED_APPCAST="$WORK_DIR/appcast-signed-tool.xml"

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
