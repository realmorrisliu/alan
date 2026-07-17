#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
BUILD_DIR="${TMPDIR:-/tmp}/alan-privileged-helper-live-smoke"
MODULE_CACHE_DIR="$BUILD_DIR/clang-module-cache"
TEST_BINARY="$BUILD_DIR/privileged-helper-live-smoke"
COMPILE_ONLY=0
APP_PATH="${ALAN_PRIVILEGED_HELPER_SMOKE_APP_PATH:-$HOME/Applications/Alan Dev.app}"
APP_EXECUTABLE="${APP_PATH}/Contents/MacOS/Alan Dev"

if [[ "${1:-}" == "--compile-only" ]]; then
    COMPILE_ONLY=1
    shift
fi

rm -rf "$BUILD_DIR"
mkdir -p "$MODULE_CACHE_DIR"

clang -c \
    "$REPO_ROOT/clients/apple/alan-macos/AlanPrivilegedHelperPtySpawn.c" \
    -o "$BUILD_DIR/AlanPrivilegedHelperPtySpawn.o"

CLANG_MODULE_CACHE_PATH="$MODULE_CACHE_DIR" swiftc \
    "$REPO_ROOT/clients/apple/alan-macos/Support/AlanCommandLineToolInstaller.swift" \
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
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFIAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFIEnvelope.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFILoader.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFITerminalProfileAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/ShellCoreFFIManagedTerminalAccountAdapter.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/Shell/TerminalProfileStore.swift" \
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
    "$REPO_ROOT/clients/apple/scripts/smoke-privileged-helper-live.swift" \
    "$BUILD_DIR/AlanPrivilegedHelperPtySpawn.o" \
    -o "$TEST_BINARY"

if [[ "$COMPILE_ONLY" -eq 1 ]]; then
    printf 'Privileged helper live smoke harness compiled.\n'
    exit 0
fi

export ALAN_INSTALL_CHANNEL="${ALAN_INSTALL_CHANNEL:-dev}"

if [[ ! -x "$APP_EXECUTABLE" ]]; then
    printf 'error: Alan Dev app executable not found at %s\n' "$APP_EXECUTABLE" >&2
    printf '       set ALAN_PRIVILEGED_HELPER_SMOKE_APP_PATH to a signed Alan Dev.app bundle\n' >&2
    exit 1
fi

exec "$APP_EXECUTABLE" --alan-dev-privileged-helper-smoke-and-exit "$@"
