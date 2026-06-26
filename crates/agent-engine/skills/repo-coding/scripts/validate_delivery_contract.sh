#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'USAGE'
Usage:
  crates/agent-engine/skills/repo-coding/scripts/validate_delivery_contract.sh <delivery-contract.json>
USAGE
}

if [[ $# -ne 1 ]]; then
    usage
    exit 2
fi

if ! command -v jq >/dev/null 2>&1; then
    echo "jq is required to validate the repo-worker delivery contract." >&2
    exit 1
fi

contract_path="$1"
if [[ ! -f "$contract_path" ]]; then
    echo "Missing delivery contract: $contract_path" >&2
    exit 1
fi

jq -e '
  def one_of($value; $choices):
    $choices | index($value) != null;

  def is_test_path($path):
    $path | test("(^|/)(tests?|specs?|__tests__)(/|$)|(^|/)[^/]+(_test|_spec)\\.[^/]+$|(^|/)[^/]+\\.(test|spec)\\.[^/]+$");

  def attempted_entries($entries):
    $entries | map(select(.status != "not_run"));

  def expected_overall_status($entries):
    attempted_entries($entries) as $attempted
    | ($attempted | map(.status) | unique) as $attempted_statuses
    | if ($entries | length) == 0 or ($attempted | length) == 0 then
        "not_attempted"
      elif ($entries | length) > ($attempted | length) then
        "mixed"
      elif ($attempted_statuses | length) > 1 then
        "mixed"
      elif ($attempted_statuses[0] // "") == "passed" then
        "passed"
      elif ($attempted_statuses[0] // "") == "failed" then
        "failed"
      elif ($attempted_statuses[0] // "") == "environment_blocked" then
        "environment_blocked"
      elif ($attempted_statuses[0] // "") == "blocked" then
        "blocked"
      else
        "mixed"
      end;

  (.status | type == "string") and
  (.status as $status | one_of($status; ["completed", "blocked", "failed"])) and
  (.summary | type == "string" and length > 0) and
  (.changed_files | type == "array" and all(.[]; type == "string")) and
  (.behavioral_guards | type == "array" and length > 0 and all(.[]; type == "string" and length > 0)) and
  (
    (.changed_files | any(.[]; is_test_path(.))) as $has_test_changes
    | if $has_test_changes then
        (.test_change_reason | type == "string" and length > 0)
      else
        ((has("test_change_reason") | not) or (.test_change_reason == null) or (.test_change_reason | type == "string"))
      end
  ) and
  (.verification | type == "object") and
  (.verification.entries | type == "array") and
  all(
    .verification.entries[];
    (.command | type == "string" and length > 0) and
    (.scope | type == "string") and
    (.scope as $scope | one_of($scope; ["targeted", "broader"])) and
    (.status | type == "string") and
    (.status as $status | one_of($status; ["passed", "failed", "blocked", "environment_blocked", "not_run"])) and
    (
      if .status == "passed" then
        (.exit_code | type == "number" and . == 0)
      elif .status == "failed" or .status == "environment_blocked" then
        (.exit_code | type == "number" and . != 0)
      else
        .exit_code == null
      end
    ) and
    (.summary | type == "string" and length > 0)
  ) and
  (.verification.overall_status | type == "string") and
  (.verification.overall_status as $status | one_of($status; ["passed", "failed", "blocked", "environment_blocked", "mixed", "not_attempted"])) and
  (.verification.verification_attempted | type == "boolean") and
  (.verification.attempted_count | type == "number") and
  (.verification.passed_count | type == "number") and
  (.verification.failed_count | type == "number") and
  (.verification.environment_blocked_count | type == "number") and
  (.verification.blocked_count | type == "number") and
  (.verification.not_run_count | type == "number") and
  (.verification.all_passed | type == "boolean") and
  (
    (.verification.entries // []) as $entries
    | attempted_entries($entries) as $attempted
    | ($entries | map(select(.status == "passed")) | length) as $passed_count
    | ($entries | map(select(.status == "failed")) | length) as $failed_count
    | ($entries | map(select(.status == "environment_blocked")) | length) as $environment_blocked_count
    | ($entries | map(select(.status == "blocked")) | length) as $blocked_count
    | ($entries | map(select(.status == "not_run")) | length) as $not_run_count
    | ($attempted | length) as $attempted_count
    | .verification.attempted_count == $attempted_count
    and .verification.passed_count == $passed_count
    and .verification.failed_count == $failed_count
    and .verification.environment_blocked_count == $environment_blocked_count
    and .verification.blocked_count == $blocked_count
    and .verification.not_run_count == $not_run_count
    and .verification.verification_attempted == ($attempted_count > 0)
    and .verification.all_passed == ($attempted_count > 0 and $passed_count == $attempted_count)
    and .verification.overall_status == expected_overall_status($entries)
  ) and
  (.residual_risks | type == "array" and all(.[]; type == "string")) and
  (.evaluator | type == "object") and
  (.evaluator.mode | type == "string") and
  (.evaluator.mode as $mode | one_of($mode; ["not_needed", "recommended", "used"])) and
  (.evaluator.reason | type == "string" and length > 0)
' "$contract_path" >/dev/null

echo "Repo-worker delivery contract is valid: $contract_path"
