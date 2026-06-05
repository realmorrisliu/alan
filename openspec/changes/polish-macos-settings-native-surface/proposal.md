## Why

Alan's macOS Settings surface now has the right high-level information
architecture, but the visual execution drifted toward an Apple System Settings
clone: a large white sheet, low information density, and weak preference-list
rhythm. This change moves Settings toward a Linear/Raycast-style control panel
that belongs inside Alan's calm, terminal-first shell.

## What Changes

- Polish the shell-hosted Settings layout into a native-feeling source-list plus
  dense preference list instead of an Apple Settings sheet or web-style card
  page.
- Preserve the accepted Settings navigation hierarchy from
  `add-macos-settings-navigation`: General, Terminal, Agent, and System.
- Tighten layout density, content width, row rhythm, typography, icon sizing,
  section dividers, and a shared title/detail/control row template so Settings
  feels precise rather than sparse.
- Use a native capsule selected state in the Settings source list, with darker
  selected text instead of blue accent bars.
- Keep the detail content in a left-anchored 760pt maximum-width column so wide
  windows do not stretch Settings into an empty page.
- Replace sheet/card affordances with direct section headings, horizontal
  separators, compact label/value rows, and concise row descriptions.
- Turn local System metadata rows that have obvious actions into control-panel
  affordances, for example copy daemon endpoint and open local folders, while
  avoiding fake edit controls for read-only install facts.
- Use native action copy such as Show..., Create..., and Preview... instead of
  web-style external-link arrows or blue text links.
- Reduce unnecessary accent-color dominance while keeping native controls and
  existing preference behavior.
- Verify the polish in Alan Dev with a fresh relaunch and screenshot review
  rather than relying only on unit tests.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `macos-shell-ui-ux-conformance`: Settings shall use a native macOS visual
  hierarchy with source-list navigation, dense preference-list rows,
  disciplined typography, subtle surface depth, and screenshot-verifiable
  polish.

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
