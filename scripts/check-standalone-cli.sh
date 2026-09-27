#!/usr/bin/env bash
set -euo pipefail

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

alan_binary="${1:?alan binary path is required}"

[[ -x "$alan_binary" ]] || fail "standalone CLI is missing or not executable: $alan_binary"

"$alan_binary" --version >/dev/null || fail "standalone CLI --version failed"
printf 'standalone CLI check passed\n'
