#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASELINE="$ROOT/scripts/rust-source-size-baseline.txt"
BASELINE_REL="scripts/rust-source-size-baseline.txt"
MAX_LINES=1000
inventory="$(mktemp)"
base_baseline="$(mktemp)"
trap 'rm -f "$inventory" "$base_baseline"' EXIT

git_command=(git)
if [[ -n "${ALAN_QUALITY_GIT_DIR:-}" ]]; then
    git_command+=(--git-dir="$ALAN_QUALITY_GIT_DIR")
fi
base_ref="${ALAN_QUALITY_BASE_REF:-HEAD}"

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
        if (path in limit && limit[path] <= max) {
            printf "error: %s has a stale <= %d-line debt limit; remove or correct it\n", path, max > "/dev/stderr"
            failed = 1
        } else if (path in limit && lines <= max) {
            printf "error: %s is %d lines; remove its debt entry now that it is <= %d\n", path, lines, max > "/dev/stderr"
            failed = 1
        } else if (lines > max && !(path in limit)) {
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

if ! "${git_command[@]}" cat-file -e "$base_ref^{commit}" 2>/dev/null; then
    printf 'error: Rust source-size ratchet base is not a commit: %s\n' "$base_ref" >&2
    exit 1
fi

if "${git_command[@]}" cat-file -e "$base_ref:$BASELINE_REL" 2>/dev/null; then
    "${git_command[@]}" show "$base_ref:$BASELINE_REL" >"$base_baseline"
    awk '
        NR == FNR {
            if ($0 ~ /^#/ || NF == 0) {
                next
            }
            previous[$1] = $2
            next
        }
        $0 ~ /^#/ || NF == 0 { next }
        !($1 in previous) {
            printf "error: %s is a new Rust source-size debt entry; split it to <= 1000 lines\n", $1 > "/dev/stderr"
            failed = 1
            next
        }
        $2 > previous[$1] {
            printf "error: %s source-size debt grew from %d to %d lines\n", $1, previous[$1], $2 > "/dev/stderr"
            failed = 1
        }
        END { exit failed }
    ' "$base_baseline" "$BASELINE"
else
    printf 'Rust source-size ratchet baseline established relative to %s.\n' "$base_ref"
fi

printf 'Rust source-size ratchet passed (%s-line target).\n' "$MAX_LINES"
