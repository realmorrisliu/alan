#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

SURFACE_FILES=(
  "$REPO_ROOT/clients/apple/alan-macos/App/AlanMacShellCommands.swift"
  "$REPO_ROOT/clients/apple/alan-macos/Views/Shell/ShellWorkspaceView.swift"
  "$REPO_ROOT/clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift"
  "$REPO_ROOT/clients/apple/alan-macos/Views/Shell/ShellSidebarTabDrop.swift"
  "$REPO_ROOT/clients/apple/alan-macos/Views/Shell/ShellSidebarSpaceSlider.swift"
  "$REPO_ROOT/clients/apple/alan-macos/Views/Shell/ShellSidebarTabRow.swift"
  "$REPO_ROOT/clients/apple/alan-macos/Views/Shell/ShellSidebarActivityProgressRail.swift"
  "$REPO_ROOT/clients/apple/alan-macos/Views/Shell/ShellPaneTopologyIndicator.swift"
)

if rg -n \
  "performShellWorkspaceCommand|host\\.openTerminalTab|host\\.openAlanTab|host\\.pinTab|host\\.unpinTab|host\\.updatePinnedTabSnapshot|host\\.selectAdjacentSpace|host\\.selectSpace\\(at:" \
  "${SURFACE_FILES[@]}"; then
  echo "Shared shell menu/context/keyboard surfaces must route through ShellActionRegistry." >&2
  exit 1
fi

if ! rg -q "shellActionKeyboardShortcut\\(host\\.shellActionShortcut\\(\\.newTerminalTab\\)\\)" \
  "$REPO_ROOT/clients/apple/alan-macos/App/AlanMacShellCommands.swift"; then
  echo "Native shell menu shortcut hints must come from ShellActionRegistry descriptors." >&2
  exit 1
fi

if ! rg -q "shellActionShortcut\\(\\.spaceSelectByIndex, target: target\\)" \
  "$REPO_ROOT/clients/apple/alan-macos/Views/Shell/ShellWorkspaceView.swift"; then
  echo "Numeric Space keyboard shortcuts must use registry-derived shortcut descriptors." >&2
  exit 1
fi

if rg -n "ShellCommandTabView|Ask alan\\.\\.\\.|Go to or Command|newAlanTab|commandInputOpen" \
  "$REPO_ROOT/clients/apple/alan-macos" \
  "$REPO_ROOT/clients/apple/alan-macos.xcodeproj/project.pbxproj"; then
  echo "Removed Ask alan and New alan Tab surfaces must stay out of the shell action registry pass." >&2
  exit 1
fi

if rg -n "Command-P|Create Alan Tab|createAlanTab|spaceOpenAlan|launchTarget: \\.alan|ShellLaunchTarget\\.alan" \
  "$REPO_ROOT/clients/apple/alan-macos" \
  "$REPO_ROOT/clients/apple/alan-macos.xcodeproj/project.pbxproj"; then
  echo "Removed command input and first-party alan tab automation must stay absent." >&2
  exit 1
fi

echo "Shell action registry integration checks passed."
