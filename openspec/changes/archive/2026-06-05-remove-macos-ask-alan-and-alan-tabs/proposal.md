## Why

The macOS shell should stay terminal-first. The existing Ask alan floating
command input and first-party alan tab creation path add a second product mode
that is not needed: users can still run `alan`, `alan chat`, or `alan ask` from
normal terminal panes when they want agent behavior.

## What Changes

- **BREAKING**: Remove the macOS `Ask alan...` entry point, `Command-P`
  floating command input, and the command input view/state model.
- **BREAKING**: Remove `New alan tab` from native menus, sidebar actions,
  keyboard shortcuts, command vocabulary, action registry, App Intents, and
  automation helpers.
- **BREAKING**: Remove the macOS shell's first-party `.alan` tab launch mode
  and automatic `alan chat`/alan-tab runtime launch branch.
- Keep CLI surfaces unchanged: `alan ask`, `alan chat`, daemon sessions, and
  terminal-launched agent workflows remain available outside this macOS tab UI.
- Keep terminal activity metadata for agent processes that users launch inside
  normal terminal tabs.
- Treat old `.alan` tab state as out of scope; this change removes the feature
  as if it was never part of the macOS product surface.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `macos-shell-ui-ux-conformance`: Remove default Ask alan/floating command
  input requirements and replace them with terminal-first constraints that
  forbid those surfaces.
- `macos-shell-workspace-interactions`: Remove command input and New alan Tab
  from native command surface requirements.
- `macos-shell-action-registry`: Remove alan tab and command input actions from
  the supported registry contract.
- `macos-shell-keybinding-system`: Remove the Command-P Ask alan shortcut and
  require default keybindings to stay terminal/workspace focused.
- `macos-shell-automation-surfaces`: Remove Create Alan Tab App Intent and
  automation helper requirements from the macOS shell automation surface.
- `macos-shell-build-test-contract`: Replace Ask alan/New alan Tab verification
  with deletion guards and CLI-preservation checks.

## Impact

- `clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift` no longer shows
  the Ask alan command launcher or New alan Tab space action.
- `clients/apple/alan-macos/App/AlanMacShellCommands.swift` no longer exposes
  Ask alan, Command-P, or New alan Tab.
- `clients/apple/alan-macos/MacShellRootView.swift`,
  `ShellHostController.swift`, and `Views/Shell/ShellCommandTabView.swift` lose
  the floating command input state, toggle, and view.
- Shell action/command models remove `newAlanTab` and command-input actions.
- `ShellAutomationIntents.swift`, control/local command seams, and App Intent
  metadata remove Create Alan Tab.
- `TerminalHostRuntime.swift` and terminal launch resolution remove the macOS
  first-party alan tab launch branch while preserving normal shell runtime
  behavior.
- Shell contract scripts and focused Apple tests need updates so removed
  surfaces cannot be reintroduced accidentally.
