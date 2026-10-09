#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

source "$ROOT/scripts/cargo-cli-output.sh"
if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    alan_cli_build_lease "$ROOT" "$(alan_cli_target_dir "$ROOT")" checkout "$ROOT/scripts/check-rust-quality.sh"
fi

cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- \
    -D warnings \
    -D clippy::allow_attributes_without_reason \
    -D clippy::undocumented_unsafe_blocks \
    -D clippy::redundant_clone
cargo clippy --locked --workspace --lib --bins --all-features -- \
    -D warnings \
    -D clippy::dbg_macro \
    -D clippy::todo \
    -D clippy::unimplemented
RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --all-features --no-deps \
    --document-private-items
