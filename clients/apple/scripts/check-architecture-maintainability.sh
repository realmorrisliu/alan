#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
APPLE_ROOT="$REPO_ROOT/clients/apple"
SOURCE_ROOT="$APPLE_ROOT/alan-macos"
HELPER_SOURCE_ROOT="$APPLE_ROOT/alan-macos-privileged-helper"
PROJECT_FILE="$APPLE_ROOT/alan-macos.xcodeproj/project.pbxproj"
README_FILE="$APPLE_ROOT/README.md"
ARCH_DOC="$APPLE_ROOT/ARCHITECTURE.md"
WARNING_BASELINE="$SCRIPT_DIR/architecture-warning-baseline.txt"
WARNING_BASELINE_REL="clients/apple/scripts/architecture-warning-baseline.txt"
STRICT=0

if [[ "${1:-}" == "--strict" ]]; then
    STRICT=1
fi

warnings=0
failures=0
warning_inventory="$(mktemp)"
warning_inventory_sorted="$(mktemp)"
warning_baseline_body="$(mktemp)"
warning_baseline_sorted="$(mktemp)"
base_warning_baseline="$(mktemp)"
trap 'rm -f "$warning_inventory" "$warning_inventory_sorted" "$warning_baseline_body" "$warning_baseline_sorted" "$base_warning_baseline"' EXIT

git_command=(git)
if [[ -n "${ALAN_QUALITY_GIT_DIR:-}" ]]; then
    git_command+=(--git-dir="$ALAN_QUALITY_GIT_DIR")
fi
base_ref="${ALAN_QUALITY_BASE_REF:-HEAD}"

"$SCRIPT_DIR/check-brand-identity.sh"

warn() {
    local key="$1"
    shift
    printf 'warning: %s\n' "$1"
    printf '%s\n' "$key" >>"$warning_inventory"
    warnings=$((warnings + 1))
}

fail() {
    printf 'error: %s\n' "$1" >&2
    failures=$((failures + 1))
}

validate_warning_baseline() {
    local duplicate

    if [[ ! -f "$WARNING_BASELINE" ]]; then
        fail "clients/apple/scripts/architecture-warning-baseline.txt must record accepted warnings"
        return
    fi

    if ! awk -F '|' '
        /^[[:space:]]*#/ || NF == 0 { next }
        $1 == "large" {
            if (NF != 3 || $2 == "" || $3 !~ /^[0-9]+$/) {
                printf "invalid Apple warning baseline entry: %s\n", $0 > "/dev/stderr"
                invalid = 1
                next
            }
            print
            next
        }
        $1 == "bridge" {
            if (NF != 3 || $2 == "" || $3 == "") {
                printf "invalid Apple warning baseline entry: %s\n", $0 > "/dev/stderr"
                invalid = 1
                next
            }
            print
            next
        }
        $1 == "missing-target-folder" ||
        $1 == "readme-missing-file" ||
        $1 == "readme-missing-folder" ||
        $1 == "readme-missing-command" {
            if (NF != 2 || $2 == "") {
                printf "invalid Apple warning baseline entry: %s\n", $0 > "/dev/stderr"
                invalid = 1
                next
            }
            print
            next
        }
        {
            printf "unknown Apple warning baseline entry: %s\n", $0 > "/dev/stderr"
            invalid = 1
        }
        END { exit invalid }
    ' "$WARNING_BASELINE" >"$warning_baseline_body"; then
        fail "Apple architecture warning baseline is malformed"
        return
    fi

    LC_ALL=C sort "$warning_inventory" >"$warning_inventory_sorted"
    LC_ALL=C sort "$warning_baseline_body" >"$warning_baseline_sorted"

    duplicate="$(uniq -d "$warning_baseline_sorted" | head -n 1)"
    if [[ -n "$duplicate" ]]; then
        fail "Apple architecture warning baseline contains duplicate entry: $duplicate"
    fi

    duplicate="$(uniq -d "$warning_inventory_sorted" | head -n 1)"
    if [[ -n "$duplicate" ]]; then
        fail "Apple architecture report emitted duplicate warning key: $duplicate"
    fi

    if ! cmp -s "$warning_baseline_body" "$warning_baseline_sorted"; then
        fail "Apple architecture warning baseline entries must stay sorted"
    fi

    if ! cmp -s "$warning_inventory_sorted" "$warning_baseline_sorted"; then
        printf 'Apple architecture warning ledger drift:\n' >&2
        diff -u "$warning_baseline_sorted" "$warning_inventory_sorted" >&2 || true
        fail "update the Apple warning baseline in the same reduction change"
    fi
}

compare_warning_baseline_with_base() {
    if ! "${git_command[@]}" cat-file -e "$base_ref^{commit}" 2>/dev/null; then
        fail "Apple architecture warning ratchet base is not a commit: $base_ref"
        return
    fi

    if ! "${git_command[@]}" cat-file -e "$base_ref:$WARNING_BASELINE_REL" 2>/dev/null; then
        printf 'Apple architecture warning baseline established relative to %s.\n' "$base_ref"
        return
    fi

    "${git_command[@]}" show "$base_ref:$WARNING_BASELINE_REL" >"$base_warning_baseline"
    if ! awk -F '|' '
        NR == FNR {
            if ($0 ~ /^[[:space:]]*#/ || NF == 0) {
                next
            }
            if ($1 == "large") {
                previous_large[$2] = $3
            } else {
                previous[$0] = 1
            }
            next
        }
        $1 == "large" {
            if (!($2 in previous_large)) {
                printf "error: new Apple large-file warning: %s\n", $2 > "/dev/stderr"
                failed = 1
            } else if ($3 > previous_large[$2]) {
                printf "error: Apple large-file debt grew for %s from %d to %d lines\n", $2, previous_large[$2], $3 > "/dev/stderr"
                failed = 1
            }
            next
        }
        !($0 in previous) {
            printf "error: new or broadened Apple architecture warning: %s\n", $0 > "/dev/stderr"
            failed = 1
        }
        END { exit failed }
    ' "$base_warning_baseline" "$warning_baseline_body"; then
        fail "Apple architecture warning debt may shrink but must not grow"
    fi
}

contains_line() {
    local needle="$1"
    shift
    local item
    for item in "$@"; do
        [[ "$item" == "$needle" ]] && return 0
    done
    return 1
}

check_appkit_import_gate() {
    local rel="$1"
    local file="$2"
    if ! awk '
        /^#if .*os\(macOS\)/ || /^#elseif .*os\(macOS\)/ || /^#if .*canImport\(AppKit\)/ {
            inside_appkit_gate = 1
            next
        }
        /^#else/ || /^#endif/ {
            inside_appkit_gate = 0
            next
        }
        /^import AppKit$/ && !inside_appkit_gate {
            exit 1
        }
    ' "$file"; then
        fail "$rel imports AppKit before a macOS/AppKit platform gate"
    fi
}

current_root_swift_allowlist=(
    "AlanAppSingletonGuard.swift"
    "AlanApp.swift"
    "GhosttyLiveHost.swift"
    "MacShellRootView.swift"
    "ShellControlPlane.swift"
    "ShellHostController.swift"
    "ShellModel.swift"
    "TerminalHostRuntime.swift"
    "TerminalHostView.swift"
    "TerminalPaneView.swift"
    "TerminalRuntimeRegistry.swift"
    "TerminalRuntimeService.swift"
    "TerminalSurfaceController.swift"
)

target_dirs=(
    "App"
    "Views/Shell"
    "Models"
    "Controllers"
    "Services"
    "Support"
)

large_file_threshold=1200

printf 'Apple architecture maintainability report\n'
printf 'Source root: clients/apple/alan-macos\n\n'

if [[ ! -f "$ARCH_DOC" ]]; then
    fail "clients/apple/ARCHITECTURE.md must record the architecture inventory and target layout"
else
    if ! grep -q "## Shell Core Boundary" "$ARCH_DOC"; then
        fail "clients/apple/ARCHITECTURE.md must document the Shell Core Boundary"
    fi
    if ! grep -q "new reusable domain behavior belongs in" "$ARCH_DOC"; then
        fail "clients/apple/ARCHITECTURE.md must keep the Rust shell-core ownership rule"
    fi
fi

if grep -Fq "ShellStateMutations.swift in Sources" "$PROJECT_FILE" \
    || grep -Fq "ShellTreeMutations.swift in Sources" "$PROJECT_FILE" \
    || grep -Fq "ShellStateMutationParitySupport.swift" "$PROJECT_FILE" \
    || grep -Fq "ShellTreeMutationParitySupport.swift" "$PROJECT_FILE"
then
    fail "Swift reducer parity support must stay out of the alan-macos Xcode target"
fi

if find "$REPO_ROOT/clients/apple/scripts/support" -name '*ParitySupport.swift' -print -quit | grep -q .; then
    fail "Swift parity support files must not be reintroduced; use shell-core contract tests or FFI-backed test builders"
fi

if ! grep -Fq "ShellStateRuntimeSupport.swift in Sources" "$PROJECT_FILE"; then
    fail "alan-macos target must keep the narrow runtime shell-state support owner"
fi

require_rust_reducer_adapter() {
    local file="$1"
    shift
    local forbidden

    if [[ ! -f "$file" ]]; then
        return
    fi
    for forbidden in "$@"; do
        if grep -Fq "$forbidden" "$file"; then
            fail "${file#$SOURCE_ROOT/} must route ${forbidden#shellState.} through the Rust shell-core adapter"
        fi
    done
}

require_single_owner_pattern() {
    local pattern="$1"
    local owner="$2"
    local description="$3"
    local file
    local rel

    while IFS= read -r file; do
        rel="${file#$SOURCE_ROOT/}"
        if [[ "$rel" != "$owner" ]]; then
            fail "$description must stay in $owner; found in $rel"
        fi
    done < <(grep -RIl --include='*.swift' -F "$pattern" "$SOURCE_ROOT" || true)
}

require_existing_single_owner_pattern() {
    local pattern="$1"
    local owner="$2"
    local description="$3"
    local file
    local rel
    local matched=0

    while IFS= read -r file; do
        matched=1
        rel="${file#$SOURCE_ROOT/}"
        if [[ "$rel" != "$owner" ]]; then
            fail "$description must stay in $owner; found in $rel"
        fi
    done < <(grep -RIl --include='*.swift' -F "$pattern" "$SOURCE_ROOT" || true)

    if [[ "$matched" -eq 0 ]]; then
        fail "$description must exist in $owner"
    fi
}

shell_core_ffi_shared_callsite_owner_allowlist=(
    "Models/Shell/ShellSettingsSurfaceModel.swift"
    "Models/Shell/ShellValueTypes.swift"
    "Services/Shell/ShellActionCoordinator.swift"
    "Services/Shell/ShellLocalCommandExecutor.swift"
    "Services/Shell/ShellReducerCommandCoordinator.swift"
    "Services/Shell/ShellWorkspaceManifestStartupCoordinator.swift"
    "Services/Shell/ShellWorkspaceManifestStore.swift"
    "Services/Shell/TerminalProfileStore.swift"
    "TerminalHostRuntime.swift"
)

shell_core_ffi_direct_init_owner_allowlist=(
    "Services/Shell/ShellCoreFFILoader.swift"
)

shell_core_ffi_raw_symbol_owner_allowlist=(
    "Services/Shell/ShellCoreFFILoader.swift"
)

require_shell_core_ffi_shared_callsite_owners() {
    local file
    local rel

    while IFS= read -r file; do
        rel="${file#$SOURCE_ROOT/}"
        if ! contains_line "$rel" "${shell_core_ffi_shared_callsite_owner_allowlist[@]}"; then
            fail "shell-core FFI shared calls must stay in documented owner files; found in $rel"
        fi
    done < <(grep -RIl --include='*.swift' -F "ShellCoreFFIAdapter.shared" "$SOURCE_ROOT" || true)
}

require_shell_core_ffi_direct_init_owners() {
    local file
    local rel

    while IFS= read -r file; do
        rel="${file#$SOURCE_ROOT/}"
        if ! contains_line "$rel" "${shell_core_ffi_direct_init_owner_allowlist[@]}"; then
            fail "direct shell-core FFI adapter construction must stay in the loader owner; found in $rel"
        fi
    done < <(grep -RIl --include='*.swift' -F "ShellCoreFFIAdapter(" "$SOURCE_ROOT" || true)
}

require_shell_core_ffi_raw_symbol_owners() {
    local file
    local rel

    while IFS= read -r file; do
        rel="${file#$SOURCE_ROOT/}"
        if ! contains_line "$rel" "${shell_core_ffi_raw_symbol_owner_allowlist[@]}"; then
            fail "raw shell-core FFI symbols must stay in the loader owner; found in $rel"
        fi
    done < <(grep -RIl --include='*.swift' -F "alan_shell_core_ffi_" "$SOURCE_ROOT" || true)
}

require_shell_core_action_metadata_query_owners() {
    require_existing_single_owner_pattern \
        "ShellCoreFFIAdapter.shared.actionTitle" \
        "Services/Shell/ShellActionCoordinator.swift" \
        "shell-core action title lookup"

    require_existing_single_owner_pattern \
        "ShellCoreFFIAdapter.shared.actionAvailability" \
        "Services/Shell/ShellActionCoordinator.swift" \
        "shell-core action availability lookup"

    require_existing_single_owner_pattern \
        "ShellCoreFFIAdapter.shared.defaultActionShortcut" \
        "Services/Shell/ShellActionCoordinator.swift" \
        "shell-core action shortcut lookup"

    require_existing_single_owner_pattern \
        "ShellCoreFFIAdapter.shared.keyboardAction" \
        "Services/Shell/ShellActionCoordinator.swift" \
        "shell-core keyboard action lookup"

    require_existing_single_owner_pattern \
        "actions.standard_descriptors" \
        "Services/Shell/ShellCoreFFIActionAdapter.swift" \
        "shell-core action descriptor FFI operation"

    require_existing_single_owner_pattern \
        "actions.default_shortcut" \
        "Services/Shell/ShellCoreFFIActionAdapter.swift" \
        "shell-core default shortcut FFI operation"

    require_existing_single_owner_pattern \
        "actions.keyboard_action" \
        "Services/Shell/ShellCoreFFIActionAdapter.swift" \
        "shell-core keyboard action FFI operation"

    if grep -RIn --include='*.swift' \
        -e "ShellActionAvailabilityResolver\\.availability" \
        -e "ShellActionMetadataCatalog\\.shortcut" \
        -e "ShellActionMetadataCatalog\\.keyboardAction" \
        "$SOURCE_ROOT" >&2
    then
        fail "production shell action metadata must use shell-core FFI instead of Swift metadata fallback"
    fi
}

reject_shell_host_published_terminal_runtime() {
    local file="$SOURCE_ROOT/ShellHostController.swift"

    if grep -En '@Published[^[:cntrl:]]*terminalRuntime' "$file" >&2; then
        fail "ShellHostController.terminalRuntime must not be @Published; high-frequency terminal runtime state must not invalidate the whole SwiftUI shell"
    fi
}

reject_swiftui_shell_hot_path_sync_boundaries() {
    local matched=0
    local pattern
    local search_roots=(
        "$SOURCE_ROOT/MacShellRootView.swift"
        "$SOURCE_ROOT/TerminalHostView.swift"
        "$SOURCE_ROOT/TerminalPaneView.swift"
        "$SOURCE_ROOT/Views/Shell"
    )

    for pattern in \
        "ShellCoreFFIAdapter" \
        "ShellReducerCommandCoordinator" \
        "reducerCoordinator.apply" \
        "AlanShellLocalCommandExecutor.execute" \
        "actions.execute" \
        "actions.standard_descriptors" \
        "JSONEncoder" \
        "JSONDecoder"
    do
        if grep -RIn --include='*.swift' -F "$pattern" "${search_roots[@]}" >&2; then
            matched=1
        fi
    done

    if [[ "$matched" -ne 0 ]]; then
        fail "SwiftUI shell render/body/context-menu hot paths must not synchronously call shell-core FFI, JSON codecs, local command executors, or reducers"
    fi
}

require_rust_reducer_adapter \
    "$SOURCE_ROOT/Controllers/Shell/ShellHostControlCommandHandling.swift" \
    "shellState.resizingSplit(" \
    "shellState.equalizingSplits(" \
    "shellState.focusingAdjacentPane(" \
    "shellState.movingPaneWithinTab("

require_rust_reducer_adapter \
    "$SOURCE_ROOT/Services/Shell/ShellLocalCommandExecutor.swift" \
    "state.creatingSpace(" \
    "state.settingTerminalProfile(" \
    "state.openingTerminalTab(" \
    "state.closingTab(" \
    "state.pinningTab(" \
    "state.unpinningTab(" \
    "state.organizingTab(" \
    "state.movingTabToSpace(" \
    "state.splittingPane(" \
    "state.closingPane(" \
    "state.movingPaneToNewTab(" \
    "state.movingPane(" \
    "state.focusingPane(" \
    "state.settingAttention("

require_rust_reducer_adapter \
    "$SOURCE_ROOT/ShellHostController.swift" \
    "shellState.creatingSpace(" \
    "shellState.settingTerminalProfile(" \
    "shellState.settingPresentationIcon(" \
    "shellState.deletingSpace(" \
    "shellState.organizingTab(" \
    "shellState.clearingInactiveTemporaryTabs(" \
    "shellState.closingPane(" \
    "shellState.closingTab(" \
    "shellState.duplicatingTab(" \
    "shellState.resizingSplit(" \
    "shellState.equalizingSplits(" \
    "shellState.focusingPane(" \
    "shellState.movingPane(" \
    "shellState.movingPaneToNewTab(" \
    "shellState.movingPaneWithinTab(" \
    "shellState.openingContentTab(" \
    "shellState.splittingPane("

require_single_owner_pattern \
    "ShellCoreFFIAdapter.shared.applyReducer" \
    "Services/Shell/ShellReducerCommandCoordinator.swift" \
    "shell-core reducer invocation"

require_single_owner_pattern \
    "ShellCoreFFIAdapter.shared.executeAction" \
    "Services/Shell/ShellActionCoordinator.swift" \
    "shell-core action execution"

require_single_owner_pattern \
    "ShellCoreFFIAdapter.shared.handleControlCommand" \
    "Services/Shell/ShellLocalCommandExecutor.swift" \
    "shell-core local control command handling"

require_single_owner_pattern \
    "ShellCoreFFIAdapter.shared.defaultContentWorkspaceManifest" \
    "Services/Shell/ShellWorkspaceManifestStore.swift" \
    "shell-core workspace manifest defaulting"

require_single_owner_pattern \
    "ShellCoreFFIAdapter.shared.pruningExpiredTabs" \
    "Services/Shell/ShellWorkspaceManifestStartupCoordinator.swift" \
    "shell-core workspace manifest pruning"

require_single_owner_pattern \
    "ShellCoreFFIAdapter.shared.materializeContentWorkspaceManifest" \
    "Services/Shell/ShellWorkspaceManifestStartupCoordinator.swift" \
    "shell-core workspace manifest materialization"

require_single_owner_pattern \
    "ShellCoreFFIAdapter.shared.validateTerminalProfileDocument" \
    "Services/Shell/TerminalProfileStore.swift" \
    "shell-core Terminal Profile validation"

require_single_owner_pattern \
    "ShellCoreFFIAdapter.shared.makeTerminalProfileDefinition" \
    "Services/Shell/TerminalProfileStore.swift" \
    "shell-core Terminal Profile editor semantics"

require_single_owner_pattern \
    "ShellCoreFFIAdapter.shared.upsertTerminalProfileDraft" \
    "Services/Shell/TerminalProfileStore.swift" \
    "shell-core Terminal Profile document editor semantics"

require_single_owner_pattern \
    "ShellCoreFFIAdapter.shared.shouldCaptureGlobalDefaultTerminalProfile" \
    "Services/Shell/TerminalProfileStore.swift" \
    "shell-core global default Terminal Profile capture policy"

require_single_owner_pattern \
    "ShellCoreFFIAdapter.shared.resolveTerminalLaunchIntent" \
    "TerminalHostRuntime.swift" \
    "shell-core Terminal Profile launch intent resolution"

require_single_owner_pattern \
    "ShellCoreFFIAdapter.shared.terminalProfileRows" \
    "Models/Shell/ShellSettingsSurfaceModel.swift" \
    "shell-core Terminal Profile settings row projection"

require_single_owner_pattern \
    "ShellCoreFFIAdapter.shared.managedTerminalAccountRows" \
    "Models/Shell/ShellSettingsSurfaceModel.swift" \
    "shell-core managed terminal account settings row projection"

require_single_owner_pattern \
    "ShellCoreFFIAdapter.shared.capabilityRows" \
    "Models/Shell/ShellSettingsSurfaceModel.swift" \
    "shell-core capability settings row projection"

require_single_owner_pattern \
    "ShellCoreFFIAdapter.shared.localRows" \
    "Models/Shell/ShellSettingsSurfaceModel.swift" \
    "shell-core local settings row projection"

require_single_owner_pattern \
    "ShellCoreFFIAdapter.shared.validateManagedTerminalAccountRequest" \
    "Models/Shell/ShellValueTypes.swift" \
    "shell-core managed terminal account validation"

require_single_owner_pattern \
    "ShellCoreFFIAdapter.shared.managedTerminalAccountPlan" \
    "Models/Shell/ShellValueTypes.swift" \
    "shell-core managed terminal account provisioning planner"

require_shell_core_ffi_shared_callsite_owners
require_shell_core_ffi_direct_init_owners
require_shell_core_ffi_raw_symbol_owners
require_shell_core_action_metadata_query_owners
reject_shell_host_published_terminal_runtime
reject_swiftui_shell_hot_path_sync_boundaries

printf 'Current Swift inventory:\n'
while IFS= read -r file; do
    rel="${file#$SOURCE_ROOT/}"
    lines="$(wc -l < "$file" | tr -d ' ')"
    imports="$(grep -E '^import ' "$file" | sed 's/^import //' | paste -sd ',' - || true)"
    gates="$(grep -E '^#if (os|canImport)' "$file" | sed 's/^#if //' | paste -sd ',' - || true)"
    if [[ -z "$imports" ]]; then
        imports="-"
    fi
    if [[ -z "$gates" ]]; then
        gates="-"
    fi
    printf '  %-36s %5s lines  imports=%s  gates=%s\n' "$rel" "$lines" "$imports" "$gates"
    check_appkit_import_gate "$rel" "$file"

    if [[ "$rel" != */* ]]; then
        if ! contains_line "$rel" "${current_root_swift_allowlist[@]}"; then
            fail "new root-level Swift file '$rel' should be placed in the target owner folder"
        fi
    fi

    if (( lines > large_file_threshold )); then
        warn "large|$rel|$lines" \
            "$rel is $lines lines; keep new behavior in the target owner or document the temporary boundary"
    fi

    if grep -Eq '^import (AppKit|Darwin)$' "$file"; then
        case "$rel" in
            App/*|Services/*|Support/*|Views/Shell/Terminal/*|AlanApp.swift|AlanAppSingletonGuard.swift|GhosttyLiveHost.swift|ShellControlPlane.swift|TerminalHostView.swift|TerminalRuntimeService.swift|TerminalSurfaceController.swift)
                ;;
            MacShellRootView.swift|ShellHostController.swift|TerminalRuntimeRegistry.swift)
                warn "bridge|$rel|appkit-or-darwin-outside-bridge" \
                    "$rel imports AppKit or Darwin while it remains outside a narrow bridge owner"
                ;;
            *)
                fail "$rel imports AppKit or Darwin outside an accepted app, service, support, or terminal bridge boundary"
                ;;
        esac
    fi

    if ! grep -q "$rel" "$PROJECT_FILE"; then
        fail "$rel is not referenced by the Xcode project"
    fi
done < <(find "$SOURCE_ROOT" -name '*.swift' -type f | sort)

printf '\nTarget layout status:\n'
for dir in "${target_dirs[@]}"; do
    if [[ -d "$SOURCE_ROOT/$dir" ]]; then
        printf '  present: clients/apple/alan-macos/%s\n' "$dir"
    else
        warn "missing-target-folder|$dir" \
            "target folder clients/apple/alan-macos/$dir is not present yet"
    fi
    if [[ -f "$ARCH_DOC" ]] && ! grep -q "\`$dir/\`" "$ARCH_DOC"; then
        fail "clients/apple/ARCHITECTURE.md must document target folder $dir/"
    fi
done

printf '\nREADME layout drift:\n'
while IFS= read -r entry; do
    path="$(printf '%s' "$entry" | sed -E 's/^- `([^`]+)`.*/\1/')"
    [[ "$path" == "$entry" ]] && continue
    case "$path" in
        *.swift)
            [[ -f "$SOURCE_ROOT/$path" ]] || warn "readme-missing-file|$path" \
                "README lists $path but the file is not at clients/apple/alan-macos/$path"
            ;;
        */)
            [[ -d "$SOURCE_ROOT/${path%/}" ]] || warn "readme-missing-folder|$path" \
                "README lists $path but the folder is not present yet"
            ;;
    esac
done < <(grep -E '^- `[^`]+`' "$README_FILE" || true)

if ! grep -q "check-architecture-maintainability.sh" "$README_FILE"; then
    warn "readme-missing-command|check-architecture-maintainability.sh" \
        "README does not mention the architecture maintainability report command"
fi

printf '\nXcode project membership drift:\n'
while IFS= read -r ref; do
    name="$(printf '%s' "$ref" | sed -E 's/.*path = ([^;]+);.*/\1/')"
    [[ "$name" == "$ref" ]] && continue
    [[ "$name" == *.swift ]] || continue
    if [[ ! -f "$SOURCE_ROOT/$name" && ! -f "$HELPER_SOURCE_ROOT/$name" ]]; then
        fail "Xcode project references missing Swift file $name"
    fi
done < <(grep -E 'path = .*\.swift;' "$PROJECT_FILE" || true)

validate_warning_baseline
compare_warning_baseline_with_base

if [[ -f "$ARCH_DOC" ]] && ! grep -Eq "^${warnings} known large-file / bridge-boundary warning" "$ARCH_DOC"; then
    fail "clients/apple/ARCHITECTURE.md must record the current report-mode warning count ($warnings)"
fi

if (( failures > 0 )); then
    printf '\nArchitecture maintainability check failed with %d error(s) and %d warning(s).\n' "$failures" "$warnings" >&2
    exit 1
fi

if (( STRICT == 1 && warnings > 0 )); then
    printf '\nArchitecture maintainability strict check failed with %d warning(s).\n' "$warnings" >&2
    exit 1
fi

printf '\nArchitecture maintainability report completed with %d warning(s).\n' "$warnings"
