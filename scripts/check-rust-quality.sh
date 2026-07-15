#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

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
