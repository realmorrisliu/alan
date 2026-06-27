#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'USAGE'
Usage:
  crates/agent-engine/skills/swebench/scripts/check_swebench_harness_env.sh [--python-bin <path>] [--json]

Checks whether the local machine is ready to run the official SWE-bench
harness. The check covers:

  1. a usable Python interpreter,
  2. the `swebench` Python module,
  3. Docker CLI availability,
  4. Docker daemon reachability.

Environment:
  ALAN_SWEBENCH_HARNESS_PYTHON_BIN
      Optional default Python interpreter for the harness.
USAGE
}

python_bin_override=""
json_output=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --python-bin)
            if [[ -z "${2:-}" ]]; then
                usage
                exit 2
            fi
            python_bin_override="$2"
            shift 2
            ;;
        --json)
            json_output=true
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            usage
            exit 2
            ;;
    esac
done

pick_python_bin() {
    if [[ -n "$python_bin_override" ]]; then
        printf '%s\n' "$python_bin_override"
    elif [[ -n "${ALAN_SWEBENCH_HARNESS_PYTHON_BIN:-}" ]]; then
        printf '%s\n' "$ALAN_SWEBENCH_HARNESS_PYTHON_BIN"
    elif command -v python3 >/dev/null 2>&1; then
        printf '%s\n' "python3"
    elif command -v python >/dev/null 2>&1; then
        printf '%s\n' "python"
    else
        printf '\n'
    fi
}

python_bin="$(pick_python_bin)"
python_available=false
python_module_available=false
docker_command_available=false
docker_daemon_reachable=false
python_executable=""
python_version=""
docker_bin=""
docker_version=""
socks_proxy_configured=false
socksio_module_available=false
socks_proxy_url=""

declare -a missing_requirements=()

if [[ -n "$python_bin" ]]; then
    if python_executable="$("$python_bin" -c 'import sys; print(sys.executable)' 2>/dev/null)"; then
        python_available=true
        python_version="$("$python_bin" -c 'import sys; print(sys.version.splitlines()[0])' 2>/dev/null || true)"
        if "$python_bin" -c 'import importlib.util, sys; sys.exit(0 if importlib.util.find_spec("swebench") else 1)' >/dev/null 2>&1; then
            python_module_available=true
        else
            missing_requirements+=("swebench_python_module")
        fi
        if "$python_bin" -c 'import importlib.util, sys; sys.exit(0 if importlib.util.find_spec("socksio") else 1)' >/dev/null 2>&1; then
            socksio_module_available=true
        fi
    else
        missing_requirements+=("python_interpreter_unusable")
    fi
else
    missing_requirements+=("python_interpreter_missing")
fi

for proxy_var_name in ALL_PROXY all_proxy HTTPS_PROXY https_proxy HTTP_PROXY http_proxy; do
    proxy_value="${!proxy_var_name:-}"
    if [[ -n "$proxy_value" && "$proxy_value" =~ ^[sS][oO][cC][kK][sS] ]]; then
        socks_proxy_configured=true
        socks_proxy_url="$proxy_value"
        break
    fi
done

if [[ "$socks_proxy_configured" == true && "$socksio_module_available" != true ]]; then
    missing_requirements+=("socksio_python_module_for_proxy")
fi

if command -v docker >/dev/null 2>&1; then
    docker_command_available=true
    docker_bin="$(command -v docker)"
    docker_version="$(docker --version 2>/dev/null || true)"
    if docker info >/dev/null 2>&1; then
        docker_daemon_reachable=true
    else
        missing_requirements+=("docker_daemon_unreachable")
    fi
else
    missing_requirements+=("docker_cli_missing")
fi

ready=false
if [[ "$python_available" == true \
    && "$python_module_available" == true \
    && "$docker_command_available" == true \
    && "$docker_daemon_reachable" == true \
    && ${#missing_requirements[@]} -eq 0 ]]; then
    ready=true
fi

if [[ "$json_output" == true ]]; then
    if ! command -v jq >/dev/null 2>&1; then
        echo "Missing required command for --json output: jq" >&2
        exit 1
    fi
    missing_requirements_json='[]'
    if (( ${#missing_requirements[@]} > 0 )); then
        missing_requirements_json="$(printf '%s\n' "${missing_requirements[@]}" | jq -R . | jq -s .)"
    fi
    jq -n \
        --arg requested_python_bin "$python_bin_override" \
        --arg selected_python_bin "$python_bin" \
        --arg python_executable "$python_executable" \
        --arg python_version "$python_version" \
        --arg docker_bin "$docker_bin" \
        --arg docker_version "$docker_version" \
        --arg socks_proxy_url "$socks_proxy_url" \
        --argjson python_available "$python_available" \
        --argjson swebench_module_available "$python_module_available" \
        --argjson socks_proxy_configured "$socks_proxy_configured" \
        --argjson socksio_module_available "$socksio_module_available" \
        --argjson docker_command_available "$docker_command_available" \
        --argjson docker_daemon_reachable "$docker_daemon_reachable" \
        --argjson ready "$ready" \
        --argjson missing_requirements "$missing_requirements_json" \
        '{
            ready: $ready,
            requested_python_bin: (if $requested_python_bin == "" then null else $requested_python_bin end),
            selected_python_bin: (if $selected_python_bin == "" then null else $selected_python_bin end),
            python_executable: (if $python_executable == "" then null else $python_executable end),
            python_version: (if $python_version == "" then null else $python_version end),
            swebench_module_available: $swebench_module_available,
            socks_proxy_configured: $socks_proxy_configured,
            socks_proxy_url: (if $socks_proxy_url == "" then null else $socks_proxy_url end),
            socksio_module_available: $socksio_module_available,
            docker_command_available: $docker_command_available,
            docker_daemon_reachable: $docker_daemon_reachable,
            docker_bin: (if $docker_bin == "" then null else $docker_bin end),
            docker_version: (if $docker_version == "" then null else $docker_version end),
            missing_requirements: $missing_requirements
        }'
else
    echo "SWE-bench harness preflight:"
    echo "  ready: $ready"
    echo "  selected_python_bin: ${python_bin:-<none>}"
    echo "  python_executable: ${python_executable:-<none>}"
    echo "  python_version: ${python_version:-<none>}"
    echo "  swebench_module_available: $python_module_available"
    echo "  socks_proxy_configured: $socks_proxy_configured"
    echo "  socksio_module_available: $socksio_module_available"
    echo "  docker_command_available: $docker_command_available"
    echo "  docker_daemon_reachable: $docker_daemon_reachable"
    if [[ -n "$socks_proxy_url" ]]; then
        echo "  socks_proxy_url: $socks_proxy_url"
    fi
    if [[ -n "$docker_version" ]]; then
        echo "  docker_version: $docker_version"
    fi
    if (( ${#missing_requirements[@]} > 0 )); then
        echo "  missing_requirements:"
        for requirement in "${missing_requirements[@]}"; do
            echo "    - $requirement"
        done
    fi
fi

if [[ "$ready" != true ]]; then
    exit 1
fi
