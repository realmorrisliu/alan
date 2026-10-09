#!/usr/bin/env bash
set -euo pipefail

# ============================================================
# alan live runtime smoke
# Runs ignored runtime integration smoke tests against real providers.
# ============================================================

if [[ "${ALAN_LIVE_PROVIDER_TESTS:-}" != "1" ]]; then
  echo "Refusing to run live runtime smoke without ALAN_LIVE_PROVIDER_TESTS=1"
  echo "Set ALAN_LIVE_PROVIDER_TESTS=1 after configuring at least one live credential set."
  exit 1
fi

configured=()

[[ -n "${ALAN_LIVE_CHATGPT_AUTH_STORAGE_PATH:-}" ]] && configured+=("chatgpt")

if [[ "${#configured[@]}" -eq 0 ]]; then
  echo "No live runtime-smoke providers are configured."
  echo "Expected:"
  echo "  ALAN_LIVE_CHATGPT_AUTH_STORAGE_PATH"
  exit 0
fi

echo "Running live runtime smoke for: ${configured[*]}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
python3 "$ROOT/scripts/build_artifacts.py" --owner "${ALAN_BUILD_OWNER:-$ROOT}" \
    run --workspace "$ROOT" -- cargo test -p alan --test live_runtime_smoke_test -- --ignored --nocapture
