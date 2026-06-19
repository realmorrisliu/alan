#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
BUILD_DIR="${TMPDIR:-/tmp}/alan-shell-runtime-metadata-tests"
MODULE_CACHE_DIR="${BUILD_DIR}/clang-module-cache"
TEST_BINARY="${BUILD_DIR}/shell-runtime-metadata-tests"

mkdir -p "$MODULE_CACHE_DIR"

cargo build -p alan-shell-core-ffi

CLANG_MODULE_CACHE_PATH="$MODULE_CACHE_DIR" swiftc \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellValueTypes.swift" \
    "$REPO_ROOT/clients/apple/scripts/support/ManagedTerminalAccountTestSupport.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellSnapshots.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellControlPlaneDTOs.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellStateRuntimeSupport.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellAutomationCommand.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellActionRegistry.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellWorkspaceManifest.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Support/ShellSidebarSpaceSliderLayout.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/ShellModel.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellControlFilePoller.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellActionCoordinator.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellClipboardWriter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellContentRenderingRegistry.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFIAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFIActionAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFIControlAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFIEnvelope.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFILoader.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFIManifestAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFIMaterialization.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFIReducerAdapter.swift" \
    "$REPO_ROOT/clients/apple/scripts/support/ShellCoreFFITestStateBuilder.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFISettingsAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFITerminalProfileAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFIManagedTerminalAccountAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/TerminalProfileStore.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellDiagnostics.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellEventStore.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellLocalCommandExecutor.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellPlatformMetadataPreserver.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellPerformanceDiagnostics.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellPublishedStateMerger.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellReducerCommandCoordinator.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellSocketServer.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/ShellControlPlane.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellPaneProjectionService.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellPersistenceWriter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellStatePersistenceStore.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellWorkspaceManifestStartupCoordinator.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellWorkspacePersistenceCoordinator.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellWorkspaceManifestStore.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Support/AlanCommandLineToolInstaller.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Support/AlanMacUpdatePolicy.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellSettingsSurfaceModel.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/TerminalHostRuntime.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Terminal/TerminalContentLifecycleAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Terminal/TerminalContentProjectionAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Terminal/TerminalNativeScrollViewAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/TerminalSurfaceController.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/TerminalRuntimeService.swift" \
    "$REPO_ROOT/clients/apple/scripts/test-shell-runtime-metadata-host-stubs.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/TerminalRuntimeRegistry.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/ShellHostController.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Controllers/Shell/ShellHostControlCommandHandling.swift" \
    "$REPO_ROOT/clients/apple/scripts/test-shell-runtime-metadata.swift" \
    -o "$TEST_BINARY"

ALAN_SHELL_CORE_FFI_LIBRARY="$REPO_ROOT/target/debug/libalan_shell_core_ffi.dylib" \
    "$TEST_BINARY"
