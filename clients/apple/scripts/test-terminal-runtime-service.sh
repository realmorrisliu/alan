#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
BUILD_DIR="${TMPDIR:-/tmp}/alan-terminal-runtime-service-tests"
MODULE_CACHE_DIR="${BUILD_DIR}/clang-module-cache"
TEST_BINARY="${BUILD_DIR}/terminal-runtime-service-tests"

mkdir -p "$MODULE_CACHE_DIR"

cargo build -p alan-shell-core-ffi

clang -c \
    "$REPO_ROOT/clients/apple/alan-macos/AlanDarwinPtySpawn.c" \
    -o "$BUILD_DIR/AlanDarwinPtySpawn.o"

CLANG_MODULE_CACHE_PATH="$MODULE_CACHE_DIR" swiftc \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellWorkspaceValueTypes.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/TerminalProfileModels.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ManagedTerminalAccountModels.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/TerminalActivityModels.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellContextSnapshot.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ManagedTerminalAccountValidation.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/AlanPrivilegedHelperContracts.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ManagedTerminalAccountPlanning.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ManagedTerminalAccountEffects.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Terminal/TerminalAgentActivityAdapter.swift" \
    "$REPO_ROOT/clients/apple/scripts/support/AlanPrivilegedHelperFakeClient.swift" \
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
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/TerminalRuntimeSnapshots.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Terminal/TerminalBootResolution.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Terminal/TerminalRenderCoordinator.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Terminal/TerminalRuntimePublicationPolicy.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Terminal/TerminalNativeScrollViewAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Terminal/TerminalSemanticCommands.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Terminal/TerminalKeyboardRouting.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Terminal/TerminalPointerRouting.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Terminal/TerminalInputRouter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Terminal/TerminalScrollbackAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Terminal/TerminalSearchAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Terminal/TerminalSelectionClipboardAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Terminal/TerminalSurfaceState.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Terminal/TerminalMetadataAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Terminal/TerminalSurfaceController.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Terminal/TerminalRuntimeDelivery.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Terminal/TerminalPtyContracts.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Terminal/TerminalPtyControlSequenceResponder.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Terminal/TerminalPtyRuntime.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Terminal/ManagedUserTerminalPtyRuntime.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Terminal/DarwinTerminalPtyRuntime.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Terminal/GhosttyProcessBootstrap.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Terminal/TerminalSurfaceContracts.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Terminal/GhosttyTerminalSurfaceHandle.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Terminal/TerminalTranscriptCapture.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Terminal/WindowTerminalRuntimeService.swift" \
    "$REPO_ROOT/clients/apple/scripts/support/TerminalRuntimeTestDoubles.swift" \
    "$REPO_ROOT/clients/apple/scripts/test-terminal-runtime-service.swift" \
    "$BUILD_DIR/AlanDarwinPtySpawn.o" \
    -o "$TEST_BINARY"

ALAN_SHELL_CORE_FFI_LIBRARY="$REPO_ROOT/target/debug/libalan_shell_core_ffi.dylib" \
    "$TEST_BINARY"
