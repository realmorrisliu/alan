#!/usr/bin/env bash
set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
    echo "jq is required to grade repo-coding benchmark fixtures." >&2
    exit 1
fi

candidate_artifact="${ALAN_SKILL_EVAL_CANDIDATE_ARTIFACT:-}"
baseline_artifact="${ALAN_SKILL_EVAL_BASELINE_ARTIFACT:-}"
case_id="${ALAN_SKILL_EVAL_CASE_ID:-unknown}"
package_root="${ALAN_SKILL_EVAL_PACKAGE_ROOT:-$PWD}"

discover_repo_root() {
    local dir="$package_root"
    while [[ "$dir" != "/" ]]; do
        if [[ -d "$dir/.git" || -f "$dir/.git" ]]; then
            printf "%s\n" "$dir"
            return 0
        fi
        dir="$(dirname "$dir")"
    done
    printf "%s\n" "$package_root"
}

repo_root="$(discover_repo_root)"

resolve_eval_artifact() {
    local path="$1"
    if [[ -z "$path" ]]; then
        return 0
    fi
    if [[ "$path" == /* ]]; then
        printf "%s\n" "$path"
    elif [[ -f "$path" ]]; then
        printf "%s\n" "$path"
    elif [[ -f "$package_root/$path" ]]; then
        printf "%s\n" "$package_root/$path"
    else
        printf "%s/%s\n" "$repo_root" "$path"
    fi
}

candidate_artifact_resolved="$(resolve_eval_artifact "$candidate_artifact")"
baseline_artifact_resolved="$(resolve_eval_artifact "$baseline_artifact")"

if [[ -z "$candidate_artifact_resolved" || ! -f "$candidate_artifact_resolved" ]]; then
    echo "Missing candidate artifact for repo-coding benchmark grading." >&2
    exit 1
fi

candidate_match="$(
    jq -r '
        (
            (.json_output.route_matches_expectation // .route_matches_expectation // false)
            and (.json_output.outcome_matches_expectation // .outcome_matches_expectation // false)
            and (.json_output.handles_match_expectation // .handles_match_expectation // false)
        )
    ' "$candidate_artifact_resolved"
)"
baseline_match=false
if [[ -n "$baseline_artifact_resolved" && -f "$baseline_artifact_resolved" ]]; then
    baseline_match="$(
        jq -r '
            (
                (.json_output.route_matches_expectation // .route_matches_expectation // false)
                and (.json_output.outcome_matches_expectation // .outcome_matches_expectation // false)
                and (.json_output.handles_match_expectation // .handles_match_expectation // false)
            )
        ' "$baseline_artifact_resolved"
    )"
fi

score="0.0"
if [[ "$candidate_match" == "true" && "$baseline_match" != "true" ]]; then
    score="1.0"
elif [[ "$candidate_match" == "true" ]]; then
    score="0.75"
fi

jq -cn \
    --arg case_id "$case_id" \
    --arg candidate_artifact "$candidate_artifact_resolved" \
    --arg baseline_artifact "$baseline_artifact_resolved" \
    --argjson passed "$candidate_match" \
    --argjson baseline_matches "$baseline_match" \
    --argjson score "$score" \
    '{
        passed: $passed,
        score: $score,
        case_id: $case_id,
        candidate_artifact: $candidate_artifact,
        baseline_artifact: $baseline_artifact,
        baseline_matches_expectation: $baseline_matches,
        summary: (
            if $passed and ($baseline_matches | not) then
                "Candidate preserves the steward/worker routing contract while the baseline does not."
            elif $passed then
                "Candidate matches the expected routing contract."
            else
                "Candidate does not yet satisfy the expected routing contract."
            end
        )
    }'
