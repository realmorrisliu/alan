#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
SETUP_SCRIPT="$SCRIPT_DIR/setup-local-ghosttykit.sh"

header="$(find -L "$REPO_ROOT/clients/apple/GhosttyKit.xcframework" \
    -path '*/Headers/ghostty.h' \
    -type f \
    -print \
    -quit)"

if [ -z "$header" ]; then
    printf 'error: unsupported Ghostty external-PTY attachment seam: linked GhosttyKit headers were not found.\n' >&2
    printf 'hint: run %s to prepare GhosttyKit artifacts from the pinned Alan fork.\n' "$SETUP_SCRIPT" >&2
    exit 1
fi

for symbol in external_pty_read_fd external_pty_write_fd external_pty_close_fds; do
    if ! grep -q "$symbol" "$header"; then
        printf 'error: unsupported Ghostty external-PTY attachment seam: missing %s in %s.\n' "$symbol" "$header" >&2
        printf 'hint: rebuild GhosttyKit from the pinned Alan fork revision; Alan must not fall back to Ghostty-owned launch.\n' >&2
        exit 1
    fi
done

printf 'ok: Ghostty external-PTY attachment seam is available in %s\n' "$header"
