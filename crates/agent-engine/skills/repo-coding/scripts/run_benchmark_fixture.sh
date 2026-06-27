#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'USAGE'
Usage:
  scripts/run_benchmark_fixture.sh candidate|baseline <fixtures-json> <case-id>
USAGE
}

if [[ $# -ne 3 ]]; then
    usage
    exit 2
fi

if ! command -v jq >/dev/null 2>&1; then
    echo "jq is required to run repo-coding benchmark fixtures." >&2
    exit 1
fi

variant="$1"
fixtures_path="$2"
case_id="$3"

if [[ "$variant" != "candidate" && "$variant" != "baseline" ]]; then
    usage
    exit 2
fi

case_payload="$(jq -cer --arg id "$case_id" '.cases[] | select(.id == $id)' "$fixtures_path")"
goal="$(printf "%s\n" "$case_payload" | jq -r '.goal')"
expected_route="$(printf "%s\n" "$case_payload" | jq -r '.expected_route')"
expected_outcome="$(printf "%s\n" "$case_payload" | jq -r '.expected_outcome')"
expected_handles_json="$(printf "%s\n" "$case_payload" | jq -c '.expected_handles')"

case "$variant:$case_id" in
    candidate:single_repo_fix)
        route="repo_worker"
        outcome="bounded_delivery"
        handles_json='["workspace","approval_scope","plan","conversation_snapshot"]'
        summary="Candidate keeps the home-root steward responsible for routing, then launches a bounded repo worker for the fix."
        ;;
    baseline:single_repo_fix)
        route="steward_inline_shell"
        outcome="inline_edit_loop"
        handles_json='[]'
        summary="Baseline collapses the repo-local coding loop into the parent steward session."
        ;;
    candidate:cross_repo_orchestration)
        route="steward_multi_repo"
        outcome="sequenced_multi_repo_delivery"
        handles_json='["approval_scope","plan","conversation_snapshot"]'
        summary="Candidate keeps multi-repo coordination in the steward and treats repo execution as separate bounded launches."
        ;;
    baseline:cross_repo_orchestration)
        route="single_repo_shell"
        outcome="mixed_scope_inline_delivery"
        handles_json='["workspace"]'
        summary="Baseline blurs multiple repositories into one repo-local shell workflow."
        ;;
    candidate:owner_boundary_escalation)
        route="owner_boundary_escalation"
        outcome="escalate_for_owner_approval"
        handles_json='["approval_scope"]'
        summary="Candidate routes deploy-workflow edits to owner-boundary escalation instead of a silent fast path."
        ;;
    baseline:owner_boundary_escalation)
        route="repo_worker_fast_path"
        outcome="unsafe_publish_attempt"
        handles_json='["workspace"]'
        summary="Baseline attempts to keep deploy-workflow changes inside a normal repo-worker fast path."
        ;;
    *)
        echo "Unknown benchmark fixture combination: $variant $case_id" >&2
        exit 1
        ;;
esac

jq -cn \
    --arg case_id "$case_id" \
    --arg variant "$variant" \
    --arg goal "$goal" \
    --arg route "$route" \
    --arg outcome "$outcome" \
    --arg expected_route "$expected_route" \
    --arg expected_outcome "$expected_outcome" \
    --arg summary "$summary" \
    --argjson handles "$handles_json" \
    --argjson expected_handles "$expected_handles_json" \
    '{
        passed: true,
        case_id: $case_id,
        variant: $variant,
        goal: $goal,
        route: $route,
        outcome: $outcome,
        handles: $handles,
        expected_route: $expected_route,
        expected_outcome: $expected_outcome,
        expected_handles: $expected_handles,
        route_matches_expectation: ($route == $expected_route),
        outcome_matches_expectation: ($outcome == $expected_outcome),
        handles_match_expectation: ($handles == $expected_handles),
        summary: $summary
    }'
