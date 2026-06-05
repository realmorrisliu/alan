## Why

The macOS sidebar currently separates Space identity, Space switching, Space
creation, and tab creation across the top header, bottom dock, and tab list.
This adds visual weight and makes related navigation controls feel farther apart
than they should in the Arc-like, terminal-first shell.

## What Changes

- Replace the current Space header and bottom Space dock with a compact top
  Space slider.
- Show the selected Space as text and non-selected Spaces as dots, with the
  current Space count capped at 8.
- Move New Space from the bottom dock into the right-aligned action slot of the
  sidebar titlebar, with pin/unpin and appearance controls kept leading.
- Remove the always-visible terminal profile selector from the Space header and
  expose profile selection from the Space context menu.
- Reorder the tab list so pinned tabs appear first, then a divider when pinned
  tabs exist, then the New Tab row, then unpinned tabs.
- Keep New Tab visually consistent with ordinary sidebar rows while creating a
  normal unpinned terminal tab in the current Space.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `macos-shell-ui-ux-conformance`: Updates the default sidebar layout contract
  for Space navigation, Space creation, profile disclosure, and New Tab
  placement.
- `macos-shell-build-test-contract`: Adds focused verification expectations for
  the new sidebar layout ordering, interaction surfaces, and visual review.

## Impact

- Affected code is expected in the Apple client sidebar and shell root views,
  especially `ShellSidebarView.swift`, `MacShellRootView.swift`,
  `ShellDesignTokens.swift`, and focused sidebar/window placement scripts.
- No daemon, runtime, protocol, or provider API changes are expected.
- No dependency changes are expected.
