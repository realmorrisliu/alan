#!/usr/bin/env bash
set -euo pipefail

# ============================================================
# alan live provider harness
# Runs ignored provider integration tests against real upstreams.
# ============================================================

if [[ "${ALAN_LIVE_PROVIDER_TESTS:-}" != "1" ]]; then
  echo "Refusing to run live provider harness without ALAN_LIVE_PROVIDER_TESTS=1"
  echo "Set ALAN_LIVE_PROVIDER_TESTS=1 after configuring at least one provider credential set."
  exit 1
fi

configured=()

[[ -n "${ALAN_LIVE_OPENAI_RESPONSES_API_KEY:-}" ]] && configured+=("openai_responses")
[[ -n "${ALAN_LIVE_CHATGPT_AUTH_STORAGE_PATH:-}" ]] && configured+=("chatgpt")
[[ -n "${ALAN_LIVE_OPENAI_CHAT_COMPLETIONS_API_KEY:-}" ]] && configured+=("openai_chat_completions")
[[ -n "${ALAN_LIVE_OPENAI_CHAT_COMPATIBLE_API_KEY:-}" ]] && configured+=("openai_chat_completions_compatible")
[[ -n "${ALAN_LIVE_ANTHROPIC_MESSAGES_API_KEY:-}" ]] && configured+=("anthropic_messages")

if [[ "${#configured[@]}" -eq 0 ]]; then
  echo "No live providers are configured."
  echo "Expected one or more of:"
  echo "  ALAN_LIVE_OPENAI_RESPONSES_API_KEY"
  echo "  ALAN_LIVE_CHATGPT_AUTH_STORAGE_PATH"
  echo "  ALAN_LIVE_OPENAI_CHAT_COMPLETIONS_API_KEY"
  echo "  ALAN_LIVE_OPENAI_CHAT_COMPATIBLE_API_KEY"
  echo "  ALAN_LIVE_ANTHROPIC_MESSAGES_API_KEY"
  exit 0
fi

echo "Running live provider harness for: ${configured[*]}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
python3 "$ROOT/scripts/build_artifacts.py" --owner "${ALAN_BUILD_OWNER:-$ROOT}" \
    run --workspace "$ROOT" -- cargo test -p alan-llm --test live_provider_harness -- --ignored --nocapture
