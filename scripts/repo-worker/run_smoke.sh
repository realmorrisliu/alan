#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'USAGE'
Usage:
  scripts/repo-worker/run_smoke.sh [--mode local|ci]

Modes:
  local  Run package checks and deterministic repo-worker loop (default).
  ci     Same checks with CI-friendly artifact output.
USAGE
}

mode="local"
if [[ "${1:-}" == "--mode" ]]; then
    if [[ -z "${2:-}" ]]; then
        usage
        exit 2
    fi
    mode="$2"
    shift 2
fi

if [[ $# -gt 0 ]]; then
    usage
    exit 2
fi

case "$mode" in
    local|ci) ;;
    *)
        usage
        exit 2
        ;;
esac

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
package_root="$repo_root/crates/agent-engine/skills/repo-coding"
child_root="$package_root/agents/repo-worker"
artifact_root="$repo_root/target/repo-worker/smoke/latest"
workspace_dir="$artifact_root/workspace"
trace_file="$artifact_root/loop_trace.log"

required_files=(
    "SKILL.md"
    "skill.yaml"
    "agents/openai.yaml"
    "references/package.md"
    "references/delivery_contract.md"
    "references/evaluator_boundary.md"
    "evals/README.md"
    "evals/evals.json"
    "evals/evaluator_cases.json"
    "evals/files/delivery_contract_invalid_passed_with_failure_exit.json"
    "evals/files/delivery_contract_invalid_test_only_without_reason.json"
    "evals/files/delivery_contract_valid_mixed.json"
    "evals/files/delivery_contract_valid_environment_blocked.json"
    "evals/files/benchmark_cases.json"
    "scripts/validate_delivery_contract.sh"
    "scripts/check_delivery_contract_examples.sh"
    "scripts/check_evaluator_boundaries.sh"
    "scripts/run_benchmark_fixture.sh"
    "scripts/grade_benchmark_case.sh"
    "agents/repo-worker/agent.toml"
    "agents/repo-worker/persona/ROLE.md"
    "agents/repo-worker/policy.yaml"
    "agents/repo-worker/skills/decompose/SKILL.md"
    "agents/repo-worker/skills/edit-verify/SKILL.md"
    "agents/repo-worker/skills/deliver/SKILL.md"
    "agents/repo-worker/extensions/code-index.yaml"
    "agents/repo-worker/extensions/test-analyzer.yaml"
    "agents/repo-worker/extensions/pr-helper.yaml"
)

missing=0
for rel in "${required_files[@]}"; do
    if [[ ! -f "$package_root/$rel" ]]; then
        echo "Missing repo-worker package artifact: crates/agent-engine/skills/repo-coding/$rel" >&2
        missing=1
    fi
done
if [[ $missing -ne 0 ]]; then
    exit 1
fi

while IFS= read -r skill_file; do
    if ! grep -q '^---$' "$skill_file"; then
        echo "Invalid skill frontmatter (missing delimiter): $skill_file" >&2
        exit 1
    fi
    if ! grep -q '^name:' "$skill_file"; then
        echo "Invalid skill frontmatter (missing name): $skill_file" >&2
        exit 1
    fi
    if ! grep -q '^description:' "$skill_file"; then
        echo "Invalid skill frontmatter (missing description): $skill_file" >&2
        exit 1
    fi
done < <(find "$child_root/skills" -name SKILL.md -type f | sort)

rm -rf "$artifact_root"
mkdir -p "$workspace_dir/src"
cp -R "$package_root" "$artifact_root/package_snapshot"

cat >"$workspace_dir/Cargo.toml" <<'CARGO'
[package]
name = "repo-worker-loop-fixture"
version = "0.1.0"
edition = "2024"

[lib]
path = "src/lib.rs"

[workspace]
CARGO

cat >"$workspace_dir/src/lib.rs" <<'RS'
pub fn add(a: i32, b: i32) -> i32 {
    a - b
}

#[cfg(test)]
mod tests {
    use super::add;

    #[test]
    fn add_returns_sum() {
        assert_eq!(add(2, 3), 5);
    }
}
RS

cat >"$artifact_root/input_script.json" <<JSON
{"id":"repo_worker/minimum_loop_smoke","mode":"$mode","steps":["receive_task","plan","edit","verify","deliver"]}
JSON

: >"$trace_file"

echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] receive_task: fix add() implementation" >>"$trace_file"
cat >"$artifact_root/plan.md" <<'PLAN'
1. Locate failing implementation in src/lib.rs.
2. Replace subtraction with addition.
3. Run cargo test.
4. Emit delivery summary.
PLAN

echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] plan: wrote plan.md" >>"$trace_file"
perl -0pi -e 's/a - b/a + b/' "$workspace_dir/src/lib.rs"
echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] edit: patched src/lib.rs" >>"$trace_file"

set +e
(cd "$workspace_dir" && cargo test --quiet) >"$artifact_root/verify.log" 2>&1
verify_exit=$?
set -e

echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] verify: cargo test exit=$verify_exit" >>"$trace_file"

verified=false
if [[ $verify_exit -eq 0 ]]; then
    verified=true
fi

change_line="$(grep -n "a + b" "$workspace_dir/src/lib.rs" | head -n1 | cut -d: -f1 || true)"
edit_applied=false
if [[ -n "$change_line" ]]; then
    edit_applied=true
fi

delivery_status="completed"
evaluator_mode="not_needed"
evaluator_reason="A targeted deterministic check passed on a small repo-local bug fix, so evaluator support was not needed."
residual_risks_json='[]'
verification_status="passed"
verification_summary="Targeted cargo test passed after patching add()."
if [[ $verify_exit -ne 0 ]]; then
    delivery_status="failed"
    evaluator_mode="not_needed"
    evaluator_reason="Targeted verification failed once, but evaluator support is only recommended after repeated failures or when scope/risk increases."
    residual_risks_json='["Targeted verification failed; inspect verify.log before expanding scope."]'
    verification_status="failed"
    verification_summary="Targeted cargo test failed after the patch; broader checks were intentionally skipped."
fi

cat >"$artifact_root/delivery_contract.json" <<JSON
{
  "status": "$delivery_status",
  "summary": "Fixed add() to return addition instead of subtraction.",
  "changed_files": [
    "src/lib.rs"
  ],
  "behavioral_guards": [
    "The public add(a, b) function should return the mathematical sum.",
    "The add_returns_sum unit test remains the nearby deterministic behavior guard."
  ],
  "verification": {
    "overall_status": "$verification_status",
    "verification_attempted": true,
    "attempted_count": 1,
    "passed_count": $([[ "$verification_status" == "passed" ]] && echo 1 || echo 0),
    "failed_count": $([[ "$verification_status" == "failed" ]] && echo 1 || echo 0),
    "environment_blocked_count": 0,
    "blocked_count": 0,
    "not_run_count": 0,
    "all_passed": $([[ "$verification_status" == "passed" ]] && echo true || echo false),
    "entries": [
      {
        "command": "cargo test --quiet",
        "scope": "targeted",
        "status": "$verification_status",
        "exit_code": $verify_exit,
        "summary": "$verification_summary"
      }
    ]
  },
  "residual_risks": $residual_risks_json,
  "evaluator": {
    "mode": "$evaluator_mode",
    "reason": "$evaluator_reason"
  }
}
JSON

set +e
bash "$package_root/scripts/validate_delivery_contract.sh" \
    "$artifact_root/delivery_contract.json" \
    >"$artifact_root/delivery_contract.log" 2>&1
delivery_contract_exit=$?
bash "$package_root/scripts/check_evaluator_boundaries.sh" \
    "$package_root/evals/evaluator_cases.json" \
    >"$artifact_root/evaluator_boundary.log" 2>&1
evaluator_boundary_exit=$?
bash "$package_root/scripts/check_delivery_contract_examples.sh" \
    >"$artifact_root/delivery_contract_examples.log" 2>&1
delivery_contract_examples_exit=$?
set -e

delivery_contract_valid=false
if [[ $delivery_contract_exit -eq 0 ]]; then
    delivery_contract_valid=true
fi

evaluator_boundaries_valid=false
if [[ $evaluator_boundary_exit -eq 0 ]]; then
    evaluator_boundaries_valid=true
fi

delivery_contract_examples_valid=false
if [[ $delivery_contract_examples_exit -eq 0 ]]; then
    delivery_contract_examples_valid=true
fi

smoke_passed=false
if [[ "$verified" == "true" && "$delivery_contract_valid" == "true" && "$evaluator_boundaries_valid" == "true" && "$delivery_contract_examples_valid" == "true" ]]; then
    smoke_passed=true
fi

cat >"$artifact_root/delivery_summary.md" <<SUMMARY
# Repo Worker Smoke Summary

- status: $delivery_status
- mode: $mode
- edit_applied: $edit_applied
- edit_line: ${change_line:-unknown}
- verify_exit: $verify_exit
- verified: $verified
- evaluator_mode: $evaluator_mode
- delivery_contract_valid: $delivery_contract_valid
- delivery_contract_examples_valid: $delivery_contract_examples_valid
- evaluator_boundaries_valid: $evaluator_boundaries_valid
SUMMARY

cat >"$artifact_root/assertion_report.json" <<ASSERT
{"scenario":"repo_worker/minimum_loop_smoke","passed":$smoke_passed,"assertions":[{"name":"package_present","passed":true},{"name":"edit_applied","passed":$edit_applied},{"name":"verify_command_exit_zero","passed":$verified},{"name":"delivery_contract_valid","passed":$delivery_contract_valid},{"name":"delivery_contract_examples_valid","passed":$delivery_contract_examples_valid},{"name":"evaluator_boundaries_valid","passed":$evaluator_boundaries_valid}]}
ASSERT

cat >"$artifact_root/summary.json" <<REPORT
{"mode":"$mode","status":"$delivery_status","verify_exit":$verify_exit,"verified":$verified,"delivery_contract_valid":$delivery_contract_valid,"delivery_contract_examples_valid":$delivery_contract_examples_valid,"evaluator_boundaries_valid":$evaluator_boundaries_valid,"passed":$smoke_passed,"artifact_root":"target/repo-worker/smoke/latest"}
REPORT

echo "Repo worker smoke summary:"
echo "  mode: $mode"
echo "  verify_exit: $verify_exit"
echo "  delivery_contract_valid: $delivery_contract_valid"
echo "  delivery_contract_examples_valid: $delivery_contract_examples_valid"
echo "  evaluator_boundaries_valid: $evaluator_boundaries_valid"
echo "  passed: $smoke_passed"
echo "  artifacts: $artifact_root"

if [[ $verify_exit -ne 0 || $delivery_contract_exit -ne 0 || $delivery_contract_examples_exit -ne 0 || $evaluator_boundary_exit -ne 0 ]]; then
    exit 1
fi
