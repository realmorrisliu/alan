#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
BUILD_DIR="${TMPDIR:-/tmp}/alan-shell-sidebar-tab-row-tests"
MODULE_CACHE_DIR="${BUILD_DIR}/clang-module-cache"
TEST_BINARY="${BUILD_DIR}/shell-sidebar-tab-row-tests"

mkdir -p "$MODULE_CACHE_DIR"

cargo build -p alan-shell-core-ffi

CLANG_MODULE_CACHE_PATH="$MODULE_CACHE_DIR" swiftc \
    "$REPO_ROOT/clients/apple/alan-macos/Support/AlanCommandLineToolInstaller.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Support/AlanMacUpdatePolicy.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellValueTypes.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellPaneSnapshots.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellContentSnapshots.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellPaneTreeSnapshots.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellTabSpaceSnapshots.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellWorkspaceSnapshots.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellStateRuntimeSupport.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellTitlePresentation.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellSidebarTabPresentation.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellSidebarTabDragDrop.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellSidebarPaneTopology.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellActivityNotificationPresentation.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellActionRegistry.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFIAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFIActionAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFIEnvelope.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFILoader.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFIMaterialization.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFIReducerAdapter.swift" \
    "$REPO_ROOT/clients/apple/scripts/support/ShellCoreFFITestStateBuilder.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFITerminalProfileAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFIManagedTerminalAccountAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/TerminalProfileStore.swift" \
    "$REPO_ROOT/clients/apple/scripts/test-shell-sidebar-tab-rows.swift" \
    -L "$REPO_ROOT/target/debug" \
    -lalan_shell_core_ffi \
    -o "$TEST_BINARY"

ALAN_SHELL_CORE_FFI_LIBRARY="$REPO_ROOT/target/debug/libalan_shell_core_ffi.dylib" \
    DYLD_LIBRARY_PATH="$REPO_ROOT/target/debug:${DYLD_LIBRARY_PATH:-}" \
    "$TEST_BINARY"
