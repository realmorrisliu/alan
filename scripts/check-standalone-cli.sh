#!/usr/bin/env bash
set -euo pipefail

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

alan_binary="${1:?alan binary path is required}"
host_binary="${2:?stable Host binary path is required}"
dev_host_binary="${3:?development Host binary path is required}"

for binary in "$alan_binary" "$host_binary" "$dev_host_binary"; do
    [[ -x "$binary" ]] || fail "standalone binary is missing or not executable: $binary"
done

"$alan_binary" --version >/dev/null || fail "standalone CLI --version failed"
printf 'standalone CLI/Host distribution check passed\n'
