#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
BUILD_DIR="${TMPDIR:-/tmp}/alan-shell-performance-diagnostics-tests"
MODULE_CACHE_DIR="$BUILD_DIR/clang-module-cache"
TEST_BINARY="$BUILD_DIR/shell-performance-diagnostics-tests"

rm -rf "$BUILD_DIR"
mkdir -p "$MODULE_CACHE_DIR"

CLANG_MODULE_CACHE_PATH="$MODULE_CACHE_DIR" swiftc \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellPerformanceDiagnostics.swift" \
    "$REPO_ROOT/clients/apple/scripts/test-shell-performance-diagnostics.swift" \
    -o "$TEST_BINARY"

"$TEST_BINARY"
