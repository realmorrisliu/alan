#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

for command in cargo git openspec python3 rg rustc; do
    if ! command -v "$command" >/dev/null 2>&1; then
        printf 'error: repository quality gate requires %s on PATH\n' "$command" >&2
        exit 1
    fi
done

host_target="$(rustc -vV | awk '/^host: / { print $2 }')"
if [[ -z "$host_target" ]]; then
    printf 'error: could not resolve the host Rust target\n' >&2
    exit 1
fi
quality_target_dir="${ALAN_QUALITY_TARGET_DIR:-$ROOT/target/quality-gate}"
alan_binary="$quality_target_dir/$host_target/debug/alan"
export CARGO_BUILD_TARGET="$host_target"
export CARGO_TARGET_DIR="$quality_target_dir"

"$ROOT/scripts/check-rust-source-size.sh"
"$ROOT/scripts/check-rust-architecture.sh"
"$ROOT/scripts/check-rust-quality.sh"
python3 "$ROOT/scripts/test_cargo_cli_output.py"

cargo build --locked -p alan --bin alan
"$ROOT/scripts/check-host-source-boundaries.sh"
bash "$ROOT/scripts/check-openspec-current-surfaces.sh"
"$ROOT/scripts/check-standalone-cli.sh" "$alan_binary"
ALAN_STANDALONE_TARGET_DIR="$quality_target_dir" \
ALAN_CLI_SOURCE="$alan_binary" \
ALAN_BUILD_PROFILE=debug \
ALAN_SKIP_BUILD=1 \
    "$ROOT/scripts/test-standalone-cli-distribution.sh"

printf 'Repository quality gate passed.\n'
