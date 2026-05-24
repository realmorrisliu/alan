## Why

Alan Dev visual acceptance found that dragging a sidebar tab to reorder it can
also move the macOS window. The sidebar tab list and space controls are active
workspace interaction surfaces, so window background dragging must not compete
with tab reorder, space switching, or sidebar controls.

The issue is separate from the content-container model work. It belongs in a
small macOS chrome bug fix because the likely cause is the window-level
background dragging contract, not tab ordering state.

## What Changes

- Limit primary shell window dragging to explicit blank titlebar/chrome areas.
- Treat sidebar tab rows, space controls, command launcher, and sidebar titlebar
  controls as interaction surfaces that do not move the window when dragged.
- Preserve existing blank-titlebar drag and double-click visible-frame zoom
  behavior.
- Add focused placement tests for the drag-hit boundary so future sidebar
  controls do not reintroduce window-drag interference.

## Capabilities

### Modified Capabilities

- `macos-shell-ui-ux-conformance`: adds a contract that sidebar workspace
  controls own their drag interactions and are excluded from window dragging.

## Impact

- Affected Swift code: `ShellWindowPlacement.swift` and, only if needed for
  precise hit boundaries, sidebar AppKit/SwiftUI host wrappers.
- Affected tests: `clients/apple/scripts/test-shell-window-placement.swift`.
- User-visible effect: dragging tabs or space controls in the sidebar should no
  longer move the Alan Dev window, while dragging blank top chrome still moves
  the window.
