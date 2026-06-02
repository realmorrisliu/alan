## Why

Alan's macOS Settings tab now has enough content that a single long scroll
mixes everyday preferences, terminal setup, provider accounts, session defaults,
skills, and local diagnostics into one surface. A left settings navigation gives
the existing Settings tab a clearer hierarchy without changing its shell-hosted
contract. The first navigation pass improved the layout but still mirrored
implementation domains too closely. Settings should group rows by user task and
configuration scope instead.

## What Changes

- Add an internal Settings navigation column with four task-oriented groups:
  General, Terminal, Agent, and System.
- Render only the selected group in the main Settings content area instead of
  showing every settings section in one continuous scroll.
- Keep Terminal Profiles and Managed Terminal Accounts under Terminal as a
  standalone terminal-app capability that remains useful without an agent.
- Fold provider connection state, session runtime defaults, skills, skill
  package source, and the command line tool into Agent.
- Keep app install state, daemon endpoint, updates, storage paths, shell state,
  shell control, and diagnostics under System.
- Add an Alan-only agent selector affordance in Agent so the IA is ready for
  future agent-specific settings without showing unsupported Codex settings.
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
- Focused Apple client tests should cover navigation order, per-row group and
  section membership, Terminal independence from Agent, Agent ownership of
  connection/runtime/skill rows, System ownership of host/runtime rows,
  redaction, unavailable rows, and the existing Settings singleton behavior.
