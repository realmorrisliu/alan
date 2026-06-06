#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/../../.." && pwd)
DERIVED_DATA="${ALAN_UI_SMOKE_DERIVED_DATA:-$REPO_ROOT/debug/DerivedData/apple-shell-ui-smoke}"
OUTPUT_DIR="${ALAN_UI_SMOKE_OUTPUT_DIR:-$REPO_ROOT/debug/artifacts/apple-shell-ui-smoke}"
DEFAULT_APP_HOME="${HOME:-/Users/${USER:-$(id -un)}}"
DEFAULT_APP_PATH="$DEFAULT_APP_HOME/Applications/Alan Dev.app"
DEFAULT_BUILT_APP_PATH="$DERIVED_DATA/Build/Products/Debug/Alan.app"
DEFAULT_APP_EXECUTABLE="$DEFAULT_APP_PATH/Contents/MacOS/Alan Dev"
CUSTOM_SMOKE_TMPDIR=0
CUSTOM_CONTROL_NAMESPACE=0
CUSTOM_APP_SUPPORT_DIR=0
CUSTOM_APP_PATH=0
CUSTOM_APP_EXECUTABLE=0
[[ -n "${ALAN_UI_SMOKE_TMPDIR:-}" ]] && CUSTOM_SMOKE_TMPDIR=1
[[ -n "${ALAN_SHELL_CONTROL_NAMESPACE:-}" ]] && CUSTOM_CONTROL_NAMESPACE=1
[[ -n "${ALAN_UI_SMOKE_APP_SUPPORT_DIR:-}" ]] && CUSTOM_APP_SUPPORT_DIR=1
[[ -n "${ALAN_UI_SMOKE_APP_PATH:-}" ]] && CUSTOM_APP_PATH=1
[[ -n "${ALAN_UI_SMOKE_APP_EXECUTABLE:-}" ]] && CUSTOM_APP_EXECUTABLE=1
SMOKE_TMPDIR="${ALAN_UI_SMOKE_TMPDIR:-$OUTPUT_DIR/tmp}"
SYSTEM_TMPDIR="${ALAN_UI_SMOKE_SYSTEM_TMPDIR:-$(getconf DARWIN_USER_TEMP_DIR 2>/dev/null || printf '%s' "${TMPDIR:-/tmp}")}"
LAUNCH_MODE="${ALAN_UI_SMOKE_LAUNCH_MODE:-open}"
DEFAULT_CONTROL_NAMESPACE="alan-ui-smoke-shell-control-$$"
CHANNEL_CONTROL_NAMESPACE="alan-dev-shell-control"
sanitize_control_namespace() {
    local raw="$1"
    local trimmed
    local sanitized
    trimmed=$(printf '%s' "$raw" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')
    if [[ -z "$trimmed" ]]; then
        printf '%s\n' "$CHANNEL_CONTROL_NAMESPACE"
        return
    fi

    sanitized=$(
        printf '%s' "$trimmed" \
            | sed -e 's/[^[:alnum:]_.-]/-/g' -e 's/^[-_.]*//' -e 's/[-_.]*$//'
    )
    if [[ -z "$sanitized" ]]; then
        printf '%s\n' "$CHANNEL_CONTROL_NAMESPACE"
    else
        printf '%s\n' "$sanitized"
    fi
}
CONTROL_NAMESPACE="$(sanitize_control_namespace "${ALAN_SHELL_CONTROL_NAMESPACE:-$DEFAULT_CONTROL_NAMESPACE}")"
SMOKE_APP_SUPPORT_DIR="${ALAN_UI_SMOKE_APP_SUPPORT_DIR:-${SYSTEM_TMPDIR%/}/$CONTROL_NAMESPACE-app-support}"
control_tmpdir() {
    if [[ "$LAUNCH_MODE" == "direct" ]]; then
        printf '%s\n' "$SMOKE_TMPDIR"
    else
        printf '%s\n' "$SYSTEM_TMPDIR"
    fi
}
CONTROL_TMPDIR="$(control_tmpdir)"
CONTROL_ROOT="${CONTROL_TMPDIR%/}/$CONTROL_NAMESPACE/window_main"
COMMANDS_DIR="$CONTROL_ROOT/commands"
RESULTS_DIR="$CONTROL_ROOT/results"
STATE_PATH="$CONTROL_ROOT/state.json"
APP_PATH="${ALAN_UI_SMOKE_APP_PATH:-$DEFAULT_APP_PATH}"
APP_EXECUTABLE="${ALAN_UI_SMOKE_APP_EXECUTABLE:-$DEFAULT_APP_EXECUTABLE}"
CAPTURE="$REPO_ROOT/clients/apple/scripts/capture-alan-window.sh"
TIMEOUT_SECONDS="${ALAN_UI_SMOKE_TIMEOUT_SECONDS:-20}"
CAPTURE_TIMEOUT_SECONDS="${ALAN_UI_SMOKE_CAPTURE_TIMEOUT_SECONDS:-6}"
SKIP_BUILD="${ALAN_UI_SMOKE_SKIP_BUILD:-1}"
KEEP_RUNNING="${ALAN_UI_SMOKE_KEEP_RUNNING:-0}"
KEEP_RUNTIME_TMP="${ALAN_UI_SMOKE_KEEP_RUNTIME_TMP:-0}"
RUN_TERMINAL_STEPS="${ALAN_UI_SMOKE_TERMINAL_STEPS:-auto}"
UI_SCRIPTING_STEPS="${ALAN_UI_SMOKE_UI_SCRIPTING_STEPS:-auto}"
RUN_RESTART_RESTORE_STEPS="${ALAN_UI_SMOKE_RESTART_RESTORE_STEPS:-auto}"
REQUIRE_TERMINAL_STEPS="${ALAN_REQUIRE_TERMINAL_UI_SMOKE:-0}"
REQUIRE_UI_SCRIPTING_STEPS="${ALAN_REQUIRE_UI_SCRIPTING_UI_SMOKE:-0}"
REQUIRE_RESTART_RESTORE_STEPS="${ALAN_REQUIRE_RESTART_RESTORE_UI_SMOKE:-0}"
SMOKE_BUNDLE_ID="${ALAN_UI_SMOKE_BUNDLE_ID:-}"
SMOKE_DISPLAY_NAME="${ALAN_UI_SMOKE_DISPLAY_NAME:-Alan UI Smoke}"
SMOKE_HOME="$OUTPUT_DIR/home"
MANIFEST_PATH="$OUTPUT_DIR/manifest.txt"
RESTART_RESTORE_CWD="$OUTPUT_DIR/restart-restore-cwd"
RESTART_RESTORE_BEFORE_TOKEN="alan-ui-smoke-restart-before"
RESTART_RESTORE_AFTER_TOKEN="alan-ui-smoke-restart-after"
RESTART_RESTORE_PWD_FILE="alan-ui-smoke-after-pwd.txt"
APP_PID=""
CAPTURE_INDEX=0
CONTROL_INDEX=0
LAUNCHCTL_ENV_KEYS=""

fail() {
    printf 'test-shell-ui-smoke: %s\n' "$*" >&2
    exit 1
}

info() {
    printf 'test-shell-ui-smoke: %s\n' "$*"
}

usage() {
    cat <<'USAGE'
Usage: clients/apple/scripts/test-shell-ui-smoke.sh [options]

Options:
  --skip-build                Reuse ALAN_UI_SMOKE_APP_PATH or the installed Alan Dev app.
  --app <path>                Launch this alan-macos.app bundle.
  --output-dir <path>         Write screenshots and the manifest here.
  --keep-running              Leave the launched app running after the smoke flow.
  --terminal-steps <mode>     auto, always, or never. Default: auto.
  --ui-scripting-steps <mode> auto, always, or never. Default: auto.
  --restart-restore-steps <mode>
                               auto, always, or never. Default: auto.
  --help                      Show this help text.

Environment:
  ALAN_UI_SMOKE_SKIP_BUILD=0 builds and launches the repo-local Debug Alan.app.

  ALAN_REQUIRE_TERMINAL_UI_SMOKE=1 makes terminal-specific smoke steps fail
  when local Ghostty artifacts are missing or terminal delivery is unavailable.

  ALAN_REQUIRE_UI_SCRIPTING_UI_SMOKE=1 makes command UI, keyboard switching,
  and pane-scoped Find checks fail unless Accessibility is available.

  ALAN_REQUIRE_RESTART_RESTORE_UI_SMOKE=1 makes restart transcript restore
  checks fail unless terminal delivery and Accessibility are both available.
USAGE
}

refresh_derived_paths() {
    SMOKE_HOME="$OUTPUT_DIR/home"
    MANIFEST_PATH="$OUTPUT_DIR/manifest.txt"
    RESTART_RESTORE_CWD="$OUTPUT_DIR/restart-restore-cwd"
    if [[ "$CUSTOM_SMOKE_TMPDIR" != "1" ]]; then
        SMOKE_TMPDIR="$OUTPUT_DIR/tmp"
    fi
    if [[ "$CUSTOM_APP_SUPPORT_DIR" != "1" ]]; then
        SMOKE_APP_SUPPORT_DIR="${SYSTEM_TMPDIR%/}/$CONTROL_NAMESPACE-app-support"
    fi
    CONTROL_TMPDIR="$(control_tmpdir)"
    CONTROL_ROOT="${CONTROL_TMPDIR%/}/$CONTROL_NAMESPACE/window_main"
    COMMANDS_DIR="$CONTROL_ROOT/commands"
    RESULTS_DIR="$CONTROL_ROOT/results"
    STATE_PATH="$CONTROL_ROOT/state.json"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --skip-build)
            SKIP_BUILD=1
            ;;
        --app)
            shift
            [[ $# -gt 0 ]] || fail "missing value after --app"
            APP_PATH="$1"
            CUSTOM_APP_PATH=1
            if [[ "$CUSTOM_APP_EXECUTABLE" != "1" ]]; then
                APP_EXECUTABLE=""
            fi
            ;;
        --output-dir)
            shift
            [[ $# -gt 0 ]] || fail "missing value after --output-dir"
            OUTPUT_DIR="$1"
            refresh_derived_paths
            ;;
        --keep-running)
            KEEP_RUNNING=1
            ;;
        --terminal-steps)
            shift
            [[ $# -gt 0 ]] || fail "missing value after --terminal-steps"
            RUN_TERMINAL_STEPS="$1"
            ;;
        --ui-scripting-steps)
            shift
            [[ $# -gt 0 ]] || fail "missing value after --ui-scripting-steps"
            UI_SCRIPTING_STEPS="$1"
            ;;
        --restart-restore-steps)
            shift
            [[ $# -gt 0 ]] || fail "missing value after --restart-restore-steps"
            RUN_RESTART_RESTORE_STEPS="$1"
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            fail "unknown argument: $1"
            ;;
    esac
    shift
done

resolve_app_path() {
    if [[ "$CUSTOM_APP_PATH" == "1" ]]; then
        return
    fi
    if [[ "$SKIP_BUILD" == "1" ]]; then
        APP_PATH="$DEFAULT_APP_PATH"
    else
        APP_PATH="$DEFAULT_BUILT_APP_PATH"
    fi
}

bundle_identifier_for() {
    local app_path="$1"
    plutil -extract CFBundleIdentifier raw -o - "$app_path/Contents/Info.plist" 2>/dev/null || true
}

executable_name_for() {
    local app_path="$1"
    local executable
    executable=$(plutil -extract CFBundleExecutable raw -o - "$app_path/Contents/Info.plist" 2>/dev/null || true)
    if [[ -n "$executable" ]]; then
        printf '%s\n' "$executable"
        return
    fi
    if [[ "$(basename "$app_path")" == "Alan Dev.app" ]]; then
        printf 'Alan Dev\n'
    else
        printf 'Alan\n'
    fi
}

resolve_app_executable() {
    if [[ "$CUSTOM_APP_EXECUTABLE" == "1" ]]; then
        return
    fi
    APP_EXECUTABLE="$APP_PATH/Contents/MacOS/$(executable_name_for "$APP_PATH")"
}

resolve_smoke_bundle_id() {
    if [[ -n "$SMOKE_BUNDLE_ID" ]]; then
        return
    fi
    if [[ "$SKIP_BUILD" == "1" ]]; then
        SMOKE_BUNDLE_ID="$(bundle_identifier_for "$APP_PATH")"
    fi
    SMOKE_BUNDLE_ID="${SMOKE_BUNDLE_ID:-app.alanworks.macos.ui-smoke}"
}

case "$RUN_TERMINAL_STEPS" in
    auto|always|never) ;;
    *) fail "--terminal-steps must be auto, always, or never" ;;
esac

if [[ "$REQUIRE_UI_SCRIPTING_STEPS" == "1" && "$UI_SCRIPTING_STEPS" == "auto" ]]; then
    UI_SCRIPTING_STEPS=always
fi

case "$UI_SCRIPTING_STEPS" in
    auto|always|never) ;;
    *) fail "--ui-scripting-steps must be auto, always, or never" ;;
esac

if [[ "$REQUIRE_RESTART_RESTORE_STEPS" == "1" && "$RUN_RESTART_RESTORE_STEPS" == "auto" ]]; then
    RUN_RESTART_RESTORE_STEPS=always
fi

case "$RUN_RESTART_RESTORE_STEPS" in
    auto|always|never) ;;
    *) fail "--restart-restore-steps must be auto, always, or never" ;;
esac

case "$LAUNCH_MODE" in
    open|direct) ;;
    *) fail "ALAN_UI_SMOKE_LAUNCH_MODE must be open or direct" ;;
esac

resolve_app_path
command -v plutil >/dev/null 2>&1 || fail "plutil is required for UI smoke"
resolve_app_executable
resolve_smoke_bundle_id
if [[ "$SKIP_BUILD" != "1" ]]; then
    command -v xcodebuild >/dev/null 2>&1 \
        || fail "xcodebuild is required to build the UI smoke app; pass --skip-build --app /path/to/Alan.app to reuse a built app"
fi
[[ -x "$CAPTURE" ]] || fail "missing capture helper: $CAPTURE"

ghostty_ready=0
if "$REPO_ROOT/clients/apple/scripts/setup-local-ghosttykit.sh" --check >/dev/null 2>&1; then
    ghostty_ready=1
fi

if [[ "$SKIP_BUILD" != "1" && "$ghostty_ready" != "1" ]]; then
    fail "Ghostty artifacts are required to build alan-macos for UI smoke; run clients/apple/scripts/setup-local-ghosttykit.sh or pass --skip-build --app /path/to/alan-macos.app"
fi

if [[ "$SKIP_BUILD" == "1" && "$CUSTOM_APP_PATH" != "1" && ! -d "$APP_PATH" ]]; then
    fail "installed Alan Dev app not found: $APP_PATH; run just install-dev first"
fi

mkdir -p "$OUTPUT_DIR"
rm -rf "$SMOKE_HOME"
mkdir -p "$SMOKE_HOME"
rm -rf "$RESTART_RESTORE_CWD"
mkdir -p "$RESTART_RESTORE_CWD"
if [[ "$CUSTOM_SMOKE_TMPDIR" != "1" ]]; then
    rm -rf "$SMOKE_TMPDIR"
fi
mkdir -p "$SMOKE_TMPDIR"
if [[ "$CUSTOM_APP_SUPPORT_DIR" != "1" ]]; then
    rm -rf "$SMOKE_APP_SUPPORT_DIR"
fi
mkdir -p "$SMOKE_APP_SUPPORT_DIR"
: >"$MANIFEST_PATH"

cleanup() {
    if [[ -n "${APP_PID:-}" && "$KEEP_RUNNING" != "1" ]]; then
        kill "$APP_PID" >/dev/null 2>&1 || true
    fi
    clear_launch_env
    if [[ "$KEEP_RUNNING" != "1" && "$KEEP_RUNTIME_TMP" != "1" && "$CUSTOM_SMOKE_TMPDIR" != "1" ]]; then
        rm -rf "$SMOKE_TMPDIR"
    fi
    if [[ "$KEEP_RUNNING" != "1" && "$KEEP_RUNTIME_TMP" != "1" && "$CUSTOM_CONTROL_NAMESPACE" != "1" ]]; then
        rm -rf "${CONTROL_TMPDIR%/}/$CONTROL_NAMESPACE"
    fi
    if [[ "$KEEP_RUNNING" != "1" && "$KEEP_RUNTIME_TMP" != "1" && "$CUSTOM_APP_SUPPORT_DIR" != "1" ]]; then
        rm -rf "$SMOKE_APP_SUPPORT_DIR"
    fi
}
trap cleanup EXIT

append_manifest() {
    printf '%s\n' "$*" >>"$MANIFEST_PATH"
}

json_get() {
    local file="$1"
    local key="$2"
    plutil -extract "$key" raw -o - "$file" 2>/dev/null || true
}

require_control_applied() {
    local result_path="$1"
    local label="$2"
    local applied
    applied=$(json_get "$result_path" applied)
    if [[ "$applied" != "true" ]]; then
        local code
        local message
        code=$(json_get "$result_path" error_code)
        message=$(json_get "$result_path" error_message)
        fail "$label failed: ${code:-unknown_error} ${message:-}"
    fi
}

next_request_id() {
    local label="$1"
    CONTROL_INDEX=$((CONTROL_INDEX + 1))
    printf 'ui-smoke-%02d-%s' "$CONTROL_INDEX" "$label"
}

wait_for_file() {
    local path="$1"
    local label="$2"
    local deadline=$((SECONDS + TIMEOUT_SECONDS))
    while (( SECONDS < deadline )); do
        if [[ -e "$path" ]]; then
            return 0
        fi
        if [[ -n "${APP_PID:-}" ]] && ! kill -0 "$APP_PID" >/dev/null 2>&1; then
            fail "alan smoke app exited while waiting for $label"
        fi
        sleep 0.2
    done
    fail "timed out waiting for $label"
}

send_control_json() {
    local request_id="$1"
    local payload="$2"
    local command_path="$COMMANDS_DIR/$request_id.json"
    local result_path="$RESULTS_DIR/$request_id.json"

    wait_for_file "$COMMANDS_DIR" "control commands directory"
    wait_for_file "$RESULTS_DIR" "control results directory"
    printf '%s\n' "$payload" >"$command_path.tmp"
    mv "$command_path.tmp" "$command_path"
    wait_for_file "$result_path" "control result $request_id"
    printf '%s\n' "$result_path"
}

json_escape_fragment() {
    printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g'
}

set_launch_env() {
    local key="$1"
    local value="$2"
    launchctl setenv "$key" "$value"
    LAUNCHCTL_ENV_KEYS="$LAUNCHCTL_ENV_KEYS $key"
}

clear_launch_env() {
    local key
    for key in $LAUNCHCTL_ENV_KEYS; do
        launchctl unsetenv "$key" >/dev/null 2>&1 || true
    done
    LAUNCHCTL_ENV_KEYS=""
}

find_app_pid() {
    /bin/ps -axo pid=,command= | awk -v executable="$APP_EXECUTABLE" '
        BEGIN {
            target = tolower(executable)
        }
        {
            pid = $1
            sub(/^[[:space:]]*[0-9]+[[:space:]]+/, "", $0)
            if (index(tolower($0), target) == 1) {
                print pid
            }
        }
    ' | tail -1
}

wait_for_app_pid() {
    local deadline=$((SECONDS + TIMEOUT_SECONDS))
    while (( SECONDS < deadline )); do
        APP_PID=$(find_app_pid)
        if [[ -n "$APP_PID" ]] && kill -0 "$APP_PID" >/dev/null 2>&1; then
            printf '%s' "$APP_PID" >"$OUTPUT_DIR/alan-ui-smoke.pid"
            return 0
        fi
        sleep 0.2
    done
    fail "timed out waiting for launched alan smoke app process"
}

wait_for_app_exit() {
    local label="$1"
    local deadline=$((SECONDS + TIMEOUT_SECONDS))
    while [[ -n "${APP_PID:-}" ]] && kill -0 "$APP_PID" >/dev/null 2>&1; do
        if (( SECONDS >= deadline )); then
            fail "timed out waiting for alan smoke app to exit after $label"
        fi
        sleep 0.2
    done
    APP_PID=""
}

install_launch_environment() {
    set_launch_env ALAN_MACOS_APPLICATION_SUPPORT_DIR "$SMOKE_APP_SUPPORT_DIR"
    set_launch_env ALAN_INSTALL_CHANNEL dev
    set_launch_env ALAN_SHELL_CONTROL_NAMESPACE "$CONTROL_NAMESPACE"
    set_launch_env SHELL /bin/zsh
}

launch_smoke_app_direct() {
    (
        cd "$REPO_ROOT"
        export HOME="$SMOKE_HOME"
        export TMPDIR="${SMOKE_TMPDIR%/}/"
        export ALAN_MACOS_APPLICATION_SUPPORT_DIR="$SMOKE_APP_SUPPORT_DIR"
        export ALAN_INSTALL_CHANNEL=dev
        export ALAN_SHELL_CONTROL_NAMESPACE="$CONTROL_NAMESPACE"
        export SHELL=/bin/zsh
        "$APP_EXECUTABLE" >"$OUTPUT_DIR/alan-ui-smoke.stdout.log" 2>"$OUTPUT_DIR/alan-ui-smoke.stderr.log" &
        printf '%s' "$!" >"$OUTPUT_DIR/alan-ui-smoke.pid"
    )
    APP_PID=$(cat "$OUTPUT_DIR/alan-ui-smoke.pid")
}

launch_smoke_app_open() {
    command -v open >/dev/null 2>&1 || fail "open is required for LaunchServices smoke launch"
    command -v launchctl >/dev/null 2>&1 || fail "launchctl is required for LaunchServices smoke launch"
    : >"$OUTPUT_DIR/alan-ui-smoke.stdout.log"
    : >"$OUTPUT_DIR/alan-ui-smoke.stderr.log"
    install_launch_environment
    open -n "$APP_PATH" --args -ApplePersistenceIgnoreState YES
    wait_for_app_pid
    clear_launch_env
}

control_state() {
    local request_id
    request_id=$(next_request_id state)
    send_control_json "$request_id" "{
  \"request_id\": \"$request_id\",
  \"command\": \"state\"
}"
}

control_space_create() {
    local request_id
    request_id=$(next_request_id space-create)
    send_control_json "$request_id" "{
  \"request_id\": \"$request_id\",
  \"command\": \"space.create\",
  \"title\": \"Smoke Space\"
}"
}

control_tab_open() {
    local request_id
    request_id=$(next_request_id tab-open)
    send_control_json "$request_id" "{
  \"request_id\": \"$request_id\",
  \"command\": \"tab.open\",
  \"title\": \"Smoke Tab\"
}"
}

control_pane_split() {
    local pane_id="$1"
    local request_id
    request_id=$(next_request_id pane-split)
    send_control_json "$request_id" "{
  \"request_id\": \"$request_id\",
  \"command\": \"pane.split\",
  \"pane_id\": \"$pane_id\",
  \"direction\": \"vertical\"
}"
}

control_terminal_send_text() {
    local pane_slot_id="$1"
    control_terminal_send_text_payload "$pane_slot_id" "printf \\\"alan-ui-smoke-input-ok\\\\n\\\"\\n" "terminal-send"
}

control_terminal_send_text_payload() {
    local pane_slot_id="$1"
    local text_json="$2"
    local label="${3:-terminal-send}"
    local request_id
    request_id=$(next_request_id "$label")
    send_control_json "$request_id" "{
  \"request_id\": \"$request_id\",
  \"command\": \"terminal.send_text\",
  \"pane_slot_id\": \"$pane_slot_id\",
  \"text\": \"$text_json\"
}"
}

control_tab_open_cwd() {
    local cwd="$1"
    local request_id
    request_id=$(next_request_id tab-open-cwd)
    send_control_json "$request_id" "{
  \"request_id\": \"$request_id\",
  \"command\": \"tab.open\",
  \"title\": \"Restart Restore\",
  \"cwd\": \"$(json_escape_fragment "$cwd")\"
}"
}

send_terminal_payload_until_applied() {
    local pane_slot_id="$1"
    local text_json="$2"
    local label="$3"
    local result_path=""
    local deadline=$((SECONDS + TIMEOUT_SECONDS))
    while (( SECONDS < deadline )); do
        result_path=$(control_terminal_send_text_payload "$pane_slot_id" "$text_json" "$label")
        if [[ "$(json_get "$result_path" applied)" == "true" ]]; then
            printf '%s\n' "$result_path"
            return 0
        fi
        sleep 0.5
    done
    printf '%s\n' "$result_path"
    return 1
}

workspace_manifest_path() {
    printf '%s\n' "$SMOKE_APP_SUPPORT_DIR/alan-macos-dev/shell-workspace-window_main.json"
}

wait_for_file_text() {
    local path="$1"
    local needle="$2"
    local label="$3"
    local deadline=$((SECONDS + TIMEOUT_SECONDS))
    while (( SECONDS < deadline )); do
        if [[ -f "$path" ]] && grep -F "$needle" "$path" >/dev/null 2>&1; then
            return 0
        fi
        sleep 0.2
    done
    fail "timed out waiting for $label"
}

wait_for_window_capture() {
    local deadline=$((SECONDS + TIMEOUT_SECONDS))
    while (( SECONDS < deadline )); do
        if capture_window_list; then
            append_manifest "window_detected=true"
            return 0
        fi
        if ! kill -0 "$APP_PID" >/dev/null 2>&1; then
            fail "alan smoke app exited before a window appeared"
        fi
        sleep 0.5
    done
    fail "alan window did not appear or Screen Recording permission is unavailable"
}

wait_for_capture_pid() {
    local capture_pid="$1"
    local deadline=$((SECONDS + CAPTURE_TIMEOUT_SECONDS))
    while kill -0 "$capture_pid" >/dev/null 2>&1; do
        if (( SECONDS >= deadline )); then
            kill "$capture_pid" >/dev/null 2>&1 || true
            wait "$capture_pid" >/dev/null 2>&1 || true
            return 124
        fi
        sleep 0.2
    done

    wait "$capture_pid"
}

capture_window_list() {
    "$CAPTURE" --pid "$APP_PID" --list >"$OUTPUT_DIR/window-list.txt" 2>/dev/null &
    wait_for_capture_pid "$!"
}

capture_window_image() {
    local output_path="$1"
    "$CAPTURE" --pid "$APP_PID" --output "$output_path" &
    wait_for_capture_pid "$!"
}

capture_step() {
    local name="$1"
    local output
    output=$(printf '%02d-%s.png' "$CAPTURE_INDEX" "$name")
    CAPTURE_INDEX=$((CAPTURE_INDEX + 1))
    capture_window_image "$OUTPUT_DIR/$output" || fail "failed to capture $name"
    append_manifest "screenshot=$output"
}

ui_scripting_is_available() {
    command -v osascript >/dev/null 2>&1 || return 1
    osascript <<'OSA' >/dev/null 2>&1
tell application "System Events"
    count of processes
end tell
OSA
}

run_osascript() {
    osascript - "$APP_PID" "$TIMEOUT_SECONDS" "$@" <<'OSA'
on waitForWindow(targetPID, timeoutSeconds)
    tell application "System Events"
        set deadline to (current date) + timeoutSeconds
        repeat
            set matches to every process whose unix id is targetPID
            if (count of matches) > 0 then
                set targetProcess to item 1 of matches
                set frontmost of targetProcess to true
                if exists window 1 of targetProcess then return targetProcess
            end if
            if (current date) > deadline then error "alan window did not appear"
            delay 0.25
        end repeat
    end tell
end waitForWindow

on run argv
    set targetPID to (item 1 of argv) as integer
    set timeoutSeconds to (item 2 of argv) as integer
    set actionName to item 3 of argv
    set targetProcess to waitForWindow(targetPID, timeoutSeconds)

    tell application "System Events"
        set frontmost of targetProcess to true
        if actionName is "command-ui" then
            keystroke "p" using command down
        else if actionName is "previous-space" then
            key code 123 using {command down, option down}
        else if actionName is "next-space" then
            key code 124 using {command down, option down}
        else if actionName is "next-tab" then
            keystroke "]" using {command down, shift down}
        else if actionName is "find" then
            keystroke "f" using command down
        else if actionName is "return" then
            key code 36
        else if actionName is "escape" then
            key code 53
        else if actionName is "quit-confirm" then
            set clickedQuit to false
            repeat with candidateMenuBarItem in menu bar items of menu bar 1 of targetProcess
                repeat with candidateMenuItem in menu items of menu 1 of candidateMenuBarItem
                    if (name of candidateMenuItem starts with "Quit") then
                        click candidateMenuItem
                        set clickedQuit to true
                        exit repeat
                    end if
                end repeat
                if clickedQuit then exit repeat
            end repeat
            if not clickedQuit then error "quit menu item not found"
            set closeDeadline to (current date) + timeoutSeconds
            repeat
                set matches to every process whose unix id is targetPID
                if (count of matches) = 0 then error "alan exited before close confirmation"
                set targetProcess to item 1 of matches
                repeat with candidateWindow in windows of targetProcess
                    if exists button "Close" of candidateWindow then
                        click button "Close" of candidateWindow
                        return
                    end if
                    repeat with candidateSheet in sheets of candidateWindow
                        if exists button "Close" of candidateSheet then
                            click button "Close" of candidateSheet
                            return
                        end if
                    end repeat
                end repeat
                if (current date) > closeDeadline then error "close confirmation did not appear"
                delay 0.25
            end repeat
        else
            error "unknown action: " & actionName
        end if
    end tell
end run
OSA
}

ui_scripting_skip_recorded=0

record_ui_scripting_skip() {
    local reason="$1"
    if [[ "$ui_scripting_skip_recorded" != "1" ]]; then
        append_manifest "skipped_ui_scripting_steps=$reason"
        ui_scripting_skip_recorded=1
    fi
}

run_ui_step() {
    local action="$1"
    local log_path="$OUTPUT_DIR/ui-scripting-$action.log"
    if run_osascript "$@" >"$log_path" 2>&1; then
        return 0
    fi

    if [[ "$UI_SCRIPTING_STEPS" == "always" ]]; then
        cat "$log_path" >&2
        fail "UI scripting step failed: $action"
    fi

    ui_scripting_enabled=0
    record_ui_scripting_skip "Accessibility permission was unavailable or denied"
    return 1
}

run_restart_restore_step() {
    local before_token="$RESTART_RESTORE_BEFORE_TOKEN-$$"
    local after_token="$RESTART_RESTORE_AFTER_TOKEN-$$"
    local pwd_file="$RESTART_RESTORE_CWD/$RESTART_RESTORE_PWD_FILE"
    local manifest_path
    local tab_result
    local pane_id
    local cwd_json
    local before_json
    local after_json
    local terminal_result
    local state_result
    local restored_pane_id
    local cwd_result

    manifest_path=$(workspace_manifest_path)
    tab_result=$(control_tab_open_cwd "$RESTART_RESTORE_CWD")
    require_control_applied "$tab_result" "restart restore tab.open"
    pane_id=$(json_get "$tab_result" pane_id)
    [[ -n "$pane_id" ]] || fail "restart restore tab.open did not include a pane target"

    cwd_json=$(json_escape_fragment "$RESTART_RESTORE_CWD")
    before_json=$(json_escape_fragment "$before_token")
    printf -v terminal_payload 'cd \\"%s\\"; echo %s; sleep 30\\n' \
        "$cwd_json" "$before_json"
    if ! terminal_result=$(send_terminal_payload_until_applied \
        "$pane_id" \
        "$terminal_payload" \
        "restart-before")
    then
        require_control_applied "$terminal_result" "restart restore terminal.send_text before quit"
    fi
    require_control_applied "$terminal_result" "restart restore terminal.send_text before quit"

    sleep 1
    capture_step restart-before-quit

    run_ui_step quit-confirm \
        || fail "restart restore quit confirmation failed"
    wait_for_app_exit "restart restore confirmed quit"

    wait_for_file "$manifest_path" "workspace manifest after restart restore quit"
    wait_for_file_text \
        "$manifest_path" \
        "$before_token" \
        "persisted restart transcript token"
    append_manifest "restart_restore_manifest=$manifest_path"
    append_manifest "restart_restore_prior_output=$before_token"

    info "relaunching alan smoke app for restart restore"
    if [[ "$LAUNCH_MODE" == "direct" ]]; then
        launch_smoke_app_direct
    else
        launch_smoke_app_open
    fi
    append_manifest "relaunch_pid=$APP_PID"
    wait_for_window_capture
    wait_for_file "$STATE_PATH" "shell control-plane state after relaunch"
    sleep 1

    state_result=$(control_state)
    require_control_applied "$state_result" "restart restore state after relaunch"
    wait_for_file_text \
        "$state_result" \
        "$before_token" \
        "restored control-state transcript token"
    capture_step restart-restore

    restored_pane_id=$(json_get "$state_result" focused_pane_id)
    [[ -n "$restored_pane_id" ]] || fail "restart restore relaunch did not expose a focused pane"

    rm -f "$pwd_file"
    after_json=$(json_escape_fragment "$after_token")
    printf -v terminal_payload 'pwd > %s; echo %s' \
        "$RESTART_RESTORE_PWD_FILE" "$after_json"
    if ! terminal_result=$(send_terminal_payload_until_applied \
        "$restored_pane_id" \
        "$terminal_payload" \
        "restart-after")
    then
        require_control_applied "$terminal_result" "restart restore terminal.send_text after relaunch"
    fi
    require_control_applied "$terminal_result" "restart restore terminal.send_text after relaunch"
    run_ui_step return \
        || fail "restart restore return key delivery failed"

    wait_for_file "$pwd_file" "restart restore cwd proof file"
    cwd_result=$(cat "$pwd_file")
    [[ "$cwd_result" == "$RESTART_RESTORE_CWD" ]] \
        || fail "restored terminal cwd mismatch: expected $RESTART_RESTORE_CWD, got ${cwd_result:-<empty>}"

    append_manifest "restart_restore_after_input=$after_token"
    append_manifest "restart_restore_cwd_verified=$cwd_result"
    sleep 1
    capture_step restart-after-input
}

ui_scripting_enabled=0
case "$UI_SCRIPTING_STEPS" in
    always)
        ui_scripting_is_available || fail "UI scripting steps require Accessibility permission for osascript/System Events"
        ui_scripting_enabled=1
        ;;
    auto)
        if ui_scripting_is_available; then
            ui_scripting_enabled=1
        fi
        ;;
    never)
        ui_scripting_enabled=0
        ;;
esac

terminal_steps_enabled=0
case "$RUN_TERMINAL_STEPS" in
    always)
        terminal_steps_enabled=1
        ;;
    never)
        terminal_steps_enabled=0
        ;;
    auto)
        terminal_steps_enabled=$ghostty_ready
        ;;
esac

if [[ "$REQUIRE_TERMINAL_STEPS" == "1" && "$terminal_steps_enabled" != "1" ]]; then
    fail "terminal UI smoke requested but Ghostty artifacts are not prepared; run clients/apple/scripts/setup-local-ghosttykit.sh"
fi

restart_restore_steps_enabled=0
case "$RUN_RESTART_RESTORE_STEPS" in
    always)
        restart_restore_steps_enabled=1
        ;;
    never)
        restart_restore_steps_enabled=0
        ;;
    auto)
        if [[ "$terminal_steps_enabled" == "1" && "$ui_scripting_enabled" == "1" ]]; then
            restart_restore_steps_enabled=1
        fi
        ;;
esac

if [[ "$REQUIRE_RESTART_RESTORE_STEPS" == "1" && "$restart_restore_steps_enabled" != "1" ]]; then
    fail "restart transcript restore smoke requested but terminal delivery or Accessibility is unavailable"
fi

if [[ "$SKIP_BUILD" != "1" ]]; then
    info "building alan-macos into $DERIVED_DATA"
    xcodebuild \
        -project "$REPO_ROOT/clients/apple/alan-macos.xcodeproj" \
        -scheme alan-macos \
        -configuration Debug \
        -destination 'generic/platform=macOS' \
        -derivedDataPath "$DERIVED_DATA" \
        PRODUCT_BUNDLE_IDENTIFIER="$SMOKE_BUNDLE_ID" \
        INFOPLIST_KEY_CFBundleDisplayName="$SMOKE_DISPLAY_NAME" \
        build
fi

[[ -x "$APP_EXECUTABLE" ]] || fail "app executable not found: $APP_EXECUTABLE"

info "launching controlled alan smoke app"
if [[ "$LAUNCH_MODE" == "direct" ]]; then
    launch_smoke_app_direct
else
    launch_smoke_app_open
fi
append_manifest "pid=$APP_PID"
append_manifest "app_bundle=$(basename "$APP_PATH")"
append_manifest "bundle_id=$SMOKE_BUNDLE_ID"
append_manifest "control_namespace=$CONTROL_NAMESPACE"
append_manifest "terminal_steps=$terminal_steps_enabled"
append_manifest "ui_scripting_steps=$ui_scripting_enabled"
append_manifest "restart_restore_steps=$restart_restore_steps_enabled"

wait_for_window_capture
wait_for_file "$STATE_PATH" "shell control-plane state"
sleep 1
capture_step launch

if [[ "$ui_scripting_enabled" == "1" ]]; then
    if run_ui_step command-ui; then
        sleep 1
        capture_step command-ui
        run_ui_step escape || true
    fi
else
    record_ui_scripting_skip "Accessibility permission was unavailable or disabled"
fi

space_result=$(control_space_create)
require_control_applied "$space_result" "space.create"
sleep 1
capture_step space-create

if [[ "$ui_scripting_enabled" == "1" ]]; then
    if run_ui_step previous-space; then
        sleep 0.5
        if run_ui_step next-space; then
            sleep 1
            capture_step space-switch
        fi
    fi
fi

tab_result=$(control_tab_open)
require_control_applied "$tab_result" "tab.open"
sleep 1
capture_step tab-open

if [[ "$ui_scripting_enabled" == "1" ]]; then
    if run_ui_step next-tab; then
        sleep 1
        capture_step tab-switch
    fi
fi

focused_pane_id=$(json_get "$tab_result" pane_id)
if [[ -z "$focused_pane_id" ]]; then
    state_result=$(control_state)
    require_control_applied "$state_result" "state"
    focused_pane_id=$(json_get "$state_result" focused_pane_id)
fi
[[ -n "$focused_pane_id" ]] || fail "tab.open/state response did not include a focused pane"

split_result=$(control_pane_split "$focused_pane_id")
require_control_applied "$split_result" "pane.split"
split_pane_id=$(json_get "$split_result" pane_slot_id)
[[ -n "$split_pane_id" ]] || split_pane_id=$(json_get "$split_result" pane_id)
[[ -n "$split_pane_id" ]] || fail "pane.split response did not include a pane target"
sleep 1
capture_step split-right

if [[ "$terminal_steps_enabled" == "1" ]]; then
    terminal_result=""
    terminal_deadline=$((SECONDS + TIMEOUT_SECONDS))
    while (( SECONDS < terminal_deadline )); do
        terminal_result=$(control_terminal_send_text "$split_pane_id")
        if [[ "$(json_get "$terminal_result" applied)" == "true" ]]; then
            break
        fi
        sleep 0.5
    done

    if [[ -z "$terminal_result" || "$(json_get "$terminal_result" applied)" != "true" ]]; then
        if [[ "$REQUIRE_TERMINAL_STEPS" == "1" ]]; then
            fail "terminal.send_text did not become available"
        fi
        append_manifest "skipped_terminal_steps=terminal runtime delivery was unavailable"
    else
        sleep 1
        capture_step terminal-input
    fi
else
    append_manifest "skipped_terminal_steps=Ghostty artifacts were not prepared"
fi

if [[ "$restart_restore_steps_enabled" == "1" ]]; then
    run_restart_restore_step
else
    append_manifest "skipped_restart_restore_steps=terminal delivery or Accessibility was unavailable"
fi

if [[ "$ui_scripting_enabled" == "1" ]]; then
    if run_ui_step find; then
        sleep 1
        capture_step find
        run_ui_step escape || true
    fi
elif [[ "$REQUIRE_UI_SCRIPTING_STEPS" == "1" ]]; then
    fail "UI scripting steps were required but skipped"
fi

info "wrote smoke manifest: $MANIFEST_PATH"
