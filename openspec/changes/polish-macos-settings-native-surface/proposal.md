## Why

Alan's macOS Settings surface now has the right high-level information
architecture, but the visual execution still reads as a web app: a pale sidebar,
a large white canvas, and dashboard-like row cards. This change makes Settings
feel like a native macOS preference surface that belongs inside Alan's calm,
terminal-first shell.

## What Changes

- Polish the shell-hosted Settings layout into a native-feeling source-list plus
  inset grouped form instead of a web-style sidebar and card page.
- Preserve the accepted Settings navigation hierarchy from
  `add-macos-settings-navigation`: General, Terminal, Agent, and System.
- Tighten layout density, content width, row rhythm, typography, icon sizing,
  dividers, and trailing control alignment so Settings feels precise rather than
  sparse.
- Replace heavy card affordances with macOS-style grouped rows, restrained
  separators, subtle material depth, and purpose-built row descriptions.
- Reduce unnecessary accent-color dominance while keeping native controls and
  existing preference behavior.
- Verify the polish in Alan Dev with a fresh relaunch and screenshot review
  rather than relying only on unit tests.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `macos-shell-ui-ux-conformance`: Settings shall use a native macOS visual
  hierarchy with source-list navigation, grouped settings rows, disciplined
  typography, subtle surface depth, and screenshot-verifiable polish.

## Impact

- `clients/apple/alan-macos/TerminalPaneView.swift` settings layout, section,
  row, navigation, and background presentation will need focused visual updates.
- `clients/apple/alan-macos/Models/Shell/ShellSettingsSurfaceModel.swift` may
  need minor presentation metadata for row descriptions or section labels, but
  row ownership and settings semantics should remain stable.
- Focused settings scripts and Swift tests should keep covering navigation,
  row membership, preference bindings, singleton Settings behavior, redaction,
  and unavailable states.
- Manual UI verification must use a fresh Alan Dev launch in light mode and
  compare Settings against this change's visual acceptance criteria.
