## Why

Quick Terminal adds a detached terminal runtime, Peak window, persistence shape,
and command surface that now conflicts with the direction of making Alan's
primary shell window the authoritative macOS workspace. The replacement should
remove the standalone quick-terminal product path and reuse the existing global
shortcut to summon the primary shell window into the user's current macOS
context.

## What Changes

- **BREAKING**: Remove Quick Terminal as a standalone Alan feature, including
  detached Peak presentation, global quick-terminal runtime slot, promotion,
  quick-terminal command aliases, and quick-terminal restore behavior.
- Reassign the former Quick Terminal global shortcut to a macOS-only Primary
  Window Summon command that creates or reopens the single primary shell window
  when needed, moves or summons it to the user's current Space/display on a
  best-effort basis, activates the app, and focuses the current selected content.
- Preserve shell workspace selection during summon: current shell Space, Tab,
  PaneSlot, split tree, and content runtime identities remain unchanged.
- Treat legacy quick-terminal manifest data as discarded legacy state: old
  records may decode during load, but they are not migrated into tabs, not
  shown, not preserved as a hidden session, and omitted from future writes.
- Keep the behavior owned by the macOS app/window lifecycle and native command
  layer, not by Rust shell core, shell workspace actions, or cross-platform
  shell state.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `macos-app-instance-lifecycle`: Adds the Primary Window Summon contract for
  the single primary shell window.
- `macos-quick-terminal-peak`: Removes the standalone Quick Terminal Peak
  capability contract.
- `macos-shell-action-registry`: Excludes Quick Terminal actions and aliases
  from the shell action registry because summon is an app/window command.
- `macos-shell-workspace-interactions`: Removes Quick Terminal summon/dismiss as
  shell workspace commands.
- `macos-shell-workspace-persistence`: Discards legacy quick-terminal restore
  data and omits it from future manifest writes.
- `macos-shell-ui-ux-conformance`: Removes detached Peak UI requirements and
  adds primary-window summon UX expectations.
- `macos-shell-terminal-lifecycle`: Removes Quick Terminal-specific close guard
  semantics; only normal terminal content close scopes remain.
- `macos-shell-build-test-contract`: Replaces Quick Terminal boundary
  verification with primary-window summon and legacy-cleanup verification.

## Impact

- Apple client app/window owner, command, keybinding, menu, and focus routing.
- Shell host/model code that currently owns quick-terminal slots, runtime
  identity, Peak presentation, promotion, and quick-terminal control commands.
- Workspace manifest decoding/materialization/writing and tests that currently
  include `quick_terminal` restore data.
- OpenSpec and developer docs that describe Quick Terminal, Peak presentation,
  or quick-terminal action IDs as supported product behavior.
