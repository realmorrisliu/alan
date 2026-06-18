#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
BUILD_DIR="${TMPDIR:-/tmp}/alan-shell-sidebar-tab-row-tests"
MODULE_CACHE_DIR="${BUILD_DIR}/clang-module-cache"
TEST_BINARY="${BUILD_DIR}/shell-sidebar-tab-row-tests"

mkdir -p "$MODULE_CACHE_DIR"

CLANG_MODULE_CACHE_PATH="$MODULE_CACHE_DIR" swiftc \
    "$REPO_ROOT/clients/apple/alan-macos/Support/AlanCommandLineToolInstaller.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Support/AlanMacUpdatePolicy.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellValueTypes.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellSnapshots.swift" \
    "$REPO_ROOT/clients/apple/scripts/support/ShellTreeMutationParitySupport.swift" \
    "$REPO_ROOT/clients/apple/scripts/support/ShellStateMutationParitySupport.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/ShellModel.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellActionRegistry.swift" \
    "$REPO_ROOT/clients/apple/scripts/support/ShellActionRegistryParitySupport.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFIAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFIEnvelope.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFILoader.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFITerminalProfileAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFIManagedTerminalAccountAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/TerminalProfileStore.swift" \
    "$REPO_ROOT/clients/apple/scripts/test-shell-sidebar-tab-rows.swift" \
    -o "$TEST_BINARY"

"$TEST_BINARY"
