#!/usr/bin/env bash
set -euo pipefail

repo_root="${1:-.}"
violations=()

is_allowed_file() {
  case "$1" in
    "$repo_root/crates/agent-engine/src/agent_root.rs") return 0 ;;
    "$repo_root/crates/agent-engine/src/paths.rs") return 0 ;;
    "$repo_root"/crates/*/tests/*.rs) return 0 ;;
    *_tests.rs) return 0 ;;
    *) return 1 ;;
  esac
}

is_comment_line() {
  [[ "$1" =~ ^[[:space:]]*// ]]
}

line_has_forbidden_layout_string() {
  [[ "$1" == *".alan/agents/default"* ]] && return 0
  [[ "$1" == *"agents/default"* ]] && return 0
  [[ "$1" == *'.join("agents").join("default")'* ]] && return 0
  return 1
}

while IFS= read -r file; do
  if is_allowed_file "$file"; then
    continue
  fi

  in_test_module=0
  line_number=0
  while IFS= read -r line || [[ -n "$line" ]]; do
    line_number=$((line_number + 1))
    if [[ "$line" =~ ^[[:space:]]*#\[cfg\(test\)\] ]]; then
      in_test_module=1
      continue
    fi
    if (( in_test_module )); then
      continue
    fi
    if is_comment_line "$line"; then
      continue
    fi
    if line_has_forbidden_layout_string "$line"; then
      violations+=("$file:$line_number: $line")
    fi
  done <"$file"
done < <(find "$repo_root/crates" -path '*/target/*' -prune -o -name '*.rs' -print | sort)

if ((${#violations[@]})); then
  printf 'Raw default agent-root layout strings found outside the runtime layout owner:\n' >&2
  printf '%s\n' "${violations[@]}" >&2
  printf '\nUse alan_agent_engine::AgentRootLayout or add a focused allowlist entry with justification.\n' >&2
  exit 1
fi
