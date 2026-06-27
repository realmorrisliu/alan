#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../../.." && pwd)"
package_root="$repo_root/crates/agent-engine/skills/repo-coding"
validator="$package_root/scripts/validate_delivery_contract.sh"
fixtures_root="$package_root/evals/files"

valid_fixtures=(
    "$fixtures_root/delivery_contract_valid_mixed.json"
    "$fixtures_root/delivery_contract_valid_environment_blocked.json"
)

invalid_fixtures=(
    "$fixtures_root/delivery_contract_invalid_passed_with_failure_exit.json"
    "$fixtures_root/delivery_contract_invalid_test_only_without_reason.json"
)

for fixture in "${valid_fixtures[@]}"; do
    bash "$validator" "$fixture" >/dev/null
done

for fixture in "${invalid_fixtures[@]}"; do
    if bash "$validator" "$fixture" >/dev/null 2>&1; then
        echo "Expected delivery contract validation to fail: $fixture" >&2
        exit 1
    fi
done

echo "Repo-worker delivery-contract examples validated."
