## ADDED Requirements

### Requirement: macOS shell actions consume shared shell core action contract
The macOS shell action registry SHALL consume shared action IDs, target
resolution, availability, shortcut metadata, and action-to-effect mapping from
the Rust shell core after the action module has Rust contract tests and adapter
tests.

Swift SHALL continue to own menu bar, context menu, keyboard, drag/drop, and
visual presentation of actions.

#### Scenario: Menu invokes a shared action
- **WHEN** a macOS menu item invokes a reusable shell action after shell core
  action integration
- **THEN** action availability and effect mapping come from the Rust shell core
- **AND** Swift performs only platform presentation and dispatch through the
  adapter

#### Scenario: Context target is resolved
- **WHEN** a macOS context menu supplies a clicked Tab, Space, or PaneSlot target
- **THEN** the Rust shell core resolves the target according to shared action
  rules
- **AND** Swift does not maintain a separate target-resolution implementation
  for the same reusable action

### Requirement: macOS-only presentation actions remain platform-owned
macOS-only presentation actions SHALL remain platform-owned.

Actions that are purely macOS presentation, windowing, update UI, file picker,
or AppKit behavior MUST NOT be forced into the shared shell core action
registry.

#### Scenario: Platform-only command is invoked
- **WHEN** a command only opens a macOS panel, presents a Sparkle update UI, or
  performs AppKit window behavior
- **THEN** Swift may own that command outside the shell core action registry
- **AND** the command does not duplicate reusable workspace mutation semantics
