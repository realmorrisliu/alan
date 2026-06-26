#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'USAGE'
Usage:
  crates/agent-engine/skills/swebench/scripts/score_swebench_predictions.sh <predictions-jsonl> [--dataset-name <name>] [--max-workers <n>] [--run-id <id>] [--work-dir <dir>] [--manifest-file <path>] [--python-bin <path>]

Defaults:
  dataset-name  princeton-nlp/SWE-bench_Lite
  max-workers   8
  run-id        swebench_eval_<timestamp>
  work-dir      directory containing <predictions-jsonl>
  manifest-file <work-dir>/official_harness_run.json

This is a thin wrapper around the official SWE-bench harness entrypoint:
  python -m swebench.harness.run_evaluation

Environment:
  ALAN_SWEBENCH_HARNESS_PYTHON_BIN
      Optional Python interpreter for the official harness.
USAGE
}

predictions_path=""
dataset_name="princeton-nlp/SWE-bench_Lite"
max_workers="8"
run_id="swebench_eval_$(date -u +%Y%m%dT%H%M%SZ)"
work_dir=""
manifest_file=""
python_bin_override=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dataset-name)
            dataset_name="$2"
            shift 2
            ;;
        --max-workers)
            max_workers="$2"
            shift 2
            ;;
        --run-id)
            run_id="$2"
            shift 2
            ;;
        --work-dir)
            work_dir="$2"
            shift 2
            ;;
        --manifest-file)
            manifest_file="$2"
            shift 2
            ;;
        --python-bin)
            python_bin_override="$2"
            shift 2
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
            if [[ -n "$predictions_path" ]]; then
                usage
                exit 2
            fi
            predictions_path="$1"
            shift
            ;;
    esac
done

if [[ -z "$predictions_path" ]]; then
    usage
    exit 2
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
preflight_script="$script_dir/check_swebench_harness_env.sh"

if [[ ! -f "$predictions_path" ]]; then
    echo "Predictions file not found: $predictions_path" >&2
    exit 1
fi

predictions_path="$(cd "$(dirname "$predictions_path")" && pwd)/$(basename "$predictions_path")"
if [[ -z "$work_dir" ]]; then
    work_dir="$(dirname "$predictions_path")"
fi
mkdir -p "$work_dir"
work_dir="$(cd "$work_dir" && pwd)"

if [[ -z "$manifest_file" ]]; then
    manifest_file="$work_dir/official_harness_run.json"
fi
mkdir -p "$(dirname "$manifest_file")"
manifest_file="$(cd "$(dirname "$manifest_file")" && pwd)/$(basename "$manifest_file")"

stdout_file="$work_dir/official_harness_stdout.log"
stderr_file="$work_dir/official_harness_stderr.log"
preflight_stderr_file="$work_dir/official_harness_preflight.stderr.log"
started_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
start_epoch="$(date +%s)"

preflight_args=(--json)
if [[ -n "$python_bin_override" ]]; then
    preflight_args+=(--python-bin "$python_bin_override")
fi

preflight_status=0
set +e
preflight_json="$(bash "$preflight_script" "${preflight_args[@]}" 2>"$preflight_stderr_file")"
preflight_status=$?
set -e

if [[ -z "$preflight_json" ]]; then
    preflight_json='{"ready":false,"missing_requirements":["preflight_failed_without_json"]}'
fi

python_bin="$(printf '%s' "$preflight_json" | jq -r '.selected_python_bin // empty')"
if [[ -z "$python_bin" ]]; then
    python_bin="${python_bin_override:-${ALAN_SWEBENCH_HARNESS_PYTHON_BIN:-}}"
fi

exit_code=0
if [[ "$preflight_status" -ne 0 ]]; then
    printf 'Official SWE-bench harness preflight failed.\n' >"$stderr_file"
    if [[ -s "$preflight_stderr_file" ]]; then
        cat "$preflight_stderr_file" >>"$stderr_file"
    fi
    exit_code=1
fi

if [[ "$exit_code" -eq 0 ]]; then
    set +e
    (
        cd "$work_dir"
        "$python_bin" -m swebench.harness.run_evaluation \
          --dataset_name "$dataset_name" \
          --predictions_path "$predictions_path" \
          --max_workers "$max_workers" \
          --run_id "$run_id"
    ) >"$stdout_file" 2>"$stderr_file"
    exit_code=$?
    set -e
fi

finished_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
duration_secs="$(( $(date +%s) - start_epoch ))"
results_dir="$work_dir/evaluation_results/$run_id"
results_json="$results_dir/results.json"
instance_results_jsonl="$results_dir/instance_results.jsonl"
logs_dir="$work_dir/logs"
results_json_payload='null'
report_json=""
report_json_payload='null'
instance_results_payload='[]'
submitted_report_file="$work_dir/official_harness_submitted_report.json"
submitted_report_payload='null'
submitted_report_available=false
instance_count=0
resolved_count=0
unresolved_count=0
resolved_rate_percent="null"

if [[ -f "$results_json" ]]; then
    results_json_payload="$(jq -c . "$results_json")"
fi

report_json="$(find "$work_dir" -maxdepth 1 -type f -name "*.$run_id.json" | head -n 1)"
if [[ -n "$report_json" && -f "$report_json" ]]; then
    report_json_payload="$(jq -c . "$report_json")"
fi

if [[ ! -f "$instance_results_jsonl" && -n "$report_json" && -f "$report_json" ]]; then
    synthesized_instance_results_jsonl="$work_dir/official_harness_instance_results.jsonl"
    jq -c '
        to_entries[]
        | select(.value | type == "object")
        | select(.value | has("resolved"))
        | {
            instance_id: .key,
            resolved: (.value.resolved // false),
            report: .value
        }
    ' "$report_json" >"$synthesized_instance_results_jsonl"
    if [[ -s "$synthesized_instance_results_jsonl" ]]; then
        instance_results_jsonl="$synthesized_instance_results_jsonl"
    fi
fi

if [[ ! -f "$instance_results_jsonl" ]]; then
    synthesized_instance_results_jsonl="$work_dir/official_harness_instance_results.jsonl"
    : >"$synthesized_instance_results_jsonl"
    while IFS= read -r instance_report_file; do
        jq -c '
            to_entries[]
            | select(.value | type == "object")
            | {
                instance_id: .key,
                resolved: (.value.resolved // false),
                report: .value
            }
        ' "$instance_report_file" >>"$synthesized_instance_results_jsonl"
    done < <(find "$logs_dir/run_evaluation/$run_id" -type f -name report.json 2>/dev/null | sort)
    if [[ -s "$synthesized_instance_results_jsonl" ]]; then
        instance_results_jsonl="$synthesized_instance_results_jsonl"
    fi
fi

if [[ -f "$instance_results_jsonl" ]]; then
    instance_results_payload="$(jq -s '.' "$instance_results_jsonl")"
    instance_count="$(jq -s 'length' "$instance_results_jsonl")"
    resolved_count="$(jq -s '[.[] | select(.resolved == true)] | length' "$instance_results_jsonl")"
    unresolved_count=$((instance_count - resolved_count))
    if (( instance_count > 0 )); then
        resolved_rate_percent="$(awk "BEGIN { printf \"%.2f\", ($resolved_count / $instance_count) * 100 }")"
    fi
fi

if [[ -f "$instance_results_jsonl" ]]; then
    jq -n \
        --arg report_json "$report_json" \
        --argjson report "$report_json_payload" \
        --argjson instance_results "$instance_results_payload" \
        --argjson instance_count "$instance_count" \
        --argjson resolved_count "$resolved_count" \
        --argjson unresolved_count "$unresolved_count" \
        '{
            submitted_instances: ($report.submitted_instances // $instance_count),
            completed_instances: ($report.completed_instances // $instance_count),
            resolved_instances: ($report.resolved_instances // $resolved_count),
            unresolved_instances: ($report.unresolved_instances // $unresolved_count),
            empty_patch_instances: ($report.empty_patch_instances // 0),
            error_instances: ($report.error_instances // 0),
            submitted_ids: ($report.submitted_ids // ($instance_results | map(.instance_id))),
            completed_ids: ($report.completed_ids // ($instance_results | map(.instance_id))),
            resolved_ids: ($report.resolved_ids // ($instance_results | map(select(.resolved == true) | .instance_id))),
            unresolved_ids: ($report.unresolved_ids // ($instance_results | map(select(.resolved != true) | .instance_id))),
            error_ids: ($report.error_ids // []),
            empty_patch_ids: ($report.empty_patch_ids // []),
            missing_ids: (($report.submitted_ids // ($instance_results | map(.instance_id)))
                - ($report.completed_ids // ($instance_results | map(.instance_id)))),
            report_json: (if $report_json == "" then null else $report_json end)
        }' >"$submitted_report_file"
    submitted_report_payload="$(jq -c . "$submitted_report_file")"
    submitted_report_available=true
fi

jq -n \
    --arg dataset_name "$dataset_name" \
    --arg predictions_path "$predictions_path" \
    --arg run_id "$run_id" \
    --arg work_dir "$work_dir" \
    --arg started_at "$started_at" \
    --arg finished_at "$finished_at" \
    --arg stdout_file "$stdout_file" \
    --arg stderr_file "$stderr_file" \
    --arg results_dir "$results_dir" \
    --arg results_json "$results_json" \
    --arg instance_results_jsonl "$instance_results_jsonl" \
    --arg report_json "$report_json" \
    --arg submitted_report_file "$submitted_report_file" \
    --arg logs_dir "$logs_dir" \
    --arg preflight_stderr_file "$preflight_stderr_file" \
    --argjson max_workers "$max_workers" \
    --argjson duration_secs "$duration_secs" \
    --argjson exit_code "$exit_code" \
    --argjson preflight "$preflight_json" \
    --argjson results_json_available "$([[ -f "$results_json" ]] && echo true || echo false)" \
    --argjson instance_results_available "$([[ -f "$instance_results_jsonl" ]] && echo true || echo false)" \
    --argjson report_json_available "$([[ -n "$report_json" && -f "$report_json" ]] && echo true || echo false)" \
    --argjson submitted_report_available "$submitted_report_available" \
    --argjson instance_count "$instance_count" \
    --argjson resolved_count "$resolved_count" \
    --argjson unresolved_count "$unresolved_count" \
    --argjson results "$results_json_payload" \
    --argjson report "$report_json_payload" \
    --argjson submitted_report "$submitted_report_payload" \
    '{
        dataset_name: $dataset_name,
        predictions_path: $predictions_path,
        run_id: $run_id,
        work_dir: $work_dir,
        started_at: $started_at,
        finished_at: $finished_at,
        duration_secs: $duration_secs,
        exit_code: $exit_code,
        preflight: $preflight,
        overall_status: (
            if ($preflight.ready | not) then
                "preflight_failed"
            elif $exit_code != 0 then
                "failed_to_run"
            elif ($instance_results_available and $instance_count > 0 and $resolved_count == $instance_count) then
                "all_resolved"
            elif ($instance_results_available and $resolved_count > 0) then
                "partially_resolved"
            elif $instance_results_available then
                "none_resolved"
            else
                "completed_without_instance_results"
            end
        ),
        max_workers: $max_workers,
        stdout_file: $stdout_file,
        stderr_file: $stderr_file,
        preflight_stderr_file: (if ($preflight.ready | not) and ($preflight_stderr_file != "") then $preflight_stderr_file else null end),
        results_dir: $results_dir,
        results_json: (if $results_json_available then $results_json else null end),
        instance_results_jsonl: (if $instance_results_available then $instance_results_jsonl else null end),
        report_json: (if $report_json_available then $report_json else null end),
        submitted_report_json: (if $submitted_report_available then $submitted_report_file else null end),
        logs_dir: $logs_dir,
        results_json_available: $results_json_available,
        instance_results_available: $instance_results_available,
        report_json_available: $report_json_available,
        submitted_report_available: $submitted_report_available,
        instance_count: $instance_count,
        resolved_count: $resolved_count,
        unresolved_count: $unresolved_count,
        results: (if $results_json_available then $results else null end),
        report: (if $report_json_available then $report else null end),
        submitted_report: (if $submitted_report_available then $submitted_report else null end)
    }' >"$manifest_file"

if [[ -f "$instance_results_jsonl" ]]; then
    while IFS= read -r official_result; do
        if [[ -z "$official_result" ]]; then
            continue
        fi
        case_instance_id="$(printf '%s' "$official_result" | jq -r '.instance_id // empty')"
        if [[ -z "$case_instance_id" ]]; then
            continue
        fi
        case_official_result_dir="$work_dir/cases/$case_instance_id"
        if [[ -d "$case_official_result_dir" ]]; then
            printf '%s\n' "$official_result" >"$case_official_result_dir/official_harness_instance_result.json"
        fi
    done <"$instance_results_jsonl"
fi

case_results_jsonl="$work_dir/case_results.jsonl"
if [[ -f "$case_results_jsonl" && -f "$instance_results_jsonl" ]]; then
    tmp_case_results_jsonl="$work_dir/case_results.with_official.tmp.jsonl"
    : >"$tmp_case_results_jsonl"
    while IFS= read -r case_record; do
        if [[ -z "$case_record" ]]; then
            continue
        fi
        case_instance_id="$(printf '%s' "$case_record" | jq -r '.run.instance_id // .instance_id // empty')"
        official_result="$(jq -c --arg instance_id "$case_instance_id" 'select(.instance_id == $instance_id)' "$instance_results_jsonl" | head -n 1)"
        if [[ -n "$official_result" ]]; then
            case_official_result_file="$work_dir/cases/$case_instance_id/official_harness_instance_result.json"
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

manifest_payload="$(jq -c . "$manifest_file")"
suite_run_file="$work_dir/run.json"
if [[ -f "$suite_run_file" ]]; then
    tmp_suite_run_file="$work_dir/run.with_official.tmp.json"
    jq \
        --arg official_harness_manifest "$manifest_file" \
        --arg case_results_jsonl "$case_results_jsonl" \
        --argjson official_harness "$manifest_payload" \
        --argjson official_harness_requested true \
        --argjson official_harness_exit_code "$exit_code" \
        --argjson case_results "$([[ -f "$case_results_jsonl" ]] && jq -s . "$case_results_jsonl" || printf 'null')" \
        --argjson submitted_report "$submitted_report_payload" \
        '. + {
            official_harness_manifest: $official_harness_manifest,
            official_harness_requested: $official_harness_requested,
            official_harness_exit_code: $official_harness_exit_code,
            official_harness: $official_harness,
            official_harness_submitted_report_file: (if $submitted_report == null then null else ($official_harness.submitted_report_json // null) end),
            official_harness_submitted_report: $submitted_report,
            case_results_jsonl: (if $case_results == null then .case_results_jsonl else $case_results_jsonl end),
            case_results: (if $case_results == null then .case_results else $case_results end)
        }' "$suite_run_file" >"$tmp_suite_run_file"
    mv "$tmp_suite_run_file" "$suite_run_file"
fi

suite_benchmark_file="$work_dir/benchmark.json"
if [[ -f "$suite_benchmark_file" ]]; then
    tmp_suite_benchmark_file="$work_dir/benchmark.with_official.tmp.json"
    jq \
        --arg official_harness_manifest "$manifest_file" \
        --argjson official_harness "$manifest_payload" \
        --argjson official_harness_requested true \
        --argjson official_harness_exit_code "$exit_code" \
        --argjson submitted_report "$submitted_report_payload" \
        '. + {
            official_harness_manifest: $official_harness_manifest,
            official_harness_requested: $official_harness_requested,
            official_harness_exit_code: $official_harness_exit_code,
            official_harness: $official_harness,
            official_harness_submitted_report_file: (if $submitted_report == null then null else ($official_harness.submitted_report_json // null) end),
            official_harness_submitted_report: $submitted_report
        }' "$suite_benchmark_file" >"$tmp_suite_benchmark_file"
    mv "$tmp_suite_benchmark_file" "$suite_benchmark_file"
fi

suite_kpi_file="$work_dir/kpi.json"
if [[ -f "$suite_kpi_file" ]]; then
    tmp_suite_kpi_file="$work_dir/kpi.with_official.tmp.json"
    jq \
        --argjson official_harness_requested true \
        --argjson official_harness_exit_code "$exit_code" \
        --argjson official_harness "$manifest_payload" \
        --argjson submitted_report "$submitted_report_payload" \
        --argjson official_instance_count "$instance_count" \
        --argjson official_resolved_count "$resolved_count" \
        --argjson official_unresolved_count "$unresolved_count" \
        --argjson official_resolved_rate_percent "$resolved_rate_percent" \
        '. + {
            official_harness_requested: $official_harness_requested,
            official_harness_exit_code: $official_harness_exit_code,
            official_harness_overall_status: $official_harness.overall_status,
            official_instance_count: $official_instance_count,
            official_resolved_count: $official_resolved_count,
            official_unresolved_count: $official_unresolved_count,
            official_resolved_rate_percent: $official_resolved_rate_percent,
            official_resolved_ids: (if $submitted_report == null then [] else ($submitted_report.resolved_ids // []) end),
            official_unresolved_ids: (if $submitted_report == null then [] else ($submitted_report.unresolved_ids // []) end)
        }' "$suite_kpi_file" >"$tmp_suite_kpi_file"
    mv "$tmp_suite_kpi_file" "$suite_kpi_file"
fi

if [[ -s "$stdout_file" ]]; then
    cat "$stdout_file"
fi
if [[ -s "$stderr_file" ]]; then
    cat "$stderr_file" >&2
fi

if [[ "$exit_code" -ne 0 ]]; then
    exit "$exit_code"
fi
