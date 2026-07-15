#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
failed=0

direct_alan_dependencies() {
    cargo tree -p "$1" --depth 1 --edges normal --prefix none --no-dedupe --target all \
        | awk 'NR > 1 && $1 ~ /^alan-/ { print $1 }' \
        | sort -u \
        | paste -sd ' ' -
}

direct_dependencies() {
    cargo tree -p "$1" --depth 1 --edges normal --prefix none --no-dedupe --target all \
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

check_dependencies alan-ap ""
check_dependencies alan-agent-protocol ""
check_dependencies alan-auth ""
check_dependencies alan-knowledge ""
check_dependencies alan-shell-core ""
check_dependencies alan-swebench-tooling ""
check_dependencies alan-kernel "alan-ap"
check_dependencies alan-agentfs "alan-ap alan-knowledge"
check_dependencies alan-hostfs "alan-ap"
check_dependencies alan-llmfs "alan-ap alan-llm"
check_dependencies alan-memfs "alan-ap alan-knowledge"
check_dependencies alan-routefs "alan-ap"
check_dependencies alan-editfs "alan-ap"
check_dependencies alan-branchfs "alan-ap alan-knowledge"
check_dependencies alan-shell "alan-ap"
check_dependencies alan-shell-core-ffi "alan-shell-core"
check_dependencies alan-terminal-ui "alan-agent-protocol alan-ap alan-shell"
check_dependencies alan-llm "alan-agent-protocol alan-auth"
check_dependencies alan-agent-engine \
    "alan-agent-protocol alan-agentfs alan-ap alan-kernel alan-llm alan-llmfs alan-routefs"
check_dependencies alan-tools "alan-agent-engine alan-agent-protocol"
check_dependencies alan-service-manager \
    "alan-agent-engine alan-agentfs alan-ap alan-hostfs alan-kernel alan-llm alan-llmfs alan-routefs alan-shell"
check_dependencies alan-os-host \
    "alan-agent-engine alan-agentfs alan-ap alan-hostfs alan-kernel alan-llm alan-llmfs alan-routefs alan-service-manager alan-shell"
check_dependencies alan \
    "alan-agent-engine alan-agent-protocol alan-ap alan-auth alan-kernel alan-os-host alan-service-manager alan-shell alan-swebench-tooling alan-tools"

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
