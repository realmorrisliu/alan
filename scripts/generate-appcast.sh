#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PROJECT_FILE="$REPO_ROOT/clients/apple/alan-macos.xcodeproj/project.pbxproj"

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

file_size() {
    stat -f '%z' "$1" 2>/dev/null || stat -c '%s' "$1"
}

xml_escape() {
    sed \
        -e 's/&/\&amp;/g' \
        -e 's/</\&lt;/g' \
        -e 's/>/\&gt;/g' \
        -e 's/"/\&quot;/g' \
        -e "s/'/\&apos;/g"
}

single_xcode_value() {
    local key="$1"
    awk -F '= ' -v key="$key" '$1 ~ key {
        value = $2
        gsub(/[;[:space:]]/, "", value)
        print value
    }' "$PROJECT_FILE" | sort -u
}

version="${ALAN_RELEASE_VERSION:-$(awk -F '"' '/^version = / { print $2; exit }' "$REPO_ROOT/Cargo.toml")}"
build="${ALAN_RELEASE_BUILD:-$(single_xcode_value CURRENT_PROJECT_VERSION | head -n 1)}"
release_tag="${ALAN_RELEASE_TAG:-v$version}"
archive="${ALAN_RELEASE_ARCHIVE:-$REPO_ROOT/target/release-artifacts/alan-$version-macos.zip}"
archive_url="${ALAN_RELEASE_ARCHIVE_URL:-https://github.com/realmorrisliu/alan/releases/download/$release_tag/alan-$version-macos.zip}"
output="${ALAN_APPCAST_OUTPUT:-$REPO_ROOT/target/release-artifacts/appcast.xml}"
pub_date="${ALAN_APPCAST_PUB_DATE:-$(date -u '+%a, %d %b %Y %H:%M:%S +0000')}"
title="${ALAN_APPCAST_ITEM_TITLE:-Alan $version}"
description="${ALAN_APPCAST_ITEM_DESCRIPTION:-Alan for macOS $version}"

[[ -n "$version" ]] || fail "release version could not be resolved"
[[ -n "$build" ]] || fail "release build could not be resolved"
[[ -f "$archive" ]] || fail "release archive not found: $archive"

signature="${ALAN_SPARKLE_ED_SIGNATURE:-}"
if [[ -z "$signature" ]]; then
    sign_update="${ALAN_SPARKLE_SIGN_UPDATE:-}"
    private_key="${ALAN_SPARKLE_PRIVATE_KEY:-$REPO_ROOT/release-secrets/sparkle_ed25519_private.pem}"
    if [[ -z "$sign_update" ]]; then
        derived_data="${ALAN_XCODE_DERIVED_DATA:-$REPO_ROOT/target/xcode-derived}"
        for candidate in \
            "$derived_data/SourcePackages/artifacts/sparkle/Sparkle/bin/sign_update" \
            "$REPO_ROOT/target/xcode-derived/SourcePackages/artifacts/sparkle/Sparkle/bin/sign_update" \
            "$REPO_ROOT/target/xcode-derived-auto-update/SourcePackages/artifacts/sparkle/Sparkle/bin/sign_update"
        do
            if [[ -x "$candidate" ]]; then
                sign_update="$candidate"
                break
            fi
        done
    fi
    [[ -x "$sign_update" ]] ||
        fail "set ALAN_SPARKLE_ED_SIGNATURE or ALAN_SPARKLE_SIGN_UPDATE to a Sparkle EdDSA signing tool"
    [[ -f "$private_key" ]] || fail "Sparkle private key not found: $private_key"

    sign_output="$("$sign_update" --ed-key-file "$private_key" "$archive")"
    signature="$(printf '%s\n' "$sign_output" | sed -nE 's/.*sparkle:edSignature="([^"]+)".*/\1/p' | head -n 1)"
    if [[ -z "$signature" ]]; then
        signature="$(printf '%s\n' "$sign_output" | tail -n 1 | tr -d '[:space:]')"
    fi
fi
[[ -n "$signature" ]] || fail "Sparkle EdDSA signature is empty"

archive_length="$(file_size "$archive")"
archive_sha256="$(shasum -a 256 "$archive" | awk '{ print $1 }')"
mkdir -p "$(dirname "$output")"

escaped_title="$(printf '%s' "$title" | xml_escape)"
escaped_description="$(printf '%s' "$description" | xml_escape)"
escaped_url="$(printf '%s' "$archive_url" | xml_escape)"
escaped_signature="$(printf '%s' "$signature" | xml_escape)"

cat >"$output" <<EOF
<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0"
     xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle"
     xmlns:dc="http://purl.org/dc/elements/1.1/">
  <channel>
    <title>Alan for macOS Updates</title>
    <link>https://alanworks.app/appcast.xml</link>
    <description>Stable Alan for macOS releases.</description>
    <item>
      <title>$escaped_title</title>
      <sparkle:releaseNotesLink>https://github.com/realmorrisliu/alan/releases/tag/$release_tag</sparkle:releaseNotesLink>
      <pubDate>$pub_date</pubDate>
      <description>$escaped_description</description>
      <sparkle:minimumSystemVersion>14.0</sparkle:minimumSystemVersion>
      <sparkle:shortVersionString>$version</sparkle:shortVersionString>
      <sparkle:version>$build</sparkle:version>
      <sparkle:sha256>$archive_sha256</sparkle:sha256>
      <enclosure
        url="$escaped_url"
        sparkle:version="$build"
        sparkle:shortVersionString="$version"
        sparkle:edSignature="$escaped_signature"
        length="$archive_length"
        type="application/octet-stream" />
    </item>
  </channel>
</rss>
EOF

ALAN_EXPECTED_VERSION="$version" \
ALAN_EXPECTED_BUILD="$build" \
ALAN_EXPECTED_ARCHIVE_URL="$archive_url" \
ALAN_EXPECTED_ARCHIVE_PATH="$archive" \
    "$SCRIPT_DIR/validate-appcast.sh" "$output" >/dev/null

printf 'Generated appcast: %s\n' "$output"
