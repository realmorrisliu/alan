## ADDED Requirements

### Requirement: Registry Excludes Quick Terminal Actions
The macOS shell action registry SHALL NOT register Quick Terminal actions,
aliases, shortcuts, or shell workspace effects. Primary Window Summon SHALL be
owned by macOS app/window command routing outside the shell action registry.

#### Scenario: Action descriptors are enumerated
- **WHEN** menus, keyboard dispatch, context menus, tests, or FFI action adapters
  enumerate shell action descriptors
- **THEN** no descriptor has `shell.quick_terminal.toggle`,
  `shell.quick_terminal.show`, `shell.quick_terminal.hide`,
  `shell.quick_terminal.focus`, `shell.quick_terminal.close`, or
  `shell.quick_terminal.promote`

#### Scenario: Former shortcut is resolved
- **WHEN** the former Quick Terminal global shortcut is configured
- **THEN** the macOS app command layer resolves it to Primary Window Summon
- **AND** the shell action registry does not translate it into a shell action

#### Scenario: Compatibility alias is requested
- **WHEN** a shell action, FFI, or registry path requests a Quick Terminal action
  by its old action ID
- **THEN** Alan treats the action as unsupported rather than mapping it to
  Primary Window Summon
