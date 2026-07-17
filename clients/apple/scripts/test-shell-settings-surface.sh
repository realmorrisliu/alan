#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
BUILD_DIR="${TMPDIR:-/tmp}/alan-shell-settings-surface-tests"
MODULE_CACHE_DIR="$BUILD_DIR/clang-module-cache"
TEST_BINARY="$BUILD_DIR/shell-settings-surface-tests"

rm -rf "$BUILD_DIR"
mkdir -p "$MODULE_CACHE_DIR"

cargo build -p alan-shell-core-ffi

SHELL_SETTINGS_VIEW_FILES=(
    "$REPO_ROOT/clients/apple/alan-macos/Views/Shell/Settings/ShellSettingsContentView.swift"
    "$REPO_ROOT/clients/apple/alan-macos/Views/Shell/Settings/ShellSettingsNavigationView.swift"
    "$REPO_ROOT/clients/apple/alan-macos/Views/Shell/Settings/ShellSettingsComponents.swift"
)
SHELL_SETTINGS_MODEL_FILES=(
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellSettingsSurfaceModel.swift"
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/TerminalSettingsSummaries.swift"
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ManagedTerminalAccountSettingsSummary.swift"
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ManagedTerminalAccountCatalog.swift"
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ManagedTerminalUserSettings.swift"
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellSettingsHostSummaries.swift"
)
if grep -q 'TextField("Mac user"' "${SHELL_SETTINGS_VIEW_FILES[@]}"; then
    echo "Managed User creation form must not expose Mac user as a primary input." >&2
    exit 1
fi
if grep -q 'ManagedTerminalUserProvisioningFlow.applyApproved' "${SHELL_SETTINGS_VIEW_FILES[@]}"; then
    echo "Managed User Settings actions must not run privileged apply synchronously on the UI path." >&2
    exit 1
fi
if grep -q 'guard !Task.isCancelled else { return }' "${SHELL_SETTINGS_VIEW_FILES[@]}"; then
    echo "Managed User apply tasks must clear applying state even if the Swift task is cancelled." >&2
    exit 1
fi
if ! grep -q 'defer { managedUserApplyInFlight = false }' "${SHELL_SETTINGS_VIEW_FILES[@]}"; then
    echo "Managed User apply tasks must use deferred UI cleanup so spinners cannot leak." >&2
    exit 1
fi
if ! grep -q 'Managed User apply timed out' "${SHELL_SETTINGS_VIEW_FILES[@]}"; then
    echo "Managed User apply tasks must surface a timeout instead of spinning indefinitely." >&2
    exit 1
fi
if grep -Fq 'managedUserApplyTimeoutNanoseconds: UInt64 = 90 * 1_000_000_000' \
    "${SHELL_SETTINGS_VIEW_FILES[@]}"
then
    echo "Managed User apply timeout must allow enough time for macOS administrator approval." >&2
    exit 1
fi
if ! grep -Fq '10 * 60 * 1_000_000_000' "${SHELL_SETTINGS_VIEW_FILES[@]}"; then
    echo "Managed User apply timeout must be a documented 10 minute administrator approval budget." >&2
    exit 1
fi
if grep -Eq 'ManagedTerminalAccountLocalAccountNameDiscoverer|localAccountNames|dscl \. -list /Users' \
    "${SHELL_SETTINGS_MODEL_FILES[@]}"
then
    echo "Managed Users must be sourced from Alan catalog/profile state, not arbitrary local user scans." >&2
    exit 1
fi

clang -c \
    "$REPO_ROOT/clients/apple/alan-macos/AlanPrivilegedHelperPtySpawn.c" \
    -o "$BUILD_DIR/AlanPrivilegedHelperPtySpawn.o"

CLANG_MODULE_CACHE_PATH="$MODULE_CACHE_DIR" swiftc \
    "$REPO_ROOT/clients/apple/alan-macos/Support/AlanCommandLineToolInstaller.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Support/AlanMacUpdatePolicy.swift" \
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
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellSettingsSurfaceModel.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/TerminalSettingsSummaries.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ManagedTerminalAccountSettingsSummary.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ManagedTerminalAccountCatalog.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ManagedTerminalUserSettings.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/Shell/ShellSettingsHostSummaries.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/AlanPrivilegedHelperXPC.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/AlanPrivilegedHelperXPCRequirementChecker.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/AlanPrivilegedHelperXPCClient.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/AlanPrivilegedHelperXPCListener.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/AlanPrivilegedHelperXPCService.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/AlanPrivilegedHelperManagedUserWire.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/AlanPrivilegedHelperManagedUserService.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/AlanPrivilegedHelperPTYSessionStore.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/AlanPrivilegedHelperPTYSupport.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/AlanPrivilegedHelperService.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellLocalFolderOpener.swift" \
    "$REPO_ROOT/clients/apple/scripts/support/AlanPrivilegedHelperFakeRequirementChecker.swift" \
    "$REPO_ROOT/clients/apple/scripts/support/ManagedTerminalAccountTestSupport.swift" \
    "$REPO_ROOT/clients/apple/scripts/test-shell-settings-surface.swift" \
    "$BUILD_DIR/AlanPrivilegedHelperPtySpawn.o" \
    -o "$TEST_BINARY"

ALAN_SHELL_CORE_FFI_LIBRARY="$REPO_ROOT/target/debug/libalan_shell_core_ffi.dylib" \
    DYLD_LIBRARY_PATH="$REPO_ROOT/target/debug:${DYLD_LIBRARY_PATH:-}" \
    "$TEST_BINARY"
