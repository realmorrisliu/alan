#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# shellcheck source=scripts/release-env.sh
source "$SCRIPT_DIR/release-env.sh"

failures=0

fail_check() {
    printf 'error: %s\n' "$*" >&2
    failures=$((failures + 1))
}

ok_check() {
    printf 'ok: %s\n' "$*"
}

check_command() {
    local command="$1"
    if command -v "$command" >/dev/null 2>&1; then
        ok_check "$command is available"
    else
        fail_check "required command '$command' was not found"
    fi
}

check_signing_identity() {
    local identity="${ALAN_DEVELOPER_ID_APPLICATION:-${ALAN_SIGNING_IDENTITY:-}}"
    local identities

    if [[ -z "$identity" ]]; then
        fail_check "ALAN_DEVELOPER_ID_APPLICATION is not set"
        return
    fi

    if [[ "$identity" == *"Your Name"* || "$identity" == *"TEAMID"* ]]; then
        fail_check "ALAN_DEVELOPER_ID_APPLICATION still looks like the placeholder value"
        return
    fi

    identities="$(security find-identity -v -p codesigning 2>/dev/null || true)"
    if printf '%s\n' "$identities" | grep -F "$identity" >/dev/null; then
        ok_check "Developer ID signing identity is available in the keychain"
    else
        fail_check "Developer ID signing identity is configured but not available in the keychain"
    fi
}

printf 'Release env: %s\n' "${ALAN_RELEASE_ENV_FILE_RESOLVED:-none}"

for command in cargo xcodebuild codesign ditto shasum security xcrun; do
    check_command "$command"
done

check_signing_identity

if [[ "${ALAN_NOTARIZE:-0}" == "1" ]]; then
    if "$SCRIPT_DIR/ensure-notary-profile.sh"; then
        ok_check "notary keychain profile is ready"
    else
        fail_check "notary keychain profile is not ready"
    fi
else
    ok_check "notarization check skipped because ALAN_NOTARIZE is not 1"
fi

if [[ "$failures" -gt 0 ]]; then
    printf '\nRelease check failed with %d issue(s).\n' "$failures" >&2
    exit 1
fi

printf '\nRelease check passed.\n'
