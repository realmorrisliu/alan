#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
failed=0

direct_alan_dependencies() {
    cargo tree -p "$1" --all-features --depth 1 --edges normal --prefix none --no-dedupe \
        --target all \
        | awk 'NR > 1 && $1 ~ /^alan-/ { print $1 }' \
        | sort -u \
        | paste -sd ' ' -
}

direct_dependencies() {
    cargo tree -p "$1" --all-features --depth 1 --edges normal --prefix none --no-dedupe \
        --target all \
        | awk 'NR > 1 { print $1 }' \
        | sort -u
}

check_dependencies() {
    local package="$1"
    local expected="$2"
    local actual
    actual="$(direct_alan_dependencies "$package")"
    if [[ "$actual" != "$expected" ]]; then
        printf 'error: %s normal Alan dependencies\n  expected: %s\n  actual:   %s\n' \
            "$package" "${expected:-<none>}" "${actual:-<none>}" >&2
        failed=1
    fi
}

cd "$ROOT"

dependency_expectations=(
    "alan-ap|"
    "alan-agent-protocol|"
    "alan-auth|"
    "alan-knowledge|"
    "alan-shell-core|"
    "alan-swebench-tooling|"
    "alan-kernel|alan-ap"
    "alan-agentfs|alan-ap alan-knowledge"
    "alan-hostfs|alan-ap"
    "alan-llmfs|alan-ap alan-llm"
    "alan-memfs|alan-ap alan-knowledge"
    "alan-routefs|alan-ap"
    "alan-editfs|alan-ap"
    "alan-branchfs|alan-ap alan-knowledge"
    "alan-shell|alan-ap"
    "alan-shell-core-ffi|alan-shell-core"
    "alan-terminal-ui|alan-agent-protocol alan-ap alan-shell"
    "alan-llm|alan-agent-protocol alan-auth"
    "alan-agent-engine|alan-agent-protocol alan-agentfs alan-ap alan-kernel alan-llm alan-llmfs alan-routefs"
    "alan-tools|alan-agent-engine alan-agent-protocol"
    "alan-service-manager|alan-agent-engine alan-agentfs alan-ap alan-hostfs alan-kernel alan-llm alan-llmfs alan-routefs alan-shell"
    "alan-os-host|alan-agent-engine alan-agentfs alan-ap alan-hostfs alan-kernel alan-llm alan-llmfs alan-routefs alan-service-manager alan-shell"
    "alan|alan-agent-engine alan-agent-protocol alan-ap alan-auth alan-kernel alan-os-host alan-service-manager alan-shell alan-swebench-tooling alan-tools"
)

expected_packages="$({
    for expectation in "${dependency_expectations[@]}"; do
        printf '%s\n' "${expectation%%|*}"
    done
} | sort -u)"
workspace_packages="$(
    cargo tree --workspace --all-features --depth 0 --prefix none \
        | awk 'NF { print $1 }' \
        | sort -u
)"
if [[ "$workspace_packages" != "$expected_packages" ]]; then
    printf 'error: Rust architecture inventory does not cover the complete workspace\n' >&2
    printf '  expected inventory:\n%s\n' "$expected_packages" >&2
    printf '  actual workspace:\n%s\n' "$workspace_packages" >&2
    failed=1
fi

for expectation in "${dependency_expectations[@]}"; do
    check_dependencies "${expectation%%|*}" "${expectation#*|}"
done

kernel_dependencies="$(direct_dependencies alan-kernel)"
for forbidden in alan-agent-engine alan-agent-protocol alan-llm alan-tools reqwest ratatui crossterm; do
    if printf '%s\n' "$kernel_dependencies" | rg --fixed-strings --line-regexp --quiet "$forbidden"; then
        printf 'error: alan-kernel must not depend on %s\n' "$forbidden" >&2
        failed=1
    fi
done

if rg -n \
    -e 'alan_agent_engine' \
    -e 'alan_agent_protocol' \
    -e 'agent_capability' \
    -e 'ViewModel' \
    -e 'renderer_host' \
    crates/kernel/src >&2
then
    printf 'error: alan-kernel source references a forbidden runtime or renderer concept\n' >&2
    failed=1
fi

if [[ "$failed" -ne 0 ]]; then
    exit 1
fi

printf 'Rust architecture dependency gate passed.\n'
