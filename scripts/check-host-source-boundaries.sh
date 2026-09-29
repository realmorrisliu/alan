#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

matches_file="$(mktemp)"
trap 'rm -f "$matches_file"' EXIT

implicit_path_pattern='\.alan/runtime|\.alan/agents|\.agents/skills|\.agents-dev/skills|imports/skills|~/.alan($|[^[:alnum:]_-])|~/.alan-dev($|[^[:alnum:]_-])'
if rg -n "$implicit_path_pattern" crates --glob '*.rs' >"$matches_file"; then
    violations=()
    while IFS=: read -r file line text; do
        case "$file" in
            crates/alan/src/legacy_state.rs | crates/alan/src/legacy_state/tests.rs)
                continue # bounded migration/cleanup owner and adjacent white-box suite
                ;;
            crates/agent-engine/src/agent_definition/tests.rs | crates/alan/tests/agent_definition_descriptor_integration_test.rs)
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
    scripts packaging >"$matches_file"; then
    cat "$matches_file" >&2
    fail "legacy Alan home is still used by a Host script or product surface"
fi

printf 'Host source boundary checks passed\n'
