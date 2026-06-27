#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'USAGE'
Usage:
  crates/agent-engine/skills/repo-coding/scripts/check_evaluator_boundaries.sh <evaluator-cases.json>
USAGE
}

if [[ $# -ne 1 ]]; then
    usage
    exit 2
fi

if ! command -v jq >/dev/null 2>&1; then
    echo "jq is required to validate repo-worker evaluator boundaries." >&2
    exit 1
fi

cases_path="$1"
if [[ ! -f "$cases_path" ]]; then
    echo "Missing evaluator cases: $cases_path" >&2
    exit 1
fi

mismatches="$(
    jq -rc '
      def decide_mode:
        if .evaluator_used then
          "used"
        elif (.repeated_verification_failures > 1)
          or (.deterministic_checks_available | not)
          or .ui_heavy_task
          or (.risky_refactor == "large")
        then
          "recommended"
        else
          "not_needed"
        end;

      .cases[]
      | {id, expected_mode, actual_mode: decide_mode}
      | select(.expected_mode != .actual_mode)
    ' "$cases_path"
)"

if [[ -n "$mismatches" ]]; then
    echo "Repo-worker evaluator-boundary mismatches:" >&2
    printf '%s\n' "$mismatches" >&2
    exit 1
fi

case_count="$(jq -r '.cases | length' "$cases_path")"
echo "Repo-worker evaluator boundaries validated for ${case_count} cases."
