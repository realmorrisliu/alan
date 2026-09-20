#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TEST_ROOT="$(mktemp -d "$PROJECT_ROOT/target/standalone-test.XXXXXX")"
trap 'rm -rf "$TEST_ROOT"' EXIT

export ALAN_STANDALONE_TARGET_DIR="$TEST_ROOT/target"
export ALAN_CLI_INSTALL_DIR="$TEST_ROOT/bin"
export ALAN_BUILD_PROFILE=release

"$SCRIPT_DIR/install-cli.sh"
"$TEST_ROOT/bin/alan" --version >/dev/null

ALAN_INSTALL_CHANNEL=dev "$SCRIPT_DIR/install-cli.sh"
"$TEST_ROOT/bin/alan-dev" --version >/dev/null

conflict="$TEST_ROOT/conflict"
mkdir -p "$conflict"
printf 'user-owned\n' >"$conflict/alan"
if ALAN_CLI_INSTALL_DIR="$conflict" ALAN_SKIP_BUILD=1 "$SCRIPT_DIR/install-cli.sh" >/dev/null 2>&1; then
    printf 'error: installer accepted an unrelated destination file\n' >&2
    exit 1
fi

ALAN_RELEASE_OUT_DIR="$TEST_ROOT/release" "$SCRIPT_DIR/assemble-cli-release.sh"
archive=("$TEST_ROOT"/release/*.tar.gz)
[[ -f "${archive[0]}" ]] || { printf 'error: archive was not created\n' >&2; exit 1; }
tar -tzf "${archive[0]}" | grep -qx './manifest.json'
tar -tzf "${archive[0]}" | grep -qx './alan-os-host-dev'
printf 'Standalone CLI/Host distribution checks passed.\n'
