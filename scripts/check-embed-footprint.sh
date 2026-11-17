#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

cargo test -p alan-os-host --locked --lib --no-default-features --features llm-mock
cargo check -p alan-os-host --locked --lib --no-default-features --features provider-openai
for provider in provider-anthropic provider-chatgpt provider-gemini provider-openrouter provider-typesafe; do
    cargo check -p alan-os-host --locked --lib --no-default-features --features "$provider"
done

unique_names() {
    cargo tree -p alan-os-host --locked -e normal --prefix none "$@" \
        | awk '{print $1}' \
        | sort -u
}

count_names() {
    printf '%s\n' "$1" | awk 'NF { count += 1 } END { print count + 0 }'
}

contains_name() {
    printf '%s\n' "$1" | grep -Fxq -- "$2"
}

default_names="$(unique_names)"
openai_names="$(unique_names --no-default-features --features provider-openai)"
mock_names="$(unique_names --no-default-features --features llm-mock)"

default_count="$(count_names "$default_names")"
openai_count="$(count_names "$openai_names")"
mock_count="$(count_names "$mock_names")"

printf 'alan-os-host unique package names: default=%s provider-openai=%s llm-mock=%s\n' \
    "$default_count" "$openai_count" "$mock_count"

failed=0

if [[ "$default_count" -ne 230 ]]; then
    printf 'error: default alan-os-host unique package names are %s, expected 230\n' \
        "$default_count" >&2
    failed=1
fi

forbidden=(
    alan-tools
    openrouter-rs
    schemars
    tree-sitter
    tree-sitter-bash
    tree-sitter-language
    alan-auth
    jsonschema
)
for package in "${forbidden[@]}"; do
    if contains_name "$openai_names" "$package"; then
        printf 'error: provider-openai tree contains %s\n' "$package" >&2
        failed=1
    fi
    if contains_name "$mock_names" "$package"; then
        printf 'error: llm-mock tree contains %s\n' "$package" >&2
        failed=1
    fi
done

for package in reqwest rustls aws-lc-sys; do
    if contains_name "$mock_names" "$package"; then
        printf 'error: llm-mock tree contains %s\n' "$package" >&2
        failed=1
    fi
done

if ! contains_name "$openai_names" reqwest; then
    printf 'error: provider-openai tree is missing reqwest\n' >&2
    failed=1
fi
if ! contains_name "$openai_names" alan-hostfs; then
    printf 'error: provider-openai tree is missing alan-hostfs\n' >&2
    failed=1
fi
if ! contains_name "$mock_names" alan-hostfs; then
    printf 'error: llm-mock tree is missing alan-hostfs\n' >&2
    failed=1
fi

if [[ "$openai_count" -gt 174 ]]; then
    printf 'error: provider-openai unique package names are %s, ceiling is 174\n' \
        "$openai_count" >&2
    printf '%s\n' "$openai_names" >&2
    failed=1
fi
if [[ "$mock_count" -gt 112 ]]; then
    printf 'error: llm-mock unique package names are %s, ceiling is 112\n' \
        "$mock_count" >&2
    printf '%s\n' "$mock_names" >&2
    failed=1
fi

if [[ "$failed" -ne 0 ]]; then
    exit 1
fi

printf 'Embed footprint check passed.\n'
