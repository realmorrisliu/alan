## Why

Background terminal panes keep running across alan spaces, tabs, zoom, and
split layout changes. That preserves the right shell lifecycle, but high-output
background panes can still drive Ghostty wakeups, surface refreshes, metadata
updates, and SwiftUI invalidation often enough to make the visible macOS shell
feel sluggish.

## What Changes

- Keep background terminal processes, PTYs, scrollback, and terminal state
  running in real time.
- Add a terminal runtime priority model for `foregroundInteractive`,
  `visibleBackground`, and `hiddenBackground` surfaces.
- Align Alan's embedded Ghostty scheduling with Ghostty's focused/visible
  boundary: wakeups mean state may need processing, while painting is governed
  by visibility and priority.
- Introduce a window-level terminal render coordinator that coalesces embedded
  Ghostty tick/refresh work across terminal surfaces.
- Throttle SwiftUI-facing runtime, scrollback, and metadata publication from
  hidden background panes while retaining the latest state for immediate
  catch-up when a pane becomes visible.
- Add focused verification for background high-output panes, priority
  transitions, refresh coalescing, and foreground input responsiveness.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `macos-shell-terminal-lifecycle`: Define real-time background terminal
  execution separately from visible rendering and require hidden panes to
  reattach without process restart or stale display.
- `macos-terminal-runtime-foundation`: Add the window-level render coordinator,
  runtime priority, Ghostty tick/refresh ownership, and SwiftUI publication
  throttling contracts.
- `macos-shell-build-test-contract`: Require focused tests and stress smoke for
  background terminal wakeup coalescing, hidden-to-visible catch-up, and
  foreground responsiveness.

## Impact

- `clients/apple/alan-macos/GhosttyLiveHost.swift` will stop owning independent
  per-host main-queue tick/refresh scheduling and will register wakeups with a
  window-level coordinator.
- `clients/apple/alan-macos/TerminalRuntimeRegistry.swift` or the primary shell
  owner will own render coordination for the window.
- `clients/apple/alan-macos/TerminalRuntimeService.swift`,
  `TerminalSurfaceController.swift`, and `TerminalHostView.swift` will need
  explicit priority and visibility propagation.
- `clients/apple/alan-macos/ShellHostController.swift` and runtime publication
  paths will need throttling so hidden background updates do not repeatedly
  invalidate the shell root.
- Focused Apple client tests and shell contract scripts will need coverage for
  background high-output panes and priority transitions.
