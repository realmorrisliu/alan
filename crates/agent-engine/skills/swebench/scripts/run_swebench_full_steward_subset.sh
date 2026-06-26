#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'USAGE'
Usage:
  crates/agent-engine/skills/swebench/scripts/run_swebench_full_steward_subset.sh <suite-json> [--output-dir <dir>] [--agent <name>] [--keep-session] [--score-official]

Suite JSON fields:
  suite           Optional suite name (default: swebench_lite_curated).
  dataset         Optional human label (default: SWE-bench Lite).
  dataset_name    Optional official harness dataset name (default: princeton-nlp/SWE-bench_Lite).
  max_workers     Optional official harness worker count hint (default: 8).
  agent_name      Optional agent override for all cases in the suite.
  cases           Required array of case-json paths, absolute or relative to the suite file.

This runner calls `run_swebench_full_steward_case.sh` for each case, aggregates
single-case outputs into one suite directory, writes a unified
`predictions.jsonl`, generates `score_with_official_harness.sh`, and can
optionally invoke the official SWE-bench harness immediately.
USAGE
}

resolve_path() {
    local raw_path="$1"
    local base_dir="$2"
    if [[ "$raw_path" == /* ]]; then
        printf '%s\n' "$raw_path"
    else
        printf '%s\n' "$base_dir/$raw_path"
    fi
}

suite_json=""
output_dir=""
cli_agent_name=""
keep_session=false
score_official=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --output-dir)
            if [[ -z "${2:-}" ]]; then
                usage
                exit 2
            fi
            output_dir="$2"
            shift 2
            ;;
        --agent)
            if [[ -z "${2:-}" ]]; then
                usage
                exit 2
            fi
            cli_agent_name="$2"
            shift 2
            ;;
        --keep-session)
            keep_session=true
            shift
            ;;
        --score-official)
            score_official=true
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        -*)
            usage
            exit 2
            ;;
        *)
            if [[ -n "$suite_json" ]]; then
                usage
                exit 2
            fi
            suite_json="$1"
            shift
            ;;
    esac
done

if [[ -z "$suite_json" ]]; then
    usage
    exit 2
fi

if ! command -v jq >/dev/null 2>&1; then
    echo "Missing required command: jq" >&2
    exit 1
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
package_root="$(cd "$script_dir/.." && pwd)"
repo_root="$(cd "$package_root/../../../.." && pwd)"
suite_json="$(cd "$(dirname "$suite_json")" && pwd)/$(basename "$suite_json")"
suite_dirname="$(dirname "$suite_json")"

if [[ ! -f "$suite_json" ]]; then
    echo "Suite file not found: $suite_json" >&2
    exit 1
fi

suite_name="$(jq -r '.suite // "swebench_lite_curated"' "$suite_json")"
dataset_label="$(jq -r '.dataset // "SWE-bench Lite"' "$suite_json")"
dataset_name="$(jq -r '.dataset_name // "princeton-nlp/SWE-bench_Lite"' "$suite_json")"
max_workers="$(jq -r '.max_workers // 8' "$suite_json")"
suite_agent_name="$(jq -r '.agent_name // empty' "$suite_json")"
case_count="$(jq '.cases | length' "$suite_json")"

if (( case_count == 0 )); then
    echo "Suite file must define at least one case path in .cases." >&2
    exit 1
fi

effective_agent_name="$cli_agent_name"
if [[ -z "$effective_agent_name" ]]; then
    effective_agent_name="$suite_agent_name"
fi

if [[ -z "$output_dir" ]]; then
    output_dir="$repo_root/target/benchmarks/swebench_lite/suites/$suite_name"
fi
mkdir -p "$output_dir/cases"

predictions_jsonl="$output_dir/predictions.jsonl"
suite_run_file="$output_dir/run.json"
suite_kpi_file="$output_dir/kpi.json"
benchmark_file="$output_dir/benchmark.json"
case_results_jsonl="$output_dir/case_results.jsonl"
scoring_script="$output_dir/score_with_official_harness.sh"
official_harness_manifest="$output_dir/official_harness_run.json"

: >"$predictions_jsonl"
: >"$case_results_jsonl"

started_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
start_epoch="$(date +%s)"
total=0
passed=0
failed=0
official_harness_requested=false
official_harness_exit_code="null"
official_harness_summary_json="null"

case_runner="${ALAN_SWEBENCH_CASE_RUNNER:-$script_dir/run_swebench_full_steward_case.sh}"
score_runner="${ALAN_SWEBENCH_SCORE_RUNNER:-$script_dir/score_swebench_predictions.sh}"

if [[ ! -f "$case_runner" ]]; then
    echo "Case runner not found: $case_runner" >&2
    exit 1
fi

if [[ ! -f "$score_runner" ]]; then
    echo "Score runner not found: $score_runner" >&2
    exit 1
fi

while IFS= read -r case_path_raw; do
    if [[ -z "$case_path_raw" ]]; then
        continue
    fi
    case_path="$(resolve_path "$case_path_raw" "$suite_dirname")"
    if [[ ! -f "$case_path" ]]; then
        echo "Missing suite case file: $case_path" >&2
        exit 1
    fi

    instance_id="$(jq -r '.instance_id // empty' "$case_path")"
    if [[ -z "$instance_id" ]]; then
        echo "Case file is missing instance_id: $case_path" >&2
        exit 1
    fi

    case_output_dir="$output_dir/cases/$instance_id"
    mkdir -p "$case_output_dir"

    case_cmd=(bash "$case_runner" "$case_path" --output-dir "$case_output_dir")
    if [[ -n "$effective_agent_name" ]]; then
        case_cmd+=(--agent "$effective_agent_name")
    fi
    if [[ "$keep_session" == true ]]; then
        case_cmd+=(--keep-session)
    fi

    set +e
    "${case_cmd[@]}" >"$case_output_dir/stdout.log" 2>&1
    case_exit=$?
    set -e

    total=$((total + 1))
    if [[ $case_exit -eq 0 ]]; then
        passed=$((passed + 1))
    else
        failed=$((failed + 1))
    fi

    if [[ -f "$case_output_dir/prediction.json" ]]; then
        jq -c . "$case_output_dir/prediction.json" >>"$predictions_jsonl"
    fi

    if [[ -f "$case_output_dir/run.json" ]]; then
        jq -cn \
            --arg case_path "$case_path" \
            --argjson exit_code "$case_exit" \
            --slurpfile run "$case_output_dir/run.json" \
            '{case_path:$case_path,exit_code:$exit_code,run:$run[0]}' \
            >>"$case_results_jsonl"
    else
        jq -cn \
            --arg case_path "$case_path" \
            --arg instance_id "$instance_id" \
            --argjson exit_code "$case_exit" \
            '{case_path:$case_path,instance_id:$instance_id,exit_code:$exit_code,run:null}' \
            >>"$case_results_jsonl"
    fi
done < <(jq -r '.cases[]' "$suite_json")

finished_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
duration_secs="$(( $(date +%s) - start_epoch ))"
pass_rate_percent="$(awk "BEGIN { printf \"%.2f\", ($passed / $total) * 100 }")"
total_escalation_count="$(jq -s '[.[] | (.run.escalation_count // 0)] | add // 0' "$case_results_jsonl")"

cat >"$scoring_script" <<EOF
#!/usr/bin/env bash
set -euo pipefail

# Optional:
#   export ALAN_SWEBENCH_HARNESS_PYTHON_BIN=/absolute/path/to/harness/python
#
bash "$score_runner" "$predictions_jsonl" \\
  --dataset-name "$dataset_name" \\
  --max-workers "$max_workers" \\
  --run-id "${suite_name}_\$(date -u +%Y%m%dT%H%M%SZ)" \\
  --work-dir "$output_dir" \\
  --manifest-file "$official_harness_manifest"
EOF
chmod +x "$scoring_script"

if [[ "$score_official" == true ]]; then
    official_harness_requested=true
    set +e
    bash "$score_runner" "$predictions_jsonl" \
      --dataset-name "$dataset_name" \
      --max-workers "$max_workers" \
      --run-id "${suite_name}_$(date -u +%Y%m%dT%H%M%SZ)" \
      --work-dir "$output_dir" \
      --manifest-file "$official_harness_manifest"
    official_harness_exit_code=$?
    set -e
    if [[ -f "$official_harness_manifest" ]]; then
        official_harness_summary_json="$(jq -c . "$official_harness_manifest")"
    fi
fi

if [[ "$official_harness_summary_json" != "null" ]]; then
    official_instance_results_jsonl="$(printf '%s' "$official_harness_summary_json" | jq -r '.instance_results_jsonl // empty')"
    if [[ -n "$official_instance_results_jsonl" && -f "$official_instance_results_jsonl" ]]; then
        tmp_case_results_jsonl="$output_dir/case_results.with_official.tmp.jsonl"
        : >"$tmp_case_results_jsonl"
        while IFS= read -r case_record; do
            if [[ -z "$case_record" ]]; then
                continue
            fi
            case_instance_id="$(printf '%s' "$case_record" | jq -r '.run.instance_id // .instance_id // empty')"
            official_result="$(jq -c --arg instance_id "$case_instance_id" 'select(.instance_id == $instance_id)' "$official_instance_results_jsonl" | head -n 1)"
            if [[ -n "$official_result" ]]; then
                case_official_result_file="$output_dir/cases/$case_instance_id/official_harness_instance_result.json"
                printf '%s\n' "$official_result" >"$case_official_result_file"
                jq -cn \
                    --argjson record "$case_record" \
                    --arg case_official_result_file "$case_official_result_file" \
                    --argjson official "$official_result" \
                    '$record + {
                        official_harness_result_file: $case_official_result_file,
                        official_harness_result: $official
                    }' >>"$tmp_case_results_jsonl"
            else
                jq -cn \
                    --argjson record "$case_record" \
                    '$record + {
                        official_harness_result_file: null,
                        official_harness_result: null
                    }' >>"$tmp_case_results_jsonl"
            fi
        done <"$case_results_jsonl"
        mv "$tmp_case_results_jsonl" "$case_results_jsonl"
    fi
fi

jq -n \
    --arg suite "$suite_name" \
    --arg dataset "$dataset_label" \
    --arg dataset_name "$dataset_name" \
    --arg suite_json "$suite_json" \
    --arg started_at "$started_at" \
    --arg finished_at "$finished_at" \
    --arg predictions_jsonl "$predictions_jsonl" \
    --arg scoring_script "$scoring_script" \
    --arg official_harness_manifest "$official_harness_manifest" \
    --arg case_results_jsonl "$case_results_jsonl" \
    --argjson max_workers "$max_workers" \
    --argjson total "$total" \
    --argjson passed "$passed" \
    --argjson failed "$failed" \
    --argjson total_escalation_count "$total_escalation_count" \
    --argjson duration_secs "$duration_secs" \
    --argjson case_results "$(jq -s '.' "$case_results_jsonl")" \
    --argjson official_harness_requested "$official_harness_requested" \
    --argjson official_harness_exit_code "$official_harness_exit_code" \
    --argjson official_harness "$official_harness_summary_json" \
    --arg scoring_semantics "passed/failed counts reflect alan-native orchestration case results; official resolved/unresolved status comes from the SWE-bench harness manifest when present" \
    '{
        suite: $suite,
        dataset: $dataset,
        dataset_name: $dataset_name,
        suite_json: $suite_json,
        started_at: $started_at,
        finished_at: $finished_at,
        duration_secs: $duration_secs,
        total: $total,
        passed: $passed,
        failed: $failed,
        total_escalation_count: $total_escalation_count,
        max_workers: $max_workers,
        predictions_jsonl: $predictions_jsonl,
        scoring_script: $scoring_script,
        official_harness_manifest: $official_harness_manifest,
        official_harness_requested: $official_harness_requested,
        official_harness_exit_code: $official_harness_exit_code,
        official_harness: $official_harness,
        scoring_semantics: $scoring_semantics,
        case_results_jsonl: $case_results_jsonl,
        case_results: $case_results
    }' >"$suite_run_file"

jq -n \
    --arg suite "$suite_name" \
    --arg dataset "$dataset_label" \
    --arg dataset_name "$dataset_name" \
    --arg predictions_jsonl "$predictions_jsonl" \
    --arg scoring_script "$scoring_script" \
    --arg official_harness_manifest "$official_harness_manifest" \
    --argjson total_cases "$total" \
    --argjson passed_cases "$passed" \
    --argjson failed_cases "$failed" \
    --argjson total_escalation_count "$total_escalation_count" \
    --argjson pass_rate_percent "$pass_rate_percent" \
    --argjson duration_secs "$duration_secs" \
    --argjson official_harness_requested "$official_harness_requested" \
    --argjson official_harness_exit_code "$official_harness_exit_code" \
    --argjson official_harness "$official_harness_summary_json" \
    --arg scoring_semantics "passed/failed counts reflect alan-native orchestration case results; official resolved/unresolved status comes from the SWE-bench harness manifest when present" \
    '{
        suite: $suite,
        dataset: $dataset,
        dataset_name: $dataset_name,
        total_cases: $total_cases,
        passed_cases: $passed_cases,
        failed_cases: $failed_cases,
        total_escalation_count: $total_escalation_count,
        pass_rate_percent: $pass_rate_percent,
        duration_secs: $duration_secs,
        predictions_jsonl: $predictions_jsonl,
        scoring_script: $scoring_script,
        official_harness_manifest: $official_harness_manifest,
        official_harness_requested: $official_harness_requested,
        official_harness_exit_code: $official_harness_exit_code,
        official_harness: $official_harness,
        scoring_semantics: $scoring_semantics
    }' >"$benchmark_file"

jq -n \
    --arg suite "swebench_full_steward" \
    --arg mode "subset" \
    --argjson total "$total" \
    --argjson passed "$passed" \
    --argjson failed "$failed" \
    --argjson skipped 0 \
    --argjson pass_rate_percent "$pass_rate_percent" \
    --argjson duration_secs "$duration_secs" \
    --argjson executed_scenarios "$(jq -s '[ .[] | (.run.instance_id // .instance_id // empty) | select(. != "") ]' "$case_results_jsonl")" \
    --argjson kpi_tag_counts '{"external_benchmark":1,"swebench_lite":1,"full_steward":1,"subset":1}' \
    '{
        suite: $suite,
        mode: $mode,
        total: $total,
        passed: $passed,
        failed: $failed,
        skipped: $skipped,
        pass_rate_percent: $pass_rate_percent,
        duration_secs: $duration_secs,
        executed_scenarios: $executed_scenarios,
        kpi_tag_counts: $kpi_tag_counts
    }' >"$suite_kpi_file"

echo "Full-steward SWE-bench subset summary:"
echo "  suite: $suite_name"
echo "  dataset: $dataset_label"
echo "  dataset_name: $dataset_name"
echo "  total: $total"
echo "  passed: $passed"
echo "  failed: $failed"
echo "  total_escalation_count: $total_escalation_count"
if [[ "$official_harness_requested" == true ]]; then
    echo "  official_harness_exit_code: $official_harness_exit_code"
fi
echo "  predictions: $predictions_jsonl"
echo "  scoring_script: $scoring_script"
echo "  artifacts: $output_dir"

if (( failed > 0 )); then
    exit 1
fi

if [[ "$official_harness_requested" == true && "$official_harness_exit_code" != "0" ]]; then
    exit 1
fi
