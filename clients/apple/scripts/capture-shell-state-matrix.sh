#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
out_dir="${1:-$REPO_ROOT/debug/screenshots/state-matrix-$(date +%Y%m%d-%H%M%S)}"
mkdir -p "$out_dir"

states=(
  empty-space
  single-tab
  split-panes
  multi-space
  dark-mode
  reduced-transparency
)

echo "Capturing the Alan Dev shell state matrix into: $out_dir"
echo "Target app: Alan Dev.app (dev channel). Do not use the stable app."

for state in "${states[@]}"; do
  echo ""
  echo "==> Arrange the Alan Dev window for state: $state"
  read -r -p "    Press Enter to capture..."
  "$SCRIPT_DIR/capture-alan-window.sh" --channel dev \
    --output "$out_dir/$state.png"
  echo "    Captured $out_dir/$state.png"
done

echo ""
echo "State matrix complete: $out_dir"
