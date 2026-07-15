#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

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
