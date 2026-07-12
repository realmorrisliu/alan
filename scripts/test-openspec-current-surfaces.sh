#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fixtures="$repo_root/scripts/fixtures/openspec-current-surfaces"
guard="$repo_root/scripts/check-openspec-current-surfaces.sh"
tmp_root="$(mktemp -d /tmp/alan-openspec-current-surfaces.XXXXXX)"
trap 'rm -rf "$tmp_root"' EXIT
fallback_path="$(dirname "$(command -v bash)"):$(dirname "$(command -v find)"):$(dirname "$(command -v grep)"):$(dirname "$(command -v sort)"):$(dirname "$(command -v mktemp)"):$(dirname "$(command -v cp)"):$(dirname "$(command -v mkdir)"):$(dirname "$(command -v rm)")"

make_case() {
    local name="$1"
    local case_root="$tmp_root/$name"

    mkdir -p \
        "$case_root/openspec/specs/legitimate-boundaries" \
        "$case_root/openspec/changes/active-example" \
        "$case_root/openspec/changes/archive/2026-01-01-history" \
        "$case_root/openspec/changes/clean-canonical-spec-debt" \
        "$case_root/scripts"
    cp "$fixtures/valid-config.yaml" "$case_root/openspec/config.yaml"
    cp "$fixtures/positive-legitimate.md" \
        "$case_root/openspec/specs/legitimate-boundaries/spec.md"
    printf '%s' "$case_root"
}

expect_pass() {
    local case_root="$1"
    OPEN_SPEC_CURRENT_SURFACE_SKIP_INSTRUCTIONS=1 bash "$guard" "$case_root" >/dev/null
    PATH="$fallback_path" \
        OPEN_SPEC_CURRENT_SURFACE_SKIP_INSTRUCTIONS=1 bash "$guard" "$case_root" >/dev/null
}

expect_failure() {
    local case_root="$1"
    local rule="$2"
    local owning_path="$3"
    local output

    if output="$(OPEN_SPEC_CURRENT_SURFACE_SKIP_INSTRUCTIONS=1 bash "$guard" "$case_root" 2>&1)"; then
        printf 'expected guard failure for %s\n' "$rule" >&2
        exit 1
    fi
    if [[ "$output" != *"[$rule]"* || "$output" != *"$owning_path"* ]]; then
        printf 'guard failure did not identify %s and %s:\n%s\n' \
            "$rule" "$owning_path" "$output" >&2
        exit 1
    fi
}

positive="$(make_case positive)"
cp "$fixtures/positive-cleanup.md" \
    "$positive/openspec/changes/clean-canonical-spec-debt/tasks.md"
cp "$fixtures/positive-archive.md" \
    "$positive/openspec/changes/archive/2026-01-01-history/design.md"
cp "$fixtures/positive-allowlist.txt" \
    "$positive/scripts/openspec-current-surface-allowlist.txt"
expect_pass "$positive"

no_active="$(make_case no-active)"
rm -rf \
    "$no_active/openspec/changes/active-example" \
    "$no_active/openspec/changes/clean-canonical-spec-debt"
cp "$fixtures/positive-archive.md" \
    "$no_active/openspec/changes/archive/2026-01-01-history/design.md"
PATH="$fallback_path" bash "$guard" "$no_active" >/dev/null

purpose="$(make_case purpose)"
cp "$fixtures/negative-purpose.md" \
    "$purpose/openspec/specs/legitimate-boundaries/spec.md"
expect_failure "$purpose" "purpose-placeholder" \
    "openspec/specs/legitimate-boundaries/spec.md"

config="$(make_case config)"
cp "$fixtures/invalid-config.yaml" "$config/openspec/config.yaml"
expect_failure "$config" "unsupported-rule-key" "openspec/config.yaml"

deleted_source="$(make_case deleted-source)"
cp "$fixtures/negative-deleted-source.md" \
    "$deleted_source/openspec/changes/active-example/design.md"
expect_failure "$deleted_source" "deleted-source-reference" \
    "openspec/changes/active-example/design.md"

bridge="$(make_case bridge)"
cp "$fixtures/negative-bridge.md" \
    "$bridge/openspec/changes/active-example/design.md"
expect_failure "$bridge" "bridge-authorization" \
    "openspec/changes/active-example/design.md"

printf 'OpenSpec current-surface guard fixtures passed\n'
