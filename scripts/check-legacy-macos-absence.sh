#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# Ignore local build artifacts, but reject any versioned desktop source.
while IFS= read -r path; do
    if [[ -f "$path" || -L "$path" ]]; then
        printf 'error: retired desktop source exists: %s\n' "$path" >&2
        exit 1
    fi
done < <(git ls-files --cached --others --exclude-standard -- clients/apple crates/shell-core crates/shell-core-ffi)

if rg -n 'alan-shell-core|crates/shell-core' Cargo.toml crates --glob 'Cargo.toml'; then
    printf 'error: retired desktop crate remains in Cargo graph\n' >&2
    exit 1
fi

printf 'legacy macOS absence guard passed\n'
