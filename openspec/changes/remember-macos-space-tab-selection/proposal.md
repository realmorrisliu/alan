## Why

Switching Spaces in the macOS shell currently focuses the first Tab in the
target Space instead of the Tab the user last used there. This makes Space
switching feel lossy and conflicts with the sidebar model where each Space is a
durable work context.

## What Changes

- Remember the last selected Tab and preferred focused PaneSlot for each Space
  while the shell window is running.
- Restore a Space's remembered Tab when the user switches Spaces through the
  sidebar, keyboard shortcuts, swipe commit, menu actions, command routing, or
  control-plane selection.
- Persist per-Space selected Tab state in the workspace manifest so app restart
  restores both the last selected Space and each Space's own Tab selection.
- Repair remembered Tab selection when Tabs are closed, moved between Spaces,
  retired by lifecycle pruning, or when old manifests do not include per-Space
  selection metadata.
- Preserve existing behavior for empty Spaces: selecting an empty Space shows an
  empty workspace with no fabricated Tab or PaneSlot focus.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `macos-shell-workspace-interactions`: Space selection must restore the target
  Space's remembered Tab and PaneSlot instead of defaulting to the first Tab.
- `macos-shell-workspace-persistence`: Workspace manifests must store and repair
  per-Space selected Tab state while remaining compatible with manifests that
  only have global selected Space/Tab fields.

## Impact

- `clients/apple/alan-macos/ShellHostController.swift` needs Space selection to
  resolve targets from per-Space remembered selection before falling back to the
  first available Tab.
- `clients/apple/alan-macos/Models/Shell/ShellWorkspaceManifest.swift`,
  materialization, and manifest writeback need optional per-Space selected Tab
  fields plus migration/repair behavior.
- `clients/apple/alan-macos/Models/Shell/ShellStateMutations.swift` and related
  shell state projection code need to preserve or repair per-Space selection
  through close, move, reorder, pin/unpin, and lifecycle pruning paths.
- Focused Apple script tests should cover switching Spaces, restart restore,
  empty Spaces, closing selected Tabs, and moving Tabs between Spaces.
