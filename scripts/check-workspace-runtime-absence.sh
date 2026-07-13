#!/usr/bin/env bash
set -euo pipefail

repo_root="${1:-.}"
alan_binary="${2:-}"
cd "$repo_root"

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

reject_path() {
    [[ ! -e "$1" ]] || fail "retired workspace-runtime path exists: $1"
}

for retired_path in \
    crates/agent-engine/src/agent_root.rs \
    crates/agent-engine/src/paths.rs \
    crates/agent-engine/src/persisted_workspace_config.rs \
    crates/alan/src/registry.rs \
    crates/alan/src/skill_catalog.rs \
    crates/alan/src/cli/init.rs \
    crates/alan/src/cli/workspace.rs \
    crates/agent-engine/skills/workspace-inspect \
    crates/agent-engine/skills/workspace-manager
do
    reject_path "$retired_path"
done

symbols_file="$(mktemp /tmp/alan-workspace-runtime-symbols.XXXXXX)"
matches_file="$(mktemp /tmp/alan-workspace-runtime-paths.XXXXXX)"
trap 'rm -f "$symbols_file" "$matches_file"' EXIT

symbol_pattern='WorkspaceRuntimeConfig|PersistedWorkspaceConfig|WorkspaceRegistry|AgentRootLayout|AlanHomePaths|ToolLocality|WorkspaceTool|workspace_alan_dir|workspace_runtime_dir|global_public_skills|workspace_policy_file|Commands::(Workspace|Init)'
if rg -n "$symbol_pattern" crates \
    --glob '*.rs' \
    --glob '!crates/agent-engine/skills/swebench/**' \
    --glob '!crates/alan/src/legacy_state.rs' >"$symbols_file"; then
    cat "$symbols_file" >&2
    fail "retired workspace-runtime symbol found"
fi

implicit_path_pattern='\.alan/runtime|\.alan/agents|\.agents/skills|\.agents-dev/skills|~/.alan($|[^[:alnum:]_-])|~/.alan-dev($|[^[:alnum:]_-])'
if rg -n "$implicit_path_pattern" crates --glob '*.rs' >"$matches_file"; then
    violations=()
    while IFS=: read -r file line text; do
        case "$file" in
            crates/alan/src/legacy_state.rs)
                continue # sole bounded migration/cleanup owner
                ;;
            crates/agent-engine/src/agent_definition.rs | crates/alan/tests/agent_definition_descriptor_integration_test.rs)
                continue # negative regressions proving no implicit definition discovery
                ;;
            crates/agent-engine/src/tools/sandbox_tests.rs)
                continue # security regressions keep retired sensitive roots protected
                ;;
        esac
        violations+=("$file:$line:$text")
    done <"$matches_file"
    if ((${#violations[@]})); then
        printf '%s\n' "${violations[@]}" >&2
        fail "implicit Host-directory source or runtime path found"
    fi
fi

if rg -n 'std::env::current_dir\(\)' crates/agent-engine/src/rollout.rs >"$matches_file"; then
    cat "$matches_file" >&2
    fail "rollout metadata must use Alan OS cwd, not ambient Host cwd"
fi

if rg -n 'SkillScope::(Repo|User)|serde\(rename = "(repo|user|system)"\)' \
    crates/agent-engine/src --glob '*.rs' >"$matches_file"; then
    cat "$matches_file" >&2
    fail "implicit Skill source scope found"
fi

if rg -n '\$HOME/\.alan(-dev)?|\$\{HOME\}/\.alan(-dev)?' \
    scripts packaging clients/apple/alan-macos >"$matches_file"; then
    cat "$matches_file" >&2
    fail "legacy Alan home is still used by a Host script or product surface"
fi

if [[ -n "$alan_binary" ]]; then
    [[ -x "$alan_binary" ]] || fail "alan binary is not executable: $alan_binary"
    help="$($alan_binary --help)"
    if grep -E '(^|[[:space:]])(init|workspace)([[:space:]]|$)|--agent' <<<"$help" >/dev/null; then
        printf '%s\n' "$help" >&2
        fail "retired workspace command or boot selector is present in alan --help"
    fi
    for retired_command in init workspace; do
        if "$alan_binary" "$retired_command" --help >/dev/null 2>&1; then
            fail "retired alan $retired_command command remains callable"
        fi
    done
    if "$alan_binary" --agent coding --help >/dev/null 2>&1; then
        fail "retired --agent boot selector remains callable"
    fi
fi

printf 'workspace-runtime absence guard passed\n'
