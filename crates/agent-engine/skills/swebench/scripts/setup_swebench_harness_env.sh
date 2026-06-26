#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'USAGE'
Usage:
  crates/agent-engine/skills/swebench/scripts/setup_swebench_harness_env.sh [--venv-dir <dir>] [--python-bin <path>]

Creates a dedicated virtualenv for the official SWE-bench harness and installs
the `swebench` Python package into it.

Defaults:
  venv-dir   target/benchmarks/swebench_harness/.venv

Environment:
  ALAN_SWEBENCH_HARNESS_PYTHON_BIN
      Optional default base interpreter.

After setup, export:
  ALAN_SWEBENCH_HARNESS_PYTHON_BIN=<venv-dir>/bin/python
USAGE
}

venv_dir=""
python_bin_override=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --venv-dir)
            if [[ -z "${2:-}" ]]; then
                usage
                exit 2
            fi
            venv_dir="$2"
            shift 2
            ;;
        --python-bin)
            if [[ -z "${2:-}" ]]; then
                usage
                exit 2
            fi
            python_bin_override="$2"
            shift 2
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

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../../../.." && pwd)"

if [[ -z "$venv_dir" ]]; then
    venv_dir="$repo_root/target/benchmarks/swebench_harness/.venv"
fi

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

base_python_bin="$(pick_python_bin)"
if [[ -z "$base_python_bin" ]]; then
    echo "Missing required command: python3 or python" >&2
    exit 1
fi

mkdir -p "$(dirname "$venv_dir")"
"$base_python_bin" -m venv "$venv_dir"

harness_python="$venv_dir/bin/python"
"$harness_python" -m pip install --upgrade pip setuptools wheel
"$harness_python" -m pip install swebench socksio

echo "SWE-bench harness environment ready:"
echo "  venv_dir: $venv_dir"
echo "  harness_python: $harness_python"
echo "  export ALAN_SWEBENCH_HARNESS_PYTHON_BIN=$harness_python"

bash "$script_dir/check_swebench_harness_env.sh" --python-bin "$harness_python"
