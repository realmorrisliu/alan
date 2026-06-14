#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
shell_dir="$repo_root/clients/apple/alan-macos"
baseline_file="$repo_root/scripts/shell-design-token-baseline.txt"

# Raw design literals new shell UI code must not add:
#   - hard-coded font sizes:        system(size:
#   - hard-coded RGB colors:        Color(red: / NSColor(red:
#   - hard-coded numeric paddings:  .padding(... <digit> ...)
pattern='system\(size:|Color\(red:|NSColor\(red:|\.padding\([^)]*[0-9]'

is_allowed_file() {
  case "$1" in
    */Support/ShellDesignTokens.swift) return 0 ;;
    */Support/ConsoleAdaptiveColor.swift) return 0 ;;
    *) return 1 ;;
  esac
}

mode="check"
if [[ "${1:-}" == "--update-baseline" ]]; then
  mode="update"
fi

current="$(mktemp)"
trap 'rm -f "$current"' EXIT

while IFS= read -r file; do
  if is_allowed_file "$file"; then
    continue
  fi
  count="$(grep -cE "$pattern" "$file" || true)"
  if [[ "$count" -gt 0 ]]; then
    rel="${file#"$repo_root"/}"
    printf '%s:%s\n' "$rel" "$count" >>"$current"
  fi
done < <(find "$shell_dir" -name '*.swift' | sort)

if [[ "$mode" == "update" ]]; then
  cp "$current" "$baseline_file"
  echo "Baseline updated: $baseline_file"
  exit 0
fi

violations=()
while IFS=: read -r rel count; do
  allowed=0
  if [[ -f "$baseline_file" ]]; then
    baseline_entry="$(grep -F "$rel:" "$baseline_file" | tail -1 || true)"
    if [[ -n "$baseline_entry" ]]; then
      allowed="${baseline_entry##*:}"
    fi
  fi
  if (( count > allowed )); then
    violations+=("$rel: $count raw design literals (baseline $allowed)")
  fi
done <"$current"

if ((${#violations[@]})); then
  printf 'Raw design literals exceed the recorded baseline:\n' >&2
  printf '%s\n' "${violations[@]}" >&2
  printf '\nUse ShellType / ShellSpacing / ShellPaper / ShellInk / ShellSignal tokens\n' >&2
  printf '(clients/apple/alan-macos/Support/ShellDesignTokens.swift), or run\n' >&2
  printf 'scripts/check-shell-design-tokens.sh --update-baseline after a reviewed migration.\n' >&2
  exit 1
fi

echo "Shell design token guard passed"
