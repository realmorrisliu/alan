#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/../../.." && pwd)
RUN_ID="${ALAN_PERF_DIAG_RUN_ID:-$(date +%Y%m%d-%H%M%S)}"
OUTPUT_DIR="${ALAN_PERF_DIAG_OUTPUT_DIR:-$REPO_ROOT/debug/artifacts/performance-diagnostics-workload/$RUN_ID}"
SMOKE_OUTPUT_DIR="$OUTPUT_DIR/ui-smoke"
EXPORT_DIR="$OUTPUT_DIR/export"
PIDS_DIR="$OUTPUT_DIR/pids"
REVIEW_PATH="$OUTPUT_DIR/review.md"
MANIFEST_PATH="$OUTPUT_DIR/manifest.txt"
CONTROL_NAMESPACE="${ALAN_SHELL_CONTROL_NAMESPACE:-alan-perf-diag-$RUN_ID}"
LAUNCH_MODE="${ALAN_UI_SMOKE_LAUNCH_MODE:-open}"
UI_SMOKE_DERIVED_DATA="${ALAN_UI_SMOKE_DERIVED_DATA:-$REPO_ROOT/debug/DerivedData/apple-shell-ui-smoke}"
UI_SMOKE_SKIP_BUILD="${ALAN_PERF_DIAG_SKIP_BUILD:-0}"
UI_SMOKE_APP_PATH="${ALAN_UI_SMOKE_APP_PATH:-}"
UI_SMOKE_APP_EXECUTABLE="${ALAN_UI_SMOKE_APP_EXECUTABLE:-}"
SYSTEM_TMPDIR="${ALAN_UI_SMOKE_SYSTEM_TMPDIR:-$(getconf DARWIN_USER_TEMP_DIR 2>/dev/null || printf '%s' "${TMPDIR:-/tmp}")}"
CONTROL_TMPDIR="$SYSTEM_TMPDIR"
if [[ "$LAUNCH_MODE" == "direct" ]]; then
    CONTROL_TMPDIR="$SMOKE_OUTPUT_DIR/tmp"
fi
if [[ "$UI_SMOKE_SKIP_BUILD" == "1" && -z "$UI_SMOKE_APP_PATH" ]]; then
    UI_SMOKE_APP_PATH="$UI_SMOKE_DERIVED_DATA/Build/Products/Debug/Alan.app"
    UI_SMOKE_APP_EXECUTABLE="$UI_SMOKE_APP_PATH/Contents/MacOS/Alan"
fi
CONTROL_ROOT="${CONTROL_TMPDIR%/}/$CONTROL_NAMESPACE/window_main"
COMMANDS_DIR="$CONTROL_ROOT/commands"
RESULTS_DIR="$CONTROL_ROOT/results"
STATE_PATH="$CONTROL_ROOT/state.json"
TIMEOUT_SECONDS="${ALAN_PERF_DIAG_TIMEOUT_SECONDS:-60}"
SAMPLE_SECONDS="${ALAN_PERF_DIAG_SAMPLE_SECONDS:-8}"
KEEP_APP="${ALAN_PERF_DIAG_KEEP_APP:-0}"
APP_PID=""
CONTROL_INDEX=0

fail() {
    printf 'capture-performance-diagnostics-workload: %s\n' "$*" >&2
    exit 1
}

info() {
    printf 'capture-performance-diagnostics-workload: %s\n' "$*"
}

append_manifest() {
    printf '%s\n' "$*" >>"$MANIFEST_PATH"
}

cleanup() {
    if [[ -n "${APP_PID:-}" && "$KEEP_APP" != "1" ]] && kill -0 "$APP_PID" >/dev/null 2>&1; then
        kill "$APP_PID" >/dev/null 2>&1 || true
    fi
}
trap cleanup EXIT

json_get() {
    local file="$1"
    local key="$2"
    plutil -extract "$key" raw -o - "$file" 2>/dev/null || true
}

json_escape_text() {
    awk '
        BEGIN { ORS = "" }
        {
            gsub(/\\/, "\\\\")
            gsub(/"/, "\\\"")
            if (NR > 1) {
                printf "\\n"
            }
            printf "%s", $0
        }
    '
}

wait_for_path() {
    local path="$1"
    local label="$2"
    local deadline=$((SECONDS + TIMEOUT_SECONDS))
    while (( SECONDS < deadline )); do
        if [[ -e "$path" ]]; then
            return 0
        fi
        if [[ -n "${APP_PID:-}" ]] && ! kill -0 "$APP_PID" >/dev/null 2>&1; then
            fail "Alan exited while waiting for $label"
        fi
        sleep 0.2
    done
    fail "timed out waiting for $label"
}

next_request_id() {
    local label="$1"
    printf 'perf-diag-%s-%s-%05d' "$label" "$(date +%s%N)" "$RANDOM"
}

send_control_json() {
    local request_id="$1"
    local payload="$2"
    local command_path="$COMMANDS_DIR/$request_id.json"
    local result_path="$RESULTS_DIR/$request_id.json"

    wait_for_path "$COMMANDS_DIR" "control commands directory"
    wait_for_path "$RESULTS_DIR" "control results directory"
    printf '%s\n' "$payload" >"$command_path.tmp"
    mv "$command_path.tmp" "$command_path"
    wait_for_path "$result_path" "control result $request_id"
    printf '%s\n' "$result_path"
}

require_control_applied() {
    local result_path="$1"
    local label="$2"
    local applied
    applied=$(json_get "$result_path" applied)
    if [[ "$applied" != "true" ]]; then
        fail "$label failed: $(json_get "$result_path" error_code) $(json_get "$result_path" error_message)"
    fi
}

control_state() {
    local request_id
    request_id=$(next_request_id state)
    send_control_json "$request_id" "{
  \"request_id\": \"$request_id\",
  \"command\": \"state\"
}"
}

control_enable_diagnostics() {
    local request_id
    request_id=$(next_request_id diagnostics-enable)
    send_control_json "$request_id" "{
  \"request_id\": \"$request_id\",
  \"command\": \"performance_diagnostics.set_enabled\",
  \"enabled\": true
}"
}

control_disable_diagnostics() {
    local request_id
    request_id=$(next_request_id diagnostics-disable)
    send_control_json "$request_id" "{
  \"request_id\": \"$request_id\",
  \"command\": \"performance_diagnostics.set_enabled\",
  \"enabled\": false
}"
}

control_tab_open() {
    local title="$1"
    local request_id
    request_id=$(next_request_id tab-open)
    send_control_json "$request_id" "{
  \"request_id\": \"$request_id\",
  \"command\": \"tab.open\",
  \"title\": \"$title\"
}"
}

control_send_text() {
    local pane_slot_id="$1"
    local text="$2"
    local request_id
    local text_json
    request_id=$(next_request_id terminal-send)
    text_json=$(printf '%s' "$text" | json_escape_text)
    send_control_json "$request_id" "{
  \"request_id\": \"$request_id\",
  \"command\": \"terminal.send_text\",
  \"pane_slot_id\": \"$pane_slot_id\",
  \"text\": \"$text_json\"
}"
}

control_send_return_key() {
    local pane_slot_id="$1"
    local request_id
    request_id=$(next_request_id terminal-return)
    send_control_json "$request_id" "{
  \"request_id\": \"$request_id\",
  \"command\": \"terminal.send_key\",
  \"pane_slot_id\": \"$pane_slot_id\",
  \"key\": \"return\"
}"
}

send_terminal_payload_until_applied() {
    local pane_slot_id="$1"
    local text="$2"
    local label="$3"
    local deadline=$((SECONDS + TIMEOUT_SECONDS))
    local result_path=""
    while (( SECONDS < deadline )); do
        result_path=$(control_send_text "$pane_slot_id" "$text")
        if [[ "$(json_get "$result_path" applied)" == "true" ]]; then
            local key_result_path=""
            while (( SECONDS < deadline )); do
                key_result_path=$(control_send_return_key "$pane_slot_id")
                if [[ "$(json_get "$key_result_path" applied)" == "true" ]]; then
                    printf '%s\n' "$key_result_path"
                    return 0
                fi
                sleep 0.5
            done
            fail "terminal.send_key $label failed: $(json_get "$key_result_path" error_code) $(json_get "$key_result_path" error_message)"
        fi
        sleep 0.5
    done
    [[ -n "$result_path" ]] || fail "terminal.send_text $label did not return a result"
    fail "terminal.send_text $label failed: $(json_get "$result_path" error_code) $(json_get "$result_path" error_message)"
}

control_record_child_pressure() {
    local cpu_percent="$1"
    local memory_bytes="$2"
    local request_id
    request_id=$(next_request_id child-pressure)
    send_control_json "$request_id" "{
  \"request_id\": \"$request_id\",
  \"command\": \"performance_diagnostics.record_child_pressure\",
  \"child_process_role\": \"terminal_child\",
  \"child_cpu_percent\": $cpu_percent,
  \"child_memory_bytes\": $memory_bytes
}"
}

control_export() {
    local request_id
    request_id=$(next_request_id diagnostics-export)
    send_control_json "$request_id" "{
  \"request_id\": \"$request_id\",
  \"command\": \"performance_diagnostics.export_recent\",
  \"export_directory\": \"$(printf '%s' "$EXPORT_DIR" | json_escape_text)\"
}"
}

workload_payload() {
    local label="$1"
    local pid_file="$2"
    cat <<PAYLOAD
/bin/sh -lc '(echo "alan-perf-diag $0 start"; if command -v codex >/dev/null 2>&1; then codex --version 2>/dev/null || true; else echo "codex cli unavailable"; fi; i=0; while [ "\$i" -lt 3500 ]; do i=\$((i + 1)); printf "alan-perf-diag-codex-$0 line-%04d abcdefghijklmnopqrstuvwxyz0123456789\\n" "\$i"; if [ \$((i % 125)) -eq 0 ]; then /usr/bin/shasum -a 256 /bin/ls >/dev/null 2>&1 || true; fi; done; sleep 2; echo "alan-perf-diag $0 done") & echo \$! > "\$1"; wait' "$label" "$pid_file"
PAYLOAD
}

wait_for_workload_pids() {
    local expected="$1"
    local deadline=$((SECONDS + TIMEOUT_SECONDS))
    while (( SECONDS < deadline )); do
        local count
        count=$(find "$PIDS_DIR" -name '*.pid' -type f 2>/dev/null | wc -l | tr -d ' ')
        if [[ "$count" -ge "$expected" ]]; then
            return 0
        fi
        sleep 0.25
    done
    fail "timed out waiting for terminal workload pid files"
}

sample_child_pressure_once() {
    local cpu_total="0"
    local rss_total="0"
    local found=0
    local pid_file
    for pid_file in "$PIDS_DIR"/*.pid; do
        [[ -f "$pid_file" ]] || continue
        local pid
        pid=$(tr -dc '0-9' <"$pid_file")
        [[ -n "$pid" ]] || continue
        if ! kill -0 "$pid" >/dev/null 2>&1; then
            continue
        fi
        local sample
        sample=$(ps -p "$pid" -o pcpu= -o rss= 2>/dev/null | awk 'NF >= 2 { print $1, $2; exit }')
        [[ -n "$sample" ]] || continue
        found=1
        cpu_total=$(awk -v a="$cpu_total" -v b="$(printf '%s' "$sample" | awk '{print $1}')" 'BEGIN { printf "%.2f", a + b }')
        rss_total=$(awk -v a="$rss_total" -v b="$(printf '%s' "$sample" | awk '{print $2}')" 'BEGIN { printf "%.0f", a + b }')
    done

    [[ "$found" == "1" ]] || return 1
    local memory_bytes
    memory_bytes=$(awk -v rss="$rss_total" 'BEGIN { printf "%.0f", rss * 1024 }')
    local result_path
    result_path=$(control_record_child_pressure "$cpu_total" "$memory_bytes")
    require_control_applied "$result_path" "performance_diagnostics.record_child_pressure"
    return 0
}

sample_child_pressure() {
    local deadline=$((SECONDS + SAMPLE_SECONDS))
    local recorded=0
    while (( SECONDS < deadline )); do
        if sample_child_pressure_once; then
            recorded=1
        fi
        sleep 1
    done
    [[ "$recorded" == "1" ]] || fail "no live child process CPU samples were recorded"
}

require_contains() {
    local file="$1"
    local pattern="$2"
    local label="$3"
    if ! grep -E "$pattern" "$file" >/dev/null 2>&1; then
        fail "missing $label in exported diagnostics"
    fi
}

count_pattern() {
    local file="$1"
    local pattern="$2"
    local count
    count=$(grep -E -c "$pattern" "$file" 2>/dev/null || true)
    printf '%s\n' "${count:-0}"
}

mkdir -p "$OUTPUT_DIR" "$SMOKE_OUTPUT_DIR" "$EXPORT_DIR" "$PIDS_DIR"
: >"$MANIFEST_PATH"
append_manifest "run_id=$RUN_ID"
append_manifest "control_namespace=$CONTROL_NAMESPACE"
append_manifest "output_dir=$OUTPUT_DIR"

info "launching controlled Dev-channel Alan app"
ALAN_SHELL_CONTROL_NAMESPACE="$CONTROL_NAMESPACE" \
ALAN_UI_SMOKE_DERIVED_DATA="$UI_SMOKE_DERIVED_DATA" \
ALAN_UI_SMOKE_APP_PATH="$UI_SMOKE_APP_PATH" \
ALAN_UI_SMOKE_APP_EXECUTABLE="$UI_SMOKE_APP_EXECUTABLE" \
ALAN_UI_SMOKE_OUTPUT_DIR="$SMOKE_OUTPUT_DIR" \
ALAN_UI_SMOKE_LAUNCH_MODE="$LAUNCH_MODE" \
ALAN_UI_SMOKE_SKIP_BUILD="$UI_SMOKE_SKIP_BUILD" \
ALAN_UI_SMOKE_KEEP_RUNNING=1 \
ALAN_UI_SMOKE_KEEP_RUNTIME_TMP=1 \
ALAN_UI_SMOKE_TERMINAL_STEPS=never \
ALAN_UI_SMOKE_UI_SCRIPTING_STEPS=never \
ALAN_UI_SMOKE_RESTART_RESTORE_STEPS=never \
"$SCRIPT_DIR/test-shell-ui-smoke.sh" \
    --keep-running \
    --terminal-steps never \
    --ui-scripting-steps never \
    --restart-restore-steps never

APP_PID=$(sed -n 's/^pid=//p' "$SMOKE_OUTPUT_DIR/manifest.txt" | tail -1)
[[ -n "$APP_PID" ]] || fail "UI smoke manifest did not include app pid"
append_manifest "pid=$APP_PID"

wait_for_path "$STATE_PATH" "shell state"

enable_result=$(control_enable_diagnostics)
require_control_applied "$enable_result" "performance_diagnostics.set_enabled"

state_result=$(control_state)
require_control_applied "$state_result" "state"
target_slots=()
focused_slot=$(json_get "$state_result" focused_pane_slot_id)
if [[ -n "$focused_slot" ]]; then
    target_slots+=("$focused_slot")
fi

for label in A B C; do
    tab_result=$(control_tab_open "Codex workload $label")
    require_control_applied "$tab_result" "tab.open"
    slot=$(json_get "$tab_result" pane_slot_id)
    [[ -n "$slot" ]] || slot=$(json_get "$tab_result" pane_id)
    [[ -n "$slot" ]] || fail "tab.open did not return a terminal target for workload $label"
    target_slots+=("$slot")
done

index=0
for slot in "${target_slots[@]}"; do
    index=$((index + 1))
    label="codex-$index"
    pid_file="$PIDS_DIR/$label.pid"
    payload=$(workload_payload "$label" "$pid_file")
    send_result=$(send_terminal_payload_until_applied "$slot" "$payload" "$label")
    require_control_applied "$send_result" "terminal.send_text $label"
done

wait_for_workload_pids "${#target_slots[@]}"
sample_child_pressure
sleep 2

export_result=$(control_export)
require_control_applied "$export_result" "performance_diagnostics.export_recent"
bundle_path=$(json_get "$export_result" diagnostics_bundle_path)
[[ -n "$bundle_path" ]] || fail "export response did not include diagnostics_bundle_path"
append_manifest "bundle_path=$bundle_path"

disable_result=$(control_disable_diagnostics)
require_control_applied "$disable_result" "performance_diagnostics.set_enabled false"

events_path="$bundle_path/events.jsonl"
summary_path="$bundle_path/summary.json"
[[ -f "$events_path" ]] || fail "missing events.jsonl in exported bundle"
[[ -f "$summary_path" ]] || fail "missing summary.json in exported bundle"

require_contains "$events_path" '"kind"[[:space:]]*:[[:space:]]*"shellRuntimeProjection"' "shell projection events"
require_contains "$events_path" '"kind"[[:space:]]*:[[:space:]]*"runtimeSnapshotPublish"' "runtime publication events"
require_contains "$events_path" '"kind"[[:space:]]*:[[:space:]]*"ghostty(AppTick|SurfaceRefresh|Wakeup)"' "Ghostty timing events"
require_contains "$summary_path" '"role"[[:space:]]*:[[:space:]]*"terminalChild"' "terminal child CPU pressure"

if grep -F 'alan-perf-diag' "$events_path" "$summary_path" >/dev/null 2>&1; then
    fail "exported diagnostics leaked terminal workload text"
fi
if grep -F "$REPO_ROOT" "$events_path" "$summary_path" >/dev/null 2>&1; then
    fail "exported diagnostics leaked repository path"
fi
if grep -E 'command(Line)?|workingDirectory|environment|OPENAI_API_KEY|refresh-token' \
    "$events_path" "$summary_path" >/dev/null 2>&1
then
    fail "exported diagnostics contained forbidden command/path/environment/secret fields"
fi

ghostty_count=$(count_pattern "$events_path" '"kind"[[:space:]]*:[[:space:]]*"ghostty(AppTick|SurfaceRefresh|Wakeup)"')
runtime_count=$(count_pattern "$events_path" '"kind"[[:space:]]*:[[:space:]]*"runtimeSnapshotPublish"')
shell_count=$(count_pattern "$events_path" '"kind"[[:space:]]*:[[:space:]]*"shellRuntimeProjection"')
stutter_count=$(count_pattern "$events_path" '"kind"[[:space:]]*:[[:space:]]*"automaticStutterMarker"')
child_count=$(count_pattern "$summary_path" '"role"[[:space:]]*:[[:space:]]*"terminalChild"')

{
    printf '# Real Workload Diagnostics Review\n\n'
    printf -- '- Run ID: `%s`\n' "$RUN_ID"
    printf -- '- Bundle: `%s`\n' "$bundle_path"
    printf -- '- Ghostty timing events: `%s`\n' "$ghostty_count"
    printf -- '- Runtime publication events: `%s`\n' "$runtime_count"
    printf -- '- Shell projection events: `%s`\n' "$shell_count"
    printf -- '- Automatic stutter markers: `%s`\n' "$stutter_count"
    printf -- '- Terminal child process samples: `%s`\n\n' "$child_count"
    printf 'Privacy review passed: exported `events.jsonl` and `summary.json` did not contain terminal workload text, repo paths, command-line fields, cwd fields, environment fields, or secret fixture strings.\n'
} >"$REVIEW_PATH"

append_manifest "review_path=$REVIEW_PATH"
info "diagnostics bundle: $bundle_path"
info "review: $REVIEW_PATH"
