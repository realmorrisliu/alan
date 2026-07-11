#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/harness/lib.sh
source "$script_dir/lib.sh"

usage() {
    cat <<'USAGE'
Usage:
  scripts/harness/run_autonomy_suite.sh [--ci-blocking]

Options:
  --ci-blocking   Run only scenarios marked as blocking for CI gates.
USAGE
}

mode="all"
if [[ "${1:-}" == "--ci-blocking" ]]; then
    mode="ci_blocking"
    shift
fi

if [[ $# -gt 0 ]]; then
    usage
    exit 2
fi

require_jq

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
artifact_root="$repo_root/target/harness/autonomy/latest"
harness_profile="${HARNESS_PROFILE:-default}"
mkdir -p "$artifact_root"
rm -rf "$artifact_root"/*
tag_file="$artifact_root/.kpi_tags.txt"
executed_scenarios_file="$artifact_root/.executed_scenarios.txt"
: >"$tag_file"
: >"$executed_scenarios_file"

fixtures=(
    "docs/harness/scenarios/autonomy/dedup_side_effect.json"
    "docs/harness/scenarios/governance/recovery_boundary.json"
)

suite_start_epoch="$(date +%s)"
total=0
passed=0
failed=0
skipped=0

resolve_fixture_path() {
    local base_fixture_rel="$1"
    local default_path="$repo_root/$base_fixture_rel"

    if [[ "$harness_profile" == "default" ]]; then
        printf "%s\n" "$default_path"
        return
    fi

    local rel_from_scenarios="${base_fixture_rel#docs/harness/scenarios/}"
    local profile_override_path="$repo_root/docs/harness/scenarios/profiles/$harness_profile/$rel_from_scenarios"
    if [[ ! -f "$profile_override_path" ]]; then
        echo "Missing profile fixture override for HARNESS_PROFILE=$harness_profile: $rel_from_scenarios" >&2
        return 1
    fi

    printf "%s\n" "$profile_override_path"
}

for fixture_rel in "${fixtures[@]}"; do
    fixture_path="$(resolve_fixture_path "$fixture_rel")"
    if [[ ! -f "$fixture_path" ]]; then
        echo "Missing harness fixture: $fixture_rel" >&2
        exit 1
    fi

    scenario_id="$(extract_json_string_field "$fixture_path" "id" || true)"
    scenario_cmd="$(extract_json_string_field "$fixture_path" "command" || true)"
    scenario_blocking="$(extract_json_bool_field "$fixture_path" "blocking" || true)"

    if [[ -z "$scenario_id" || -z "$scenario_cmd" || -z "$scenario_blocking" ]]; then
        echo "Invalid harness fixture format: $fixture_rel" >&2
        exit 1
    fi

    if [[ "$mode" == "ci_blocking" && "$scenario_blocking" != "true" ]]; then
        skipped=$((skipped + 1))
        continue
    fi

    scenario_dir="$artifact_root/${scenario_id//\//__}"
    mkdir -p "$scenario_dir"
    cp "$fixture_path" "$scenario_dir/input_script.json"
    record_executed_scenario "$scenario_id" "$executed_scenarios_file"
    record_fixture_kpi_tags "$fixture_path" "$tag_file"

    started_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    scenario_start_epoch="$(date +%s)"
    set +e
    validate_exact_cargo_filters "$repo_root" "$scenario_id" "$scenario_cmd" \
        >"$scenario_dir/precheck.log" 2>&1
    precheck_exit=$?
    set -e

    if [[ $precheck_exit -ne 0 ]]; then
        cp "$scenario_dir/precheck.log" "$scenario_dir/event_trace.log"
        exit_code=1
    else
        set +e
        (cd "$repo_root" && bash -lc "$scenario_cmd") >"$scenario_dir/event_trace.log" 2>&1
        exit_code=$?
        set -e
    fi
    finished_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    scenario_duration_secs=$(( $(date +%s) - scenario_start_epoch ))

    total=$((total + 1))
    if [[ $exit_code -eq 0 ]]; then
        passed=$((passed + 1))
        decision="pass"
        reason="command_exit_zero"
        assertion_passed=true
    else
        failed=$((failed + 1))
        decision="fail"
        reason="command_exit_nonzero"
        assertion_passed=false
    fi

    jq -cn \
        --arg scenario "$scenario_id" \
        --arg decision "$decision" \
        --arg reason "$reason" \
        --arg started_at "$started_at" \
        --arg finished_at "$finished_at" \
        --argjson duration_secs "$scenario_duration_secs" \
        '{scenario:$scenario,decision:$decision,reason:$reason,started_at:$started_at,finished_at:$finished_at,duration_secs:$duration_secs}' \
        >"$scenario_dir/decision_trace.jsonl"

    jq -cn \
        --arg scenario "$scenario_id" \
        --argjson passed "$assertion_passed" \
        --argjson exit_code "$exit_code" \
        --arg detail "$scenario_cmd" \
        '{scenario:$scenario,passed:$passed,exit_code:$exit_code,assertions:[{name:"command_exit_zero",passed:$passed,detail:$detail}]}' \
        >"$scenario_dir/assertion_report.json"

    if [[ $exit_code -ne 0 ]]; then
        echo "Scenario failed: $scenario_id"
        echo "  command: $scenario_cmd"
        echo "  artifact: $scenario_dir"
    fi
done

suite_duration_secs=$(( $(date +%s) - suite_start_epoch ))
if [[ $total -gt 0 ]]; then
    pass_rate_percent="$(awk "BEGIN { printf \"%.2f\", ($passed / $total) * 100 }")"
else
    pass_rate_percent="0.00"
fi

executed_scenarios_json="$(build_json_string_array "$executed_scenarios_file")"
kpi_tag_counts_json="$(build_kpi_tag_counts_json "$tag_file")"

jq -cn \
    --arg suite "autonomy" \
    --arg mode "$mode" \
    --arg profile "$harness_profile" \
    --argjson total "$total" \
    --argjson passed "$passed" \
    --argjson failed "$failed" \
    --argjson skipped "$skipped" \
    --argjson pass_rate_percent "$pass_rate_percent" \
    --argjson duration_secs "$suite_duration_secs" \
    --argjson executed_scenarios "$executed_scenarios_json" \
    --argjson kpi_tag_counts "$kpi_tag_counts_json" \
    '{suite:$suite,mode:$mode,profile:$profile,total:$total,passed:$passed,failed:$failed,skipped:$skipped,pass_rate_percent:$pass_rate_percent,duration_secs:$duration_secs,executed_scenarios:$executed_scenarios,kpi_tag_counts:$kpi_tag_counts}' \
    >"$artifact_root/kpi.json"

echo "Autonomy harness summary:"
echo "  mode: $mode"
echo "  profile: $harness_profile"
echo "  total: $total"
echo "  passed: $passed"
echo "  failed: $failed"
echo "  skipped: $skipped"
echo "  pass_rate_percent: $pass_rate_percent"
echo "  artifacts: $artifact_root"

if [[ $failed -gt 0 ]]; then
    exit 1
fi
