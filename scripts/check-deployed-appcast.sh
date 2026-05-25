#!/usr/bin/env bash
set -euo pipefail

URL="${1:-https://alanworks.app/appcast.xml}"

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

headers="$(curl -fsSI "$URL")" || fail "could not fetch appcast headers from $URL"
content_type="$(printf '%s\n' "$headers" | awk 'BEGIN { IGNORECASE = 1 } /^content-type:/ { print; exit }')"
cache_control="$(printf '%s\n' "$headers" | awk 'BEGIN { IGNORECASE = 1 } /^cache-control:/ { print; exit }')"

printf '%s\n' "$content_type" | grep -Eiq 'xml|rss' ||
    fail "appcast must be served as XML; got: ${content_type:-missing content-type}"

if [[ -z "$cache_control" ]]; then
    fail "appcast response must include Cache-Control"
fi

printf '%s\n' "$cache_control" | grep -Eiq 'no-cache|max-age=0|must-revalidate' ||
    fail "appcast Cache-Control must allow quick release visibility; got: $cache_control"

if printf '%s\n' "$cache_control" | grep -Eiq 'max-age=[1-9][0-9]{3,}'; then
    fail "appcast Cache-Control appears too long-lived: $cache_control"
fi

printf 'Deployed appcast headers validation passed: %s\n' "$URL"
