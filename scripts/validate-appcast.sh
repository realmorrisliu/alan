#!/usr/bin/env bash
set -euo pipefail

APPCAST="${1:-}"

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

require_command() {
    command -v "$1" >/dev/null 2>&1 || fail "required command '$1' was not found"
}

xpath_string() {
    local expression="$1"
    xmllint --xpath "string($expression)" "$APPCAST" 2>/dev/null || true
}

file_size() {
    stat -f '%z' "$1" 2>/dev/null || stat -c '%s' "$1"
}

require_command xmllint
[[ -n "$APPCAST" ]] || fail "usage: validate-appcast.sh <appcast.xml>"
[[ -f "$APPCAST" ]] || fail "appcast not found: $APPCAST"

xmllint --noout "$APPCAST"

enclosure_url="$(xpath_string '(//*[local-name()="enclosure"]/@url)[1]')"
enclosure_length="$(xpath_string '(//*[local-name()="enclosure"]/@length)[1]')"
enclosure_type="$(xpath_string '(//*[local-name()="enclosure"]/@type)[1]')"
ed_signature="$(xpath_string '(//*[local-name()="enclosure"]/@*[local-name()="edSignature"])[1]')"
sparkle_version="$(xpath_string '(//*[local-name()="enclosure"]/@*[local-name()="version"])[1]')"
short_version="$(xpath_string '(//*[local-name()="enclosure"]/@*[local-name()="shortVersionString"])[1]')"
sha256="$(xpath_string '(//*[local-name()="sha256"])[1]')"

[[ -n "$enclosure_url" ]] || fail "appcast enclosure URL is missing"
[[ "$enclosure_url" == https://github.com/*/releases/download/v*/* ]] ||
    fail "appcast enclosure URL must point at a GitHub Release asset"
[[ "$enclosure_url" =~ /alan-[^/]+-macos\.zip$ ]] ||
    fail "appcast enclosure URL must point at alan-<version>-macos.zip"
[[ -n "$enclosure_length" ]] || fail "appcast enclosure length is missing"
[[ "$enclosure_type" == "application/octet-stream" ]] ||
    fail "appcast enclosure type must be application/octet-stream"
[[ -n "$ed_signature" ]] || fail "appcast enclosure is missing sparkle:edSignature"
[[ -n "$sparkle_version" ]] || fail "appcast enclosure is missing sparkle:version"
[[ -n "$short_version" ]] || fail "appcast enclosure is missing sparkle:shortVersionString"
[[ -n "$sha256" ]] || fail "appcast is missing sparkle:sha256"

if [[ -n "${ALAN_EXPECTED_VERSION:-}" && "$short_version" != "$ALAN_EXPECTED_VERSION" ]]; then
    fail "appcast short version $short_version does not match $ALAN_EXPECTED_VERSION"
fi
if [[ -n "${ALAN_EXPECTED_BUILD:-}" && "$sparkle_version" != "$ALAN_EXPECTED_BUILD" ]]; then
    fail "appcast build $sparkle_version does not match $ALAN_EXPECTED_BUILD"
fi
if [[ -n "${ALAN_EXPECTED_ARCHIVE_URL:-}" && "$enclosure_url" != "$ALAN_EXPECTED_ARCHIVE_URL" ]]; then
    fail "appcast enclosure URL $enclosure_url does not match $ALAN_EXPECTED_ARCHIVE_URL"
fi
if [[ -n "${ALAN_EXPECTED_ARCHIVE_PATH:-}" ]]; then
    [[ -f "$ALAN_EXPECTED_ARCHIVE_PATH" ]] ||
        fail "expected archive not found: $ALAN_EXPECTED_ARCHIVE_PATH"
    expected_length="$(file_size "$ALAN_EXPECTED_ARCHIVE_PATH")"
    expected_sha="$(shasum -a 256 "$ALAN_EXPECTED_ARCHIVE_PATH" | awk '{ print $1 }')"
    [[ "$enclosure_length" == "$expected_length" ]] ||
        fail "appcast length $enclosure_length does not match archive length $expected_length"
    [[ "$sha256" == "$expected_sha" ]] ||
        fail "appcast sha256 $sha256 does not match archive sha256 $expected_sha"
fi

printf 'Appcast validation passed: %s\n' "$APPCAST"
