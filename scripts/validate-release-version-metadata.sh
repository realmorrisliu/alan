#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PROJECT_FILE="$REPO_ROOT/clients/apple/alan-macos.xcodeproj/project.pbxproj"

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

single_xcode_value() {
    local key="$1"
    awk -F '= ' -v key="$key" '$1 ~ key {
        value = $2
        gsub(/[;[:space:]]/, "", value)
        print value
    }' "$PROJECT_FILE" | sort -u
}

numeric_gt() {
    awk -v lhs="$1" -v rhs="$2" 'BEGIN { exit !(lhs + 0 > rhs + 0) }'
}

cargo_version="$(awk -F '"' '/^version = / { print $2; exit }' "$REPO_ROOT/Cargo.toml")"
marketing_versions="$(single_xcode_value MARKETING_VERSION)"
project_versions="$(single_xcode_value CURRENT_PROJECT_VERSION)"

[[ -n "$cargo_version" ]] || fail "Cargo.toml workspace version could not be resolved"
[[ "$(printf '%s\n' "$marketing_versions" | sed '/^$/d' | wc -l | tr -d '[:space:]')" == "1" ]] ||
    fail "Xcode MARKETING_VERSION must have exactly one value"
[[ "$(printf '%s\n' "$project_versions" | sed '/^$/d' | wc -l | tr -d '[:space:]')" == "1" ]] ||
    fail "Xcode CURRENT_PROJECT_VERSION must have exactly one value"

marketing_version="$(printf '%s\n' "$marketing_versions" | sed '/^$/d' | head -n 1)"
project_version="$(printf '%s\n' "$project_versions" | sed '/^$/d' | head -n 1)"
[[ "$marketing_version" == "$cargo_version" ]] ||
    fail "Xcode MARKETING_VERSION $marketing_version does not match Cargo.toml $cargo_version"

release_tag="${ALAN_RELEASE_TAG:-v$cargo_version}"
[[ "$release_tag" == "v$cargo_version" ]] ||
    fail "release tag $release_tag does not match v$cargo_version"

archive="${ALAN_RELEASE_ARCHIVE:-$REPO_ROOT/target/release-artifacts/alan-$cargo_version-macos.zip}"
archive_name="$(basename "$archive")"
[[ "$archive_name" == "alan-$cargo_version-macos.zip" ]] ||
    fail "release archive name $archive_name does not match alan-$cargo_version-macos.zip"

appcast="${ALAN_APPCAST_PATH:-}"
if [[ -z "$appcast" && -f "$REPO_ROOT/target/release-artifacts/appcast.xml" ]]; then
    appcast="$REPO_ROOT/target/release-artifacts/appcast.xml"
fi

if [[ -n "$appcast" ]]; then
    ALAN_EXPECTED_VERSION="$cargo_version" \
    ALAN_EXPECTED_BUILD="$project_version" \
    ALAN_EXPECTED_ARCHIVE_URL="${ALAN_RELEASE_ARCHIVE_URL:-https://github.com/realmorrisliu/alan/releases/download/v$cargo_version/alan-$cargo_version-macos.zip}" \
        "$SCRIPT_DIR/validate-appcast.sh" "$appcast" >/dev/null
fi

previous="${ALAN_PREVIOUS_SPARKLE_VERSION:-}"
if [[ -z "$previous" && -n "${ALAN_PREVIOUS_APPCAST:-}" ]]; then
    previous="$(xmllint --xpath 'string((//*[local-name()="enclosure"]/@*[local-name()="version"])[1])' "$ALAN_PREVIOUS_APPCAST" 2>/dev/null || true)"
fi

if [[ -n "$previous" ]]; then
    numeric_gt "$project_version" "$previous" ||
        fail "CURRENT_PROJECT_VERSION $project_version must be greater than previous Sparkle version $previous"
else
    printf 'warning: previous Sparkle version not provided; monotonic check skipped\n' >&2
fi

printf 'Release version metadata validation passed: %s (%s)\n' "$cargo_version" "$project_version"
