## MODIFIED Requirements

### Requirement: Commands use native Mac surfaces
Workspace actions SHALL be available through native menu/command routing,
keyboard shortcuts, context menus, and restrained toolbar or sidebar
affordances that call the same shell controller mutations where the action is
shared. Menu bar, context menu, and keyboard shortcut paths SHALL resolve shared
shell actions through the macOS shell action registry. The default macOS shell
SHALL NOT include the removed Ask alan typed command input.

#### Scenario: Menu command
- **WHEN** the user selects New Terminal Tab, Split, Focus Pane, Equalize
  Splits, Close Pane, or Close Tab from the menu bar
- **THEN** alan executes the registered shell action used by matching keyboard
  and context paths where that behavior is shared
- **AND** the menu bar does not expose New alan Tab or Ask alan commands

#### Scenario: Keyboard command
- **WHEN** the user invokes a supported command-key shortcut
- **THEN** the responder chain routes it to alan's shell action registry or
  terminal surface command handler as appropriate
- **AND** `Command-P` is not registered as an Alan-owned Ask alan command input
  shortcut

#### Scenario: Context command
- **WHEN** the user invokes a supported Tab or Space context menu command
- **THEN** alan resolves the registry action with the context Tab or Space
  target rather than first changing shell selection

#### Scenario: Removed command input stays absent
- **WHEN** workspace command surfaces are visible
- **THEN** alan does not present `Go to or Command...`, a floating typed command
  input, default action candidate lists, or unresolved typed-command status

#### Scenario: Removed alan tab action stays absent
- **WHEN** a user creates new workspace content through default shell surfaces
- **THEN** alan offers normal terminal tab creation and supported non-terminal
  content creation without offering first-party alan tab creation
