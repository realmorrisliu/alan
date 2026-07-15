#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

for command in cargo rg openspec; do
    if ! command -v "$command" >/dev/null 2>&1; then
        printf 'error: repository quality gate requires %s on PATH\n' "$command" >&2
        exit 1
    fi
done

"$ROOT/scripts/check-rust-source-size.sh"
"$ROOT/scripts/check-rust-architecture.sh"
"$ROOT/scripts/check-rust-quality.sh"

cargo build -p alan --bin alan
"$ROOT/scripts/check-daemon-era-absence.sh" "$ROOT/target/debug/alan"
"$ROOT/scripts/check-workspace-runtime-absence.sh" "$ROOT" "$ROOT/target/debug/alan"
"$ROOT/scripts/check-legacy-macos-absence.sh"
bash "$ROOT/scripts/check-openspec-current-surfaces.sh"

apple_report="$(bash "$ROOT/clients/apple/scripts/check-architecture-maintainability.sh")"
printf '%s\n' "$apple_report" | tail -n 1

printf 'Repository quality gate passed.\n'
