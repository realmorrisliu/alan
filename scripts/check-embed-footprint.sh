#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if ! command -v cargo >/dev/null 2>&1; then
    printf 'error: cargo is required on PATH\n' >&2
    exit 1
fi

cleanup_target_dir=""
if [[ -z "${CARGO_TARGET_DIR:-}" ]]; then
    cleanup_target_dir="$(mktemp -d "${TMPDIR:-/tmp}/alan-embed-footprint.XXXXXX")"
    export CARGO_TARGET_DIR="$cleanup_target_dir"
fi
trap 'if [[ -n "$cleanup_target_dir" ]]; then rm -rf "$cleanup_target_dir"; fi' EXIT

assert_absent() {
    local label="$1"
    local tree_output="$2"
    shift 2
    local crate
    for crate in "$@"; do
        if printf '%s\n' "$tree_output" | grep -Eq "^${crate}([[:space:]]|$)"; then
            printf 'error: %s unexpectedly depends on forbidden crate `%s`\n' "$label" "$crate" >&2
            exit 1
        fi
    done
}

assert_present() {
    local label="$1"
    local tree_output="$2"
    local crate="$3"
    if ! printf '%s\n' "$tree_output" | grep -Eq "^${crate}([[:space:]]|$)"; then
        printf 'error: %s is missing expected crate `%s`\n' "$label" "$crate" >&2
        exit 1
    fi
}

minimal_tree="$(
    cargo tree --locked -p alan-os-host --no-default-features \
        --edges normal --prefix none
)"
assert_absent \
    "alan-os-host (--no-default-features)" \
    "$minimal_tree" \
    alan-terminal-ui ratatui crossterm tree-sitter tree-sitter-bash tokio-tungstenite openrouter-rs

openrouter_tree="$(
    cargo tree --locked -p alan-os-host --no-default-features --features provider-openrouter \
        --edges normal --prefix none
)"
assert_present \
    "alan-os-host (--no-default-features --features provider-openrouter)" \
    "$openrouter_tree" \
    openrouter-rs
assert_absent \
    "alan-os-host (--no-default-features --features provider-openrouter)" \
    "$openrouter_tree" \
    alan-terminal-ui ratatui crossterm tree-sitter tree-sitter-bash tokio-tungstenite

first_party_tree="$(
    cargo tree --locked -p alan-os-host --no-default-features \
        --features provider-openrouter,provider-openai,provider-magpie,provider-vertex \
        --edges normal --prefix none
)"
assert_absent \
    "alan-os-host (first-party providers only)" \
    "$first_party_tree" \
    alan-terminal-ui ratatui crossterm tree-sitter tree-sitter-bash

cargo check --locked -p alan-os-host --no-default-features
cargo check --locked -p alan-os-host --no-default-features --features provider-openrouter
cargo check --locked -p alan-os-host --no-default-features \
    --features provider-openrouter,provider-openai,provider-magpie,provider-vertex

printf 'Embed footprint check passed.\n'
