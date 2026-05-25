#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
BUILD_DIR="${TMPDIR:-/tmp}/alan-macos-auto-update-policy-tests"
MODULE_CACHE_DIR="$BUILD_DIR/module-cache"
TEST_BINARY="$BUILD_DIR/macos-auto-update-policy-tests"

rm -rf "$BUILD_DIR"
mkdir -p "$MODULE_CACHE_DIR"

CLANG_MODULE_CACHE_PATH="$MODULE_CACHE_DIR" swiftc \
    "$REPO_ROOT/clients/apple/alan-macos/Support/AlanCommandLineToolInstaller.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Support/AlanMacUpdatePolicy.swift" \
    "$REPO_ROOT/clients/apple/scripts/test-macos-auto-update-policy.swift" \
    -o "$TEST_BINARY"

"$TEST_BINARY"
