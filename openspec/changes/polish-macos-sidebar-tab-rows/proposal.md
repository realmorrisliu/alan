## Why

The macOS sidebar tab list is currently taller and heavier than the Arc-like
reference Alan is targeting, especially around the New Tab row and ordinary
terminal tab rows. The sidebar also lacks a lightweight Clear affordance for
removing inactive temporary tabs without touching pinned tabs, the current tab,
or active work.

## What Changes

- Tighten the sidebar row system so New Tab and ordinary tab rows share one
  compact geometry.
- Make New Tab match Arc-style states: quiet at rest, full-row rounded material
  background on hover or keyboard focus, and no persistent background.
- Keep real tab rows compact while supporting either a vertically centered
  single line or a meaningful two-line presentation within the same visual
  system.
- Shift tab identity toward task-first titles: terminal-provided or
  agent-provided titles should make tabs distinguishable by what they are doing,
  while user-edited titles remain locked and are not overwritten.
- Use subtitles selectively: required for actionable states, recommended for
  task-title disambiguation, and hidden for fallback or duplicate metadata.
- Keep the existing leading split indicator unchanged, and use the trailing
  accessory slot for state glyphs at rest and the close button on hover.
- Remove the inline pin glyph from tab rows because pinned position and section
  grouping already convey pinned state visually.
- Add a Clear affordance in the active Space tab list that closes eligible
  temporary tabs in one action.
- Define eligible temporary tabs as current-Space unpinned tabs that are not the
  selected tab and whose active task state does not protect them from pruning.
- Repair sidebar tab drag/reorder so pointer drag from a tab row reliably
  carries the dragged tab identity into the drop target and applies the reorder.
- Preserve existing tab creation, selection, pin/unpin, close, and drag/reorder
  data model semantics.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `macos-shell-ui-ux-conformance`: Refine the default sidebar tab row visual and
  interaction contract for New Tab, compact tab rows, and Clear inactive tabs.

## Impact

- Affected code:
  - `clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift`
  - `clients/apple/alan-macos/Support/ShellDesignTokens.swift`, if shared row
    metrics belong in tokens instead of the view file
  - `clients/apple/alan-macos/Models/Shell/ShellStateMutations.swift`
  - `clients/apple/alan-macos/ShellHostController.swift`
  - Relevant shell action or sidebar tests under `clients/apple/scripts/`
- No network protocol, daemon API, provider, runtime, or dependency changes.
- No breaking changes.
