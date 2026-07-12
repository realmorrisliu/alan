#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

retired_paths=(
    clients/apple/alan-macos/Services/Shell/ShellStatePersistenceStore.swift
)
for retired_path in "${retired_paths[@]}"; do
    [[ ! -e "$retired_path" ]] || fail "retired macOS path exists: $retired_path"
done

scan_roots=(
    clients/apple/alan-macos
    clients/apple/alan-macos-privileged-helper
    clients/apple/scripts
    crates/shell-core
    crates/shell-core-ffi
    scripts/install-channel.sh
    scripts/install.sh
    scripts/uninstall.sh
)

retired_symbols='AlanNative|ShellStatePersistenceStore|restorePrevious|migrateLegacyTerminalManifest|ShellWorkspaceSpaceRecord|ShellWorkspaceTabRecord|ShellTabRestoreSnapshot|ShellPaneRestoreRecord|LegacyQuickTerminal|legacyQuickTerminal|legacy_quick_terminal|ALAN_LEGACY_APP_BUNDLE_NAME|ManagedTerminalAccountSudoers|legacySudoers|legacy_sudoers|writeSudoersDropIn|validateSudoers|verifyTerminalEntry|removeSudoersDropIn|guiUserName|bindCurrentSpaceAfterSuccess|ManagedTerminalAccountAuthorizedScriptExecutor|ManagedTerminalAccountAppleScriptPrivilegeRunner'

matches="$(rg -n "$retired_symbols" "${scan_roots[@]}" \
    --glob '!clients/apple/scripts/check-architecture-maintainability.sh' \
    --glob '!clients/apple/scripts/check-brand-identity.sh' \
    --glob '!clients/apple/scripts/check-shell-contracts.sh' \
    || true)"
[[ -z "$matches" ]] || {
    printf '%s\n' "$matches" >&2
    fail "retired macOS compatibility symbol found"
}

installer_matches="$(rg -n '/alan\.app/|\"alan\.app\"|lowercase[^\n]*alan\.app' \
    scripts/install-channel.sh scripts/install.sh scripts/uninstall.sh \
    clients/apple/alan-macos/Support/AlanCommandLineToolInstaller.swift || true)"
[[ -z "$installer_matches" ]] || {
    printf '%s\n' "$installer_matches" >&2
    fail "installer still recognizes the retired lowercase bundle"
}

codec_matches="$(rg -n 'quick_terminal|quickTerminal|terminal-only manifest|terminal-only workspace|shell-state-[^[:space:]]*\.json' \
    clients/apple/alan-macos crates/shell-core/src crates/shell-core-ffi/src || true)"
[[ -z "$codec_matches" ]] || {
    printf '%s\n' "$codec_matches" >&2
    fail "retired workspace codec or persistent shell-state surface found"
}

printf 'legacy macOS absence guard passed\n'
