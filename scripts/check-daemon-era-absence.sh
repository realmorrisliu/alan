#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
alan_binary="${1:-}"
cd "$repo_root"

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

reject_path() {
    [[ ! -e "$1" ]] || fail "retired daemon-era path exists: $1"
}

retired_paths=(
    "crates/agent-engine/src/session.rs"
    "crates/agent-engine/skills/swebench/scripts/run_swebench_full_steward_case.sh"
    "crates/agent-engine/skills/swebench/scripts/run_swebench_full_steward_subset.sh"
    "crates/alan/src/cli/daemon.rs"
    "crates/alan/src/daemon"
    "crates/alan/src/host_config.rs"
    "crates/tui/src/daemon_client.rs"
    "clients/apple/alan-macos/Controllers/Console"
    "clients/apple/alan-macos/Models/API/DaemonAPIModels.swift"
    "clients/apple/alan-macos/Models/Console"
    "clients/apple/alan-macos/Services/Daemon"
    "clients/apple/alan-macos/Support/ConsoleAdaptiveColor.swift"
    "clients/apple/alan-macos/Views/Console"
)

for retired_path in "${retired_paths[@]}"; do
    reject_path "$retired_path"
done

scan_roots=(
    .github
    AGENTS.md
    CLAUDE.md
    CONTEXT.md
    CONTRIBUTING.md
    Cargo.toml
    README.md
    clients
    crates
    docs
    justfile
    openspec/changes
    openspec/specs
    packaging
    scripts
)

exclude_globs=(
    '.git/**'
    'target/**'
    'third_party/**'
    'clients/apple/build/**'
    'clients/apple/.build/**'
    'openspec/changes/archive/**'
    'docs/adr/0029-remove-daemon-era-surfaces-before-replacement-design.md'
    'scripts/check-daemon-era-absence.sh'
)

rg_args=()
git_exclude_args=()
for exclude_glob in "${exclude_globs[@]}"; do
    rg_args+=(--glob "!$exclude_glob")
    git_exclude_args+=(":(exclude)$exclude_glob")
done

search_repo() {
    local pattern="$1"
    shift

    if command -v rg >/dev/null 2>&1; then
        rg -n -i "$pattern" "${rg_args[@]}" "$@"
    else
        git grep -n -I -i -E -- "$pattern" -- "$@" "${git_exclude_args[@]}"
    fi
}

matches_file="$(mktemp /tmp/alan-daemon-era-absence.XXXXXX)"
trap 'rm -f "$matches_file"' EXIT

retired_exact_pattern='ALAN_AGENTD_URL|BIND_ADDRESS|/api/v1/(sessions|connections)|reconnect[ _-]snapshot|websocket_url|host\.toml|alan[[:space:]]+daemon|daemon[[:space:]]+(start|stop|status)|DaemonAPIModels|AlanAPIClient|ConsoleEventReducer|EventEnvelope\.session_id|SessionMeta|(^|[^[:alnum:]_])relay([^[:alnum:]_]|$)'

is_allowed_retired_exact_match() {
    local file="$1"

    case "$file" in
        crates/alan/src/legacy_state.rs)
            return 0 # Exact authored host.toml report; never a compatibility reader
            ;;
        docs/adr/0044-one-system-level-alan-os-instance-per-channel.md)
            return 0 # Accepted decision explicitly rejects retired transports
            ;;
        openspec/changes/extract-system-level-alan-os-host/design.md | openspec/changes/extract-system-level-alan-os-host/specs/local-alan-os-attachment/spec.md)
            return 0 # Active target architecture explicitly rejects retired transports
            ;;
    esac
    return 1
}

exact_violations=()
if search_repo "$retired_exact_pattern" "${scan_roots[@]}" >"$matches_file"; then
    while IFS=: read -r file line text; do
        if ! is_allowed_retired_exact_match "$file"; then
            exact_violations+=("$file:$line:$text")
        fi
    done <"$matches_file"
fi

if (("${#exact_violations[@]}" > 0)); then
    printf '%s\n' "${exact_violations[@]}" >&2
    fail "unambiguously retired daemon-era surface found"
fi

is_allowed_daemon_match() {
    local file="$1"
    local text="$2"

    # Current documents may point at the removal decision/change by name.
    [[ "$text" == *"remove-daemon-era"* ]] && return 0
    [[ "$text" == *"check-daemon-era-absence"* ]] && return 0
    [[ "$text" == *"guard-daemon-era-absence"* ]] && return 0

    case "$file" in
        crates/shell-core/src/managed_terminal_account.rs)
            return 0 # Unix reserved account name
            ;;
        crates/agent-engine/skills/swebench/scripts/check_swebench_harness_env.sh)
            return 0 # Docker daemon
            ;;
        crates/alan/src/legacy_state.rs)
            return 0 # Exact generated daemon.pid cleanup; never a compatibility reader
            ;;
        docs/adr/0044-one-system-level-alan-os-instance-per-channel.md | docs/adr/0053-clean-generated-state-import-authored-content.md)
            return 0 # Accepted removal/cleanup decisions name the retired era negatively
            ;;
        openspec/changes/add-alan-package-management/proposal.md)
            return 0 # Planning hold names the retired contract it must be rewritten away from
            ;;
        openspec/changes/remove-workspace-runtime-model/tasks.md)
            return 0 # Exact cleanup task names generated state owned by the retired implementation
            ;;
        clients/apple/alan-macos/Services/Shell/AlanPrivilegedHelperService.swift)
            return 0 # Apple SMAppService.daemon
            ;;
        clients/apple/alan-macos/Services/Shell/AlanPrivilegedHelperManagedUserService.swift)
            return 0 # Reserved Unix account name rejected by Managed User validation
            ;;
        clients/apple/alan-macos/Services/Shell/AlanPrivilegedHelperContracts.swift)
            return 0 # Apple SMAppService.daemon diagnostic label
            ;;
        clients/apple/scripts/test-shell-settings-surface.swift)
            return 0 # SMAppService coverage and negative UI assertions
            ;;
        scripts/check-rust-inline-tui-contract.sh)
            return 0 # Negative structural assertion
            ;;
        scripts/check-openspec-current-surfaces.sh)
            return 0 # Negative structural assertions for deleted OpenSpec source references
            ;;
    esac
    return 1
}

violations=()
daemon_pattern='(^|[^[:alnum:]_])daemon([^[:alnum:]_]|$)|daemon_'
if search_repo "$daemon_pattern" "${scan_roots[@]}" >"$matches_file"; then
    while IFS=: read -r file line text; do
        if ! is_allowed_daemon_match "$file" "$text"; then
            violations+=("$file:$line:$text")
        fi
    done <"$matches_file"
fi

if (("${#violations[@]}" > 0)); then
    printf '%s\n' "${violations[@]}" >&2
    fail "unclassified daemon terminology found; remove it or add a narrow owner-specific rationale"
fi

is_allowed_session_match() {
    local file="$1"
    local text="$2"

    [[ "$text" == *"remove-daemon-era"* ]] && return 0

    case "$file" in
        CONTEXT.md)
            return 0 # Current vocabulary guide explicitly rejects Agent Session
            ;;
        crates/llm/src/openrouter.rs)
            return 0 # OpenRouter SDK request metadata
            ;;
        clients/apple/scripts/test-shell-runtime-metadata.swift)
            return 0 # External Codex metadata redaction fixture
            ;;
        clients/apple/alan-macos/Models/Shell/ShellSidebarTabPresentation.swift)
            return 0 # External command metadata redaction
            ;;
        clients/apple/alan-macos/Models/Shell/TerminalActivityModels.swift)
            return 0 # External agent activity metadata
            ;;
        clients/apple/alan-macos/Services/Terminal/TerminalAgentActivityAdapter.swift)
            return 0 # External agent session label redaction
            ;;
        clients/apple/alan-macos/Services/Shell/AlanPrivilegedHelperContracts.swift)
            return 0 # Managed User PTY session contract
            ;;
        clients/apple/scripts/support/AlanPrivilegedHelperFakeClient.swift)
            return 0 # Managed User PTY test session fixture
            ;;
        openspec/changes/define-groove-master-alan-app/design.md)
            return 0 # Groove Master practice-session domain
            ;;
        openspec/changes/extract-system-level-alan-os-host/design.md | openspec/changes/implement-minimal-service-manager/proposal.md)
            return 0 # Target architecture explicitly rejects Session identity
            ;;
    esac
    return 1
}

violations=()
session_pattern='Agent Session|AgentSession|agent-session|session[-_ ]scoped|session identity|session lifecycle|session_id'
if search_repo "$session_pattern" "${scan_roots[@]}" >"$matches_file"; then
    while IFS=: read -r file line text; do
        if ! is_allowed_session_match "$file" "$text"; then
            violations+=("$file:$line:$text")
        fi
    done <"$matches_file"
fi

if (("${#violations[@]}" > 0)); then
    printf '%s\n' "${violations[@]}" >&2
    fail "unclassified Agent Session terminology found; remove it or add a narrow owner-specific rationale"
fi

[[ -n "$alan_binary" ]] || fail "pass a built alan binary to verify the public help surface"
[[ -x "$alan_binary" ]] || fail "alan binary is not executable: $alan_binary"

"$alan_binary" --help >"$matches_file"
if grep -n -i -E '(^|[^[:alnum:]_])daemon([^[:alnum:]_]|$)|Agent Session|HTTP|WebSocket|relay|reconnect|scheduler' "$matches_file"; then
    fail "retired daemon-era surface is present in alan --help"
fi

if "$alan_binary" daemon --help >"$matches_file" 2>&1; then
    cat "$matches_file" >&2
    fail "retired alan daemon command remains callable"
fi
if ! grep -i -E "unrecognized subcommand.*daemon" "$matches_file" >/dev/null; then
    cat "$matches_file" >&2
    fail "alan daemon failed for an unexpected reason instead of being absent from Clap"
fi

printf 'daemon-era absence guard passed\n'
