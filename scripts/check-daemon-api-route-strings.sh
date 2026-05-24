#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

files=(
  "crates/alan/src/daemon/server.rs"
  "crates/alan/src/daemon/routes.rs"
  "crates/alan/src/daemon/relay.rs"
  "crates/alan/src/daemon/remote_control.rs"
  "crates/alan/src/cli/daemon.rs"
  "crates/tui/src/daemon_client.rs"
)

violations=0
for file in "${files[@]}"; do
  if [[ "$file" == *.rs ]]; then
    matches="$(awk '/^#\[cfg\(test\)\]/{exit} {print}' "$file" | rg -n '"/api/v1/|`/api/v1/|"/health"|`/health`' || true)"
  else
    matches="$(rg -n '"/api/v1/|`/api/v1/|"/health"|`/health`' "$file" || true)"
  fi
  if [[ -n "$matches" ]]; then
    echo "Raw daemon route strings found in $file"
    echo "$matches"
    violations=1
  fi
done

if [[ "$violations" -ne 0 ]]; then
  echo
  echo "Use crates/alan/src/daemon/api_contract.rs endpoint helpers instead."
  exit 1
fi

echo "Daemon API route string guardrail passed."
