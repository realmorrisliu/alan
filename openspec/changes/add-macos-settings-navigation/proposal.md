## Why

Alan's macOS Settings tab now has enough content that a single long scroll
mixes everyday preferences, terminal setup, provider accounts, session defaults,
skills, and local diagnostics into one surface. A left settings navigation gives
the existing Settings tab a clearer hierarchy without changing its shell-hosted
contract.

## What Changes

- Add an internal Settings navigation column with task-oriented groups:
  General, Terminal, Accounts, Sessions, Capabilities, and Advanced.
- Render only the selected group in the main Settings content area instead of
  showing every settings section in one continuous scroll.
- Fold Terminal Profiles and Terminal Accounts into the Terminal group while
  keeping provider connection state in Accounts.
- Move local install, daemon, path, shell-control, update, and diagnostics rows
  into Advanced so the default view stays focused on everyday preferences.
- Preserve the current row controls, redaction rules, compact unavailable
  states, and shell-tab Settings behavior.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `macos-shell-ui-ux-conformance`: Settings shall use an internal left
  navigation and selected-group content area for clearer task-oriented
  hierarchy while preserving shell-native density.

## Impact

- `clients/apple/alan-macos/TerminalPaneView.swift` will need a two-column
  Settings layout and selected group state.
- `clients/apple/alan-macos/Models/Shell/ShellSettingsSurfaceModel.swift` will
  need a grouping/navigation model derived from the existing settings sections.
- Focused Apple client tests should cover navigation order, group mapping,
  Terminal/Profile separation from provider Accounts, redaction, unavailable
  rows, and the existing Settings singleton behavior.
