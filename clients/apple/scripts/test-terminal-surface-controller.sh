#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
BUILD_DIR="${TMPDIR:-/tmp}/alan-terminal-surface-controller-tests"
MODULE_CACHE_DIR="${BUILD_DIR}/clang-module-cache"
TEST_BINARY="${BUILD_DIR}/terminal-surface-controller-tests"

mkdir -p "$MODULE_CACHE_DIR"

cargo build -p alan-shell-core-ffi

clang -c \
    "$REPO_ROOT/clients/apple/alan-macos/AlanDarwinPtySpawn.c" \
    -o "$BUILD_DIR/AlanDarwinPtySpawn.o"

CLANG_MODULE_CACHE_PATH="$MODULE_CACHE_DIR" swiftc \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellValueTypes.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellPaneSnapshots.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellContentSnapshots.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellPaneTreeSnapshots.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellTabSpaceSnapshots.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellWorkspaceSnapshots.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellControlPlaneDTOs.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellStateRuntimeSupport.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellActionRegistry.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellWorkspaceManifest.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellSettingsSurfaceModel.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/TerminalSettingsSummaries.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ManagedTerminalAccountSettingsSummary.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ManagedTerminalAccountCatalog.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ManagedTerminalUserSettings.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellSettingsHostSummaries.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/AlanPrivilegedHelperXPC.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/AlanPrivilegedHelperService.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Support/ShellSidebarSpaceSliderLayout.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellTitlePresentation.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellSidebarTabPresentation.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellSidebarTabDragDrop.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellSidebarPaneTopology.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellActivityNotificationPresentation.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellControlFilePoller.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFIAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFIActionAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFIControlAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFIEnvelope.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFILoader.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFIManifestAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFIMaterialization.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFIReducerAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFISettingsAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFITerminalProfileAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFIManagedTerminalAccountAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/TerminalProfileStore.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellDiagnostics.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellEventStore.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellLocalCommandExecutor.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellActionCoordinator.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellReducerCommandCoordinator.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellPerformanceDiagnostics.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellPublishedStateMerger.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellSocketServer.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/AlanPrivilegedHelperAppClient.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/ShellControlPlane.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Support/AlanCommandLineToolInstaller.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Support/AlanMacUpdatePolicy.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/TerminalHostRuntime.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Terminal/TerminalNativeScrollViewAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/TerminalRuntimeService.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/TerminalSurfaceController.swift" \
    "$REPO_ROOT/clients/apple/scripts/test-terminal-surface-controller.swift" \
    "$BUILD_DIR/AlanDarwinPtySpawn.o" \
    -o "$TEST_BINARY"

ALAN_SHELL_CORE_FFI_LIBRARY="$REPO_ROOT/target/debug/libalan_shell_core_ffi.dylib" \
    "$TEST_BINARY"
