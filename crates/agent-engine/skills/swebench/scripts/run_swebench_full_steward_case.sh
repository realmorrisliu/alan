#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'USAGE'
Usage:
  crates/agent-engine/skills/swebench/scripts/run_swebench_full_steward_case.sh <case-json> [--output-dir <dir>] [--agent <name>] [--keep-session]

Case JSON fields:
  instance_id               Required benchmark instance identifier.
  workspace_dir             Required absolute or case-relative path to a clean prepared git workspace.
  problem_statement         Inline problem statement text (optional if problem_statement_file is set).
  problem_statement_file    Absolute or case-relative path to a text file with the benchmark problem statement.
  timeout_secs              Optional turn timeout for the steward run (default: 1800).
  dataset                   Optional dataset label (default: SWE-bench Lite).
  agent_name                Optional agent override.

The runner starts alan's root/steward entrypoint against the prepared workspace,
submits one benchmark task, waits for the turn to complete, verifies that
repo-local work happened through delegated child launch(es), and exports:

  - model.patch
  - prediction.json / predictions.jsonl
  - run.json
  - assertion_report.json
  - kpi.json

This is the M1 single-case bring-up path for the Lite-first full-steward
benchmark adapter. It assumes the benchmark workspace is already prepared
according to the external dataset/operator flow.
USAGE
}

require_command() {
    local command_name="$1"
    if ! command -v "$command_name" >/dev/null 2>&1; then
        echo "Missing required command: $command_name" >&2
        exit 1
    fi
}

pick_free_localhost_port() {
    python3 - <<'PY'
import socket

with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
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

case_json=""
output_dir=""
cli_agent_name=""
keep_session=false

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
        -h|--help)
            usage
            exit 0
            ;;
        -*)
            usage
            exit 2
            ;;
        *)
            if [[ -n "$case_json" ]]; then
                usage
                exit 2
            fi
            case_json="$1"
            shift
            ;;
    esac
done

if [[ -z "$case_json" ]]; then
    usage
    exit 2
fi

require_command jq
require_command curl
require_command git

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
package_root="$(cd "$script_dir/.." && pwd)"
repo_root="$(cd "$package_root/../../../.." && pwd)"
configured_alan_bin="${ALAN_BIN:-}"
use_prebuilt_alan=false
alan_bin=""
if [[ -n "$configured_alan_bin" ]]; then
    if [[ ! -x "$configured_alan_bin" ]]; then
        echo "Configured ALAN_BIN is not executable: $configured_alan_bin" >&2
        exit 1
    fi
    alan_bin="$configured_alan_bin"
    use_prebuilt_alan=true
elif [[ -x "$repo_root/target/debug/alan" ]]; then
    alan_bin="$repo_root/target/debug/alan"
    use_prebuilt_alan=true
else
    require_command cargo
fi

run_alan_daemon() {
    local daemon_subcommand="$1"
    shift || true
    if [[ "$use_prebuilt_alan" == true ]]; then
        (cd "$repo_root" && env "$@" "$alan_bin" daemon "$daemon_subcommand")
    else
        (cd "$repo_root" && env "$@" cargo run -p alan -- daemon "$daemon_subcommand")
    fi
}

case_json="$(cd "$(dirname "$case_json")" && pwd)/$(basename "$case_json")"
case_dir="$(dirname "$case_json")"

if [[ ! -f "$case_json" ]]; then
    echo "Case file not found: $case_json" >&2
    exit 1
fi

instance_id="$(jq -r '.instance_id // empty' "$case_json")"
dataset_label="$(jq -r '.dataset // "SWE-bench Lite"' "$case_json")"
workspace_raw="$(jq -r '.workspace_dir // empty' "$case_json")"
problem_statement_inline="$(jq -r '.problem_statement // empty' "$case_json")"
problem_statement_file_raw="$(jq -r '.problem_statement_file // empty' "$case_json")"
timeout_secs="$(jq -r '.timeout_secs // 1800' "$case_json")"
case_agent_name="$(jq -r '.agent_name // empty' "$case_json")"

if [[ -z "$instance_id" || -z "$workspace_raw" ]]; then
    echo "Case file must define non-empty instance_id and workspace_dir." >&2
    exit 1
fi

workspace_dir="$(resolve_path "$workspace_raw" "$case_dir")"
if [[ ! -d "$workspace_dir" ]]; then
    echo "Workspace directory not found: $workspace_dir" >&2
    exit 1
fi

if [[ -n "$problem_statement_file_raw" ]]; then
    problem_statement_file="$(resolve_path "$problem_statement_file_raw" "$case_dir")"
    if [[ ! -f "$problem_statement_file" ]]; then
        echo "Problem statement file not found: $problem_statement_file" >&2
        exit 1
    fi
    problem_statement="$(cat "$problem_statement_file")"
elif [[ -n "$problem_statement_inline" ]]; then
    problem_statement="$problem_statement_inline"
    problem_statement_file=""
else
    echo "Case file must define problem_statement or problem_statement_file." >&2
    exit 1
fi

if ! git -C "$workspace_dir" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    echo "Workspace must be a git work tree: $workspace_dir" >&2
    exit 1
fi

workspace_dir="$(git -C "$workspace_dir" rev-parse --show-toplevel)"
workspace_alan_dir="$workspace_dir/.alan"
repo_status_without_alan="$(git -C "$workspace_dir" status --short --untracked-files=all -- . ':(glob,exclude).alan/**')"
if [[ -z "$repo_status_without_alan" && -d "$workspace_alan_dir" ]]; then
    if ! git -C "$workspace_dir" ls-files --error-unmatch -- .alan '.alan/**' >/dev/null 2>&1; then
        rm -rf "$workspace_alan_dir"
    fi
fi
if [[ -n "$(git -C "$workspace_dir" status --short --untracked-files=all)" ]]; then
    echo "Workspace must be clean before running the benchmark case: $workspace_dir" >&2
    exit 1
fi

if [[ -z "$output_dir" ]]; then
    output_dir="$repo_root/target/benchmarks/swebench_lite/latest/$instance_id"
fi
mkdir -p "$output_dir"

events_file="$output_dir/events.jsonl"
assistant_output_file="$output_dir/assistant_output.txt"
prompt_file="$output_dir/task_prompt.md"
create_response_file="$output_dir/create_session.json"
submit_response_file="$output_dir/submit_response.json"
read_response_file="$output_dir/read_session.json"
status_file="$output_dir/git_status.txt"
patch_file="$output_dir/model.patch"
prediction_file="$output_dir/prediction.json"
predictions_jsonl="$output_dir/predictions.jsonl"
rollout_copy_file="$output_dir/parent_rollout.jsonl"
assertion_file="$output_dir/assertion_report.json"
run_file="$output_dir/run.json"
kpi_file="$output_dir/kpi.json"
verification_entries_file="$output_dir/verification_entries.json"
verification_summary_file="$output_dir/verification_summary.json"

: >"$events_file"
: >"$assistant_output_file"

effective_agent_name="$cli_agent_name"
if [[ -z "$effective_agent_name" ]]; then
    effective_agent_name="$case_agent_name"
fi

base_url="${ALAN_AGENTD_URL:-}"
daemon_bind_address=""
session_id=""
daemon_started=false
delete_session_on_exit=true

cleanup() {
    if [[ "$delete_session_on_exit" == true && -n "$session_id" ]]; then
        curl -fsS -X DELETE "$base_url/api/v1/sessions/$session_id" >/dev/null 2>&1 || true
    fi
    if [[ "$daemon_started" == true ]]; then
        run_alan_daemon stop >/dev/null 2>&1 || true
    fi
}
trap cleanup EXIT

if [[ -z "$base_url" ]]; then
    require_command python3
    daemon_port="$(pick_free_localhost_port)"
    daemon_bind_address="127.0.0.1:$daemon_port"
    base_url="http://$daemon_bind_address"
    export ALAN_AGENTD_URL="$base_url"
fi

if ! curl -fsS "$base_url/health" >/dev/null 2>&1; then
    if [[ -n "$daemon_bind_address" ]]; then
        run_alan_daemon start BIND_ADDRESS="$daemon_bind_address" >/dev/null
    else
        run_alan_daemon start >/dev/null
    fi
    daemon_started=true
fi

started_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
start_epoch="$(date +%s)"

cat >"$prompt_file" <<EOF
You are alan's root coding steward running a $dataset_label benchmark case.

Hard requirements:
1. Treat this run as full steward mode.
2. Use \$repo-coding to delegate repo-local coding work to a fresh repo worker child runtime.
3. Do not perform repo-local file edits or repo-local verification in the parent steward runtime.
4. Keep the final response concise: what changed, what was verified, and residual risk.

Benchmark metadata:
- instance_id: $instance_id
- dataset: $dataset_label
- bound_workspace_root: $workspace_dir

The benchmark harness has already bound the target repository as the active
workspace for this run. You still must route repo-local coding execution
through child launch rather than editing inline in the parent runtime.

Problem statement:
$problem_statement
EOF

create_body="$(jq -n \
    --arg workspace_dir "$workspace_dir" \
    --arg agent_name "$effective_agent_name" \
    '{
        workspace_dir: $workspace_dir,
        governance: { profile: "autonomous" }
    }
    + (if $agent_name == "" then {} else { agent_name: $agent_name } end)')"

curl -fsS \
    -H 'Content-Type: application/json' \
    -d "$create_body" \
    "$base_url/api/v1/sessions" \
    >"$create_response_file"

session_id="$(jq -r '.session_id // empty' "$create_response_file")"
if [[ -z "$session_id" ]]; then
    echo "Failed to create session." >&2
    exit 1
fi

submit_body="$(jq -n --rawfile prompt "$prompt_file" '{
    op: {
        type: "turn",
        parts: [
            {
                type: "text",
                text: $prompt
            }
        ]
    }
}')"

curl -fsS \
    -H 'Content-Type: application/json' \
    -d "$submit_body" \
    "$base_url/api/v1/sessions/$session_id/submit" \
    >"$submit_response_file"

event_cursor=""
turn_completed=false
yielded=false
timed_out=false
error_messages_file="$output_dir/error_messages.txt"
warning_messages_file="$output_dir/warning_messages.txt"
: >"$error_messages_file"
: >"$warning_messages_file"

while true; do
    now_epoch="$(date +%s)"
    if (( now_epoch - start_epoch >= timeout_secs )); then
        timed_out=true
        break
    fi

    read_url="$base_url/api/v1/sessions/$session_id/events/read?limit=200"
    if [[ -n "$event_cursor" ]]; then
        read_url="$read_url&after_event_id=$event_cursor"
    fi

    response="$(curl -fsS "$read_url")"
    latest_event_id="$(printf '%s' "$response" | jq -r '.latest_event_id // empty')"
    if [[ -n "$latest_event_id" ]]; then
        event_cursor="$latest_event_id"
    fi

    while IFS= read -r event_json; do
        if [[ -z "$event_json" ]]; then
            continue
        fi
        printf '%s\n' "$event_json" >>"$events_file"
        event_type="$(printf '%s' "$event_json" | jq -r '.type')"
        case "$event_type" in
            text_delta)
                printf '%s' "$(printf '%s' "$event_json" | jq -r '.chunk')" >>"$assistant_output_file"
                ;;
            warning)
                printf '%s\n' "$(printf '%s' "$event_json" | jq -r '.message')" >>"$warning_messages_file"
                ;;
            error)
                printf '%s\n' "$(printf '%s' "$event_json" | jq -r '.message')" >>"$error_messages_file"
                ;;
            yield)
                yielded=true
                ;;
            turn_completed)
                turn_completed=true
                ;;
        esac
    done < <(printf '%s' "$response" | jq -c '.events[]')

    if [[ "$turn_completed" == true || "$yielded" == true ]]; then
        break
    fi

    sleep 1
done

curl -fsS "$base_url/api/v1/sessions/$session_id/read" >"$read_response_file"
resolved_model="$(jq -r '.resolved_model // "unknown"' "$read_response_file")"
rollout_path="$(jq -r '.rollout_path // empty' "$read_response_file")"

if [[ -z "$rollout_path" || ! -f "$rollout_path" ]]; then
    echo "Failed to resolve a durable rollout path for session $session_id." >&2
    exit 1
fi

cp "$rollout_path" "$rollout_copy_file"

spawn_count="$(jq -s '[.[] | select(.type == "tool_call" and .name == "invoke_delegated_skill")] | length' "$rollout_copy_file")"
parent_escalation_count="$(jq -s '[.[] | select(.type == "tool_call") | select((.audit.action // "") == "escalate")] | length' "$rollout_copy_file")"
parent_inline_write_count="$(jq -s '[.[] | select(.type == "tool_call" and .name != "invoke_delegated_skill") | select((.audit.capability // "") == "write")] | length' "$rollout_copy_file")"
parent_inline_write_names="$(jq -s '[.[] | select(.type == "tool_call" and .name != "invoke_delegated_skill") | select((.audit.capability // "") == "write") | .name] | unique' "$rollout_copy_file")"
child_runs_json="$(jq -s '[.[] | select(.type == "tool_call" and .name == "invoke_delegated_skill") | .result.child_run? | select(. != null)]' "$rollout_copy_file")"
completed_child_count="$(printf '%s' "$child_runs_json" | jq '[.[] | select(.terminal_status == "completed")] | length')"
child_escalation_count=0
child_rollout_files=()

while IFS= read -r child_run; do
    child_rollout="$(printf '%s' "$child_run" | jq -r '.rollout_path // empty')"
    child_session="$(printf '%s' "$child_run" | jq -r '.session_id // "child"')"
    if [[ -n "$child_rollout" && -f "$child_rollout" ]]; then
        child_rollout_copy="$output_dir/${child_session}.jsonl"
        cp "$child_rollout" "$child_rollout_copy"
        child_rollout_files+=("$child_rollout_copy")
        child_rollout_escalation_count="$(jq -s '[.[] | select(.type == "tool_call") | select((.audit.action // "") == "escalate")] | length' "$child_rollout")"
        child_escalation_count=$((child_escalation_count + child_rollout_escalation_count))
    fi
done < <(printf '%s\n' "$child_runs_json" | jq -c '.[]')

escalation_count=$((parent_escalation_count + child_escalation_count))

if (( ${#child_rollout_files[@]} > 0 )); then
    jq -s '
        def verification_command($cmd):
            ($cmd | test("(^|[[:space:]])(pytest|py\\.test|nosetests|nosetests3)([[:space:]]|$)"))
            or ($cmd | test("(^|[[:space:]])python(3)?([[:space:]]+[-[:alnum:]_./:=+]+)*[[:space:]]+-m[[:space:]]+(pytest|unittest)([[:space:]]|$)"))
            or ($cmd | test("(^|[[:space:]])cargo[[:space:]]+(test|check|clippy)([[:space:]]|$)"))
            or ($cmd | test("(^|[[:space:]])go[[:space:]]+test([[:space:]]|$)"))
            or ($cmd | test("(^|[[:space:]])(npm|pnpm|yarn|bun)[[:space:]]+test([[:space:]]|$)"))
            or ($cmd | test("(^|[[:space:]])(make|just)[[:space:]]+(test|check)([[:space:]]|$)"));
        def verification_scope($cmd):
            if ($cmd | test("(^|[[:space:]])(-k|::|--maxfail|test[^[:space:]]*|[^[:space:]]+\\.(py|rs|go|js|ts))([[:space:]]|$)")) then
                "targeted"
            else
                "broader"
            end;
        def first_nonempty_line($text):
            ($text // "" | split("\n") | map(select(length > 0)) | .[0] // "");
        def first_meaningful_line($text):
            ($text // "" | split("\n") | map(select(length > 0))) as $lines
            | (
                $lines
                | map(select(test("command not found|No module named|ModuleNotFoundError|ImportError|AssertionError|FAILED|ERROR|ERR"; "i")))
                | .[0]
              ) // (
                $lines
                | map(
                    select(
                        (test("SyntaxWarning|DeprecationWarning|ResourceWarning"; "i") | not)
                        and . != "\"\"\""
                        and . != "E"
                        and (test("^=+$") | not)
                    )
                )
                | .[0]
              ) // ($lines[0] // "");
        def environment_blocked($text):
            ($text // "" | test("command not found|No module named 'pytest'|No module named pytest|ModuleNotFoundError: No module named 'pytest'|ModuleNotFoundError: No module named pytest|No module named 'nose'|No module named nose|ModuleNotFoundError: No module named 'nose'|ModuleNotFoundError: No module named nose|executable file not found|Target Python binary .* not found"; "i"));
        [
            .[]
            | select(.type == "tool_call" and .name == "bash")
            | . as $call
            | ($call.arguments.command // "") as $cmd
            | select(verification_command($cmd))
            | ($call.result.stderr // $call.result.error // "") as $stderr
            | ($call.result.stdout // "") as $stdout
            | {
                command: $cmd,
                scope: verification_scope($cmd),
                status: (
                    if (($call.result.status // "") == "blocked_by_policy") then
                        "blocked"
                    elif (($call.result.exit_code // 1) == 0) then
                        "passed"
                    elif environment_blocked($stderr) then
                        "environment_blocked"
                    else
                        "failed"
                    end
                ),
                exit_code: ($call.result.exit_code // null),
                summary: (
                    first_meaningful_line($stderr) as $stderr_line
                    | if $stderr_line != "" then
                        $stderr_line
                      else
                        first_meaningful_line($stdout) as $stdout_line
                        | if $stdout_line != "" then
                            $stdout_line
                          elif (($call.result.status // "") != "") then
                            ($call.result.status // "")
                          else
                            "no output"
                          end
                      end
                )
            }
        ]
    ' "${child_rollout_files[@]}" >"$verification_entries_file"
else
    printf '[]\n' >"$verification_entries_file"
fi

jq -n \
    --slurpfile entries "$verification_entries_file" \
    '
        ($entries[0] // []) as $entries
        | ($entries | map(.status) | unique) as $status_kinds
        | ($entries | map(select(.status == "passed")) | length) as $passed_count
        | ($entries | map(select(.status == "failed")) | length) as $failed_count
        | ($entries | map(select(.status == "environment_blocked")) | length) as $environment_blocked_count
        | ($entries | map(select(.status == "blocked")) | length) as $blocked_count
        | ($entries | map(select(.status == "not_run")) | length) as $not_run_count
        | ($entries | length) as $attempted_count
        | {
            overall_status: (
                if $attempted_count == 0 then
                    "not_attempted"
                elif (($status_kinds | length) > 1) then
                    "mixed"
                elif $passed_count == $attempted_count then
                    "passed"
                elif $failed_count == $attempted_count then
                    "failed"
                elif $environment_blocked_count == $attempted_count then
                    "environment_blocked"
                elif $blocked_count == $attempted_count then
                    "blocked"
                else
                    "attempted"
                end
            ),
            verification_attempted: ($attempted_count > 0),
            attempted_count: $attempted_count,
            passed_count: $passed_count,
            failed_count: $failed_count,
            environment_blocked_count: $environment_blocked_count,
            blocked_count: $blocked_count,
            not_run_count: $not_run_count,
            all_passed: ($attempted_count > 0 and $passed_count == $attempted_count),
            entries: $entries
        }
    ' >"$verification_summary_file"

if [[ "$keep_session" != true ]]; then
    curl -fsS -X DELETE "$base_url/api/v1/sessions/$session_id" >/dev/null 2>&1 || true
    delete_session_on_exit=false
    rm -rf "$workspace_alan_dir"
fi

git -C "$workspace_dir" add -N -A -- . ':(glob,exclude).alan/**' >/dev/null 2>&1 || true
git -C "$workspace_dir" status --short --untracked-files=all -- . ':(glob,exclude).alan/**' >"$status_file"
git -C "$workspace_dir" diff --binary --no-ext-diff HEAD -- . ':(glob,exclude).alan/**' >"$patch_file"
patch_bytes="$(wc -c <"$patch_file" | tr -d ' ')"

run_status="completed"
passed=true
if [[ "$timed_out" == true ]]; then
    run_status="timed_out"
    passed=false
elif [[ "$yielded" == true ]]; then
    run_status="yielded"
    passed=false
elif [[ "$turn_completed" != true ]]; then
    run_status="missing_turn_completed"
    passed=false
elif (( spawn_count == 0 )); then
    run_status="missing_child_launch"
    passed=false
elif (( completed_child_count == 0 )); then
    run_status="child_not_completed"
    passed=false
elif (( parent_inline_write_count > 0 )); then
    run_status="parent_inline_write_detected"
    passed=false
elif (( patch_bytes == 0 )); then
    run_status="empty_patch"
    passed=false
fi

finished_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
duration_secs="$(( $(date +%s) - start_epoch ))"
pass_rate_percent="0.00"
if [[ "$passed" == true ]]; then
    pass_rate_percent="100.00"
fi

prediction_json="$(jq -cn \
    --arg instance_id "$instance_id" \
    --arg model_name_or_path "$resolved_model" \
    --rawfile model_patch "$patch_file" \
    '{
        instance_id: $instance_id,
        model_name_or_path: $model_name_or_path,
        model_patch: $model_patch
    }')"

printf '%s\n' "$prediction_json" >"$prediction_file"
printf '%s\n' "$prediction_json" >"$predictions_jsonl"

jq -n \
    --arg instance_id "$instance_id" \
    --arg run_status "$run_status" \
    --argjson timed_out "$timed_out" \
    --argjson yielded "$yielded" \
    --argjson turn_completed "$turn_completed" \
    --argjson spawn_count "$spawn_count" \
    --argjson parent_escalation_count "$parent_escalation_count" \
    --argjson child_escalation_count "$child_escalation_count" \
    --argjson escalation_count "$escalation_count" \
    --argjson completed_child_count "$completed_child_count" \
    --argjson parent_inline_write_count "$parent_inline_write_count" \
    --argjson patch_nonempty "$([[ "$patch_bytes" -gt 0 ]] && echo true || echo false)" \
    --argjson passed "$passed" \
    --argjson parent_inline_write_names "$parent_inline_write_names" \
    '{
        instance_id: $instance_id,
        run_status: $run_status,
        passed: $passed,
        assertions: [
            {
                name: "turn_completed",
                passed: $turn_completed
            },
            {
                name: "delegated_child_launch_observed",
                passed: ($spawn_count > 0),
                detail: ("spawn_count=" + ($spawn_count | tostring))
            },
            {
                name: "completed_child_run_observed",
                passed: ($completed_child_count > 0),
                detail: ("completed_child_count=" + ($completed_child_count | tostring))
            },
            {
                name: "parent_did_not_edit_inline",
                passed: ($parent_inline_write_count == 0),
                detail: ("parent_inline_write_names=" + ($parent_inline_write_names | tostring))
            },
            {
                name: "nonempty_model_patch",
                passed: $patch_nonempty
            },
            {
                name: "no_yield_or_timeout",
                passed: (($timed_out | not) and ($yielded | not))
            }
        ]
    }' >"$assertion_file"

jq -n \
    --arg instance_id "$instance_id" \
    --arg dataset "$dataset_label" \
    --arg case_json "$case_json" \
    --arg workspace_dir "$workspace_dir" \
    --arg started_at "$started_at" \
    --arg finished_at "$finished_at" \
    --arg session_id "$session_id" \
    --arg rollout_path "$rollout_path" \
    --arg resolved_model "$resolved_model" \
    --arg prompt_file "$prompt_file" \
    --arg assistant_output_file "$assistant_output_file" \
    --arg create_response_file "$create_response_file" \
    --arg submit_response_file "$submit_response_file" \
    --arg assertion_file "$assertion_file" \
    --arg kpi_file "$kpi_file" \
    --arg prediction_file "$prediction_file" \
    --arg predictions_jsonl "$predictions_jsonl" \
    --arg patch_file "$patch_file" \
    --arg status_file "$status_file" \
    --arg read_response_file "$read_response_file" \
    --arg events_file "$events_file" \
    --arg error_messages_file "$error_messages_file" \
    --arg warning_messages_file "$warning_messages_file" \
    --arg verification_entries_file "$verification_entries_file" \
    --arg verification_summary_file "$verification_summary_file" \
    --arg problem_statement_file "$problem_statement_file" \
    --argjson duration_secs "$duration_secs" \
    --argjson spawn_count "$spawn_count" \
    --argjson parent_escalation_count "$parent_escalation_count" \
    --argjson child_escalation_count "$child_escalation_count" \
    --argjson escalation_count "$escalation_count" \
    --argjson completed_child_count "$completed_child_count" \
    --argjson parent_inline_write_count "$parent_inline_write_count" \
    --argjson patch_bytes "$patch_bytes" \
    --argjson timed_out "$timed_out" \
    --argjson yielded "$yielded" \
    --argjson turn_completed "$turn_completed" \
    --arg run_status "$run_status" \
    --argjson passed "$passed" \
    --argjson child_runs "$child_runs_json" \
    --argjson parent_inline_write_names "$parent_inline_write_names" \
    --argjson verification_summary "$(jq -c . "$verification_summary_file")" \
    '{
        instance_id: $instance_id,
        dataset: $dataset,
        mode: "full_steward_single_case",
        case_json: $case_json,
        started_at: $started_at,
        finished_at: $finished_at,
        duration_secs: $duration_secs,
        session_id: $session_id,
        rollout_path: $rollout_path,
        run_status: $run_status,
        resolved_model: $resolved_model,
        workspace_dir: $workspace_dir,
        problem_statement_file: (if $problem_statement_file == "" then null else $problem_statement_file end),
        prompt_file: $prompt_file,
        assistant_output_file: $assistant_output_file,
        create_response_file: $create_response_file,
        submit_response_file: $submit_response_file,
        assertion_file: $assertion_file,
        kpi_file: $kpi_file,
        prediction_file: $prediction_file,
        predictions_jsonl: $predictions_jsonl,
        patch_file: $patch_file,
        status_file: $status_file,
        read_response_file: $read_response_file,
        events_file: $events_file,
        error_messages_file: $error_messages_file,
        warning_messages_file: $warning_messages_file,
        verification_entries_file: $verification_entries_file,
        verification_summary_file: $verification_summary_file,
        spawn_count: $spawn_count,
        parent_escalation_count: $parent_escalation_count,
        child_escalation_count: $child_escalation_count,
        escalation_count: $escalation_count,
        completed_child_count: $completed_child_count,
        parent_inline_write_count: $parent_inline_write_count,
        parent_inline_write_names: $parent_inline_write_names,
        patch_bytes: $patch_bytes,
        timed_out: $timed_out,
        yielded: $yielded,
        turn_completed: $turn_completed,
        passed: $passed,
        orchestration_passed: $passed,
        scoring_semantics: "passed reflects alan-native orchestration assertions only; official resolved/unresolved status comes from the SWE-bench harness",
        verification_status: $verification_summary.overall_status,
        verification_summary: $verification_summary,
        child_runs: $child_runs
    }' >"$run_file"

jq -n \
    --arg suite "swebench_full_steward" \
    --arg mode "single_case" \
    --argjson total 1 \
    --argjson passed_count "$([[ "$passed" == true ]] && echo 1 || echo 0)" \
    --argjson failed_count "$([[ "$passed" == true ]] && echo 0 || echo 1)" \
    --argjson skipped 0 \
    --argjson duration_secs "$duration_secs" \
    --argjson executed_scenarios "[\"$instance_id\"]" \
    --argjson kpi_tag_counts '{"external_benchmark":1,"swebench_lite":1,"full_steward":1,"single_case":1}' \
    --argjson pass_rate_percent "$pass_rate_percent" \
    '{
        suite: $suite,
        mode: $mode,
        total: $total,
        passed: $passed_count,
        failed: $failed_count,
        skipped: $skipped,
        pass_rate_percent: $pass_rate_percent,
        duration_secs: $duration_secs,
        executed_scenarios: $executed_scenarios,
        kpi_tag_counts: $kpi_tag_counts
    }' >"$kpi_file"

if [[ "$keep_session" == true ]]; then
    delete_session_on_exit=false
fi

echo "Full-steward SWE-bench case summary:"
echo "  instance_id: $instance_id"
echo "  dataset: $dataset_label"
echo "  run_status: $run_status"
echo "  session_id: $session_id"
echo "  resolved_model: $resolved_model"
echo "  spawn_count: $spawn_count"
echo "  escalation_count: $escalation_count"
echo "  parent_inline_write_count: $parent_inline_write_count"
echo "  verification_status: $(jq -r '.overall_status' "$verification_summary_file")"
echo "  patch_bytes: $patch_bytes"
echo "  artifacts: $output_dir"

if [[ "$passed" != true ]]; then
    exit 1
fi
