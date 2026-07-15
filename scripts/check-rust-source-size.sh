#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASELINE="$ROOT/scripts/rust-source-size-baseline.txt"
MAX_LINES=1000
inventory="$(mktemp)"
trap 'rm -f "$inventory"' EXIT

cd "$ROOT"
while IFS= read -r file; do
    lines="$(wc -l < "$file" | tr -d ' ')"
    printf '%s %s\n' "$file" "$lines"
done < <(rg --files crates -g '*.rs' | sort) > "$inventory"

awk -v max="$MAX_LINES" '
    NR == FNR {
        if ($0 ~ /^#/ || NF == 0) {
            next
        }
        limit[$1] = $2
        next
    }
    {
        path = $1
        lines = $2
        seen[path] = 1
        if (lines > max && !(path in limit)) {
            printf "error: %s is %d lines; new Rust files must be <= %d lines\n", path, lines, max > "/dev/stderr"
            failed = 1
        } else if (path in limit && lines != limit[path]) {
            printf "error: %s is %d lines but its ratchet is %d; update the baseline in the same change\n", path, lines, limit[path] > "/dev/stderr"
            failed = 1
        }
    }
    END {
        for (path in limit) {
            if (!(path in seen)) {
                printf "error: baseline entry %s no longer exists; remove it\n", path > "/dev/stderr"
                failed = 1
            }
        }
        exit failed
    }
' "$BASELINE" "$inventory"

printf 'Rust source-size ratchet passed (%s-line target).\n' "$MAX_LINES"
