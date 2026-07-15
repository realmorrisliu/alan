#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASELINE="$ROOT/scripts/rust-dependency-baseline.txt"
BASELINE_REL="scripts/rust-dependency-baseline.txt"
failed=0
base_baseline="$(mktemp)"
trap 'rm -f "$base_baseline"' EXIT

export LC_ALL=C

git_command=(git)
if [[ -n "${ALAN_QUALITY_GIT_DIR:-}" ]]; then
    git_command+=(--git-dir="$ALAN_QUALITY_GIT_DIR")
fi
base_ref="${ALAN_QUALITY_BASE_REF:-HEAD}"

validate_dependency_baseline() {
    local baseline="$1"
    awk -F '|' '
        /^[[:space:]]*#/ || /^[[:space:]]*$/ { next }
        NF != 2 {
            printf "error: dependency baseline line %d must contain exactly one | separator\n", NR > "/dev/stderr"
            failed = 1
            next
        }
        $1 !~ /^(alan|alan-[a-z0-9-]+)$/ {
            printf "error: dependency baseline line %d has invalid package %s\n", NR, $1 > "/dev/stderr"
            failed = 1
        }
        previous_package != "" && $1 <= previous_package {
            printf "error: dependency baseline packages must be unique and sorted: %s\n", $1 > "/dev/stderr"
            failed = 1
        }
        {
            previous_package = $1
            if ($2 ~ /^ / || $2 ~ / $/ || $2 ~ /  /) {
                printf "error: dependency baseline for %s has invalid spacing\n", $1 > "/dev/stderr"
                failed = 1
            }
            count = split($2, dependencies, " ")
            previous_dependency = ""
            for (i = 1; i <= count; i++) {
                dependency = dependencies[i]
                if (dependency == "") {
                    continue
                }
                if (dependency == "alan") {
                    printf "error: %s cannot accept the root alan composition crate as a dependency\n", $1 > "/dev/stderr"
                    failed = 1
                } else if (dependency !~ /^alan-[a-z0-9-]+$/) {
                    printf "error: dependency baseline for %s has invalid dependency %s\n", $1, dependency > "/dev/stderr"
                    failed = 1
                }
                if (previous_dependency != "" && dependency <= previous_dependency) {
                    printf "error: dependencies for %s must be unique and sorted: %s\n", $1, dependency > "/dev/stderr"
                    failed = 1
                }
                previous_dependency = dependency
            }
        }
        END { exit failed }
    ' "$baseline"
}

direct_alan_dependencies() {
    cargo tree -p "$1" --locked --all-features --depth 1 --edges normal --prefix none \
        --no-dedupe \
        --target all \
        | awk 'NR > 1 && ($1 == "alan" || $1 ~ /^alan-/) { print $1 }' \
        | sort -u \
        | paste -sd ' ' -
}

direct_dependencies() {
    cargo tree -p "$1" --locked --all-features --depth 1 --edges normal --prefix none \
        --no-dedupe \
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

validate_dependency_baseline "$BASELINE"

if ! "${git_command[@]}" cat-file -e "$base_ref^{commit}" 2>/dev/null; then
    printf 'error: Rust dependency ratchet base is not a commit: %s\n' "$base_ref" >&2
    exit 1
fi

if "${git_command[@]}" cat-file -e "$base_ref:$BASELINE_REL" 2>/dev/null; then
    "${git_command[@]}" show "$base_ref:$BASELINE_REL" >"$base_baseline"
    validate_dependency_baseline "$base_baseline"
    awk -F '|' '
        /^[[:space:]]*#/ || /^[[:space:]]*$/ { next }
        NR == FNR {
            count = split($2, dependencies, " ")
            for (i = 1; i <= count; i++) {
                if (dependencies[i] != "") {
                    previous[$1 SUBSEP dependencies[i]] = 1
                }
            }
            next
        }
        {
            count = split($2, dependencies, " ")
            for (i = 1; i <= count; i++) {
                dependency = dependencies[i]
                if (dependency != "" && !(($1 SUBSEP dependency) in previous)) {
                    printf "error: Rust dependency allowance expanded: %s -> %s\n", $1, dependency > "/dev/stderr"
                    failed = 1
                }
            }
        }
        END { exit failed }
    ' "$base_baseline" "$BASELINE"
else
    printf 'Rust dependency ratchet baseline established relative to %s.\n' "$base_ref"
fi

dependency_expectations=()
while IFS= read -r expectation; do
    if [[ -z "$expectation" || "$expectation" == \#* ]]; then
        continue
    fi
    dependency_expectations+=("$expectation")
done < "$BASELINE"

expected_packages="$({
    for expectation in "${dependency_expectations[@]}"; do
        printf '%s\n' "${expectation%%|*}"
    done
} | sort -u)"
workspace_packages="$(
    cargo tree --workspace --locked --all-features --depth 0 --prefix none \
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
