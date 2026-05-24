#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
BUILD_DIR="${TMPDIR:-/tmp}/alan-shell-automation-command-seams-tests"
MODULE_CACHE_DIR="${BUILD_DIR}/clang-module-cache"
TEST_BINARY="${BUILD_DIR}/shell-automation-command-seams-tests"

mkdir -p "$MODULE_CACHE_DIR"

CLANG_MODULE_CACHE_PATH="$MODULE_CACHE_DIR" swiftc \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellValueTypes.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellSnapshots.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellTreeMutations.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellStateMutations.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellAutomationCommand.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellAutomationEntities.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellAutomationIntents.swift" \
    "$REPO_ROOT/clients/apple/scripts/shell-automation-test-fixtures.swift" \
    "$REPO_ROOT/clients/apple/scripts/test-shell-automation-command-seams.swift" \
    -o "$TEST_BINARY"

"$TEST_BINARY"
