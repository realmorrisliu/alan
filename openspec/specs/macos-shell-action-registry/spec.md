# macos-shell-action-registry Specification

## Purpose
TBD - created by archiving change add-macos-shell-action-registry. Update Purpose after archive.
## Requirements
### Requirement: Shell Actions Have Stable Registry Entries
The macOS shell SHALL define shared shell operations through a shell-only action
registry. Each registered action SHALL have a stable action ID, user-facing
title, target kind, availability check, execution handler, and optional default
shortcut descriptor.

#### Scenario: Registered action is described
- **WHEN** a menu, context menu, or keyboard surface asks for a shell action
- **THEN** the action registry returns the same stable action ID, label,
  availability, target kind, and shortcut metadata for that action

#### Scenario: Action IDs are stable
- **WHEN** a shell action label or menu placement changes
- **THEN** the action keeps its stable `action_id` unless the behavior contract
  is intentionally replaced

#### Scenario: Action is unavailable
- **WHEN** a registered action cannot run for the current target
- **THEN** the registry reports an unavailable state and the execution handler
  does not mutate shell state

### Requirement: Action Targets Are Explicit
The macOS shell action registry SHALL distinguish current-selection targets from
context targets so different entrypoints can share an action without silently
retargeting user intent.

#### Scenario: Keyboard shortcut targets current selection
- **WHEN** a keyboard shortcut invokes a Tab or Pane action
- **THEN** the registry resolves the target from the current selected Tab or
  focused pane

#### Scenario: Tab context menu targets clicked tab
- **WHEN** a user opens a context menu for a non-selected Tab
- **THEN** Tab actions in that menu resolve against the context Tab without
  first selecting it

#### Scenario: Additional target is required
- **WHEN** an action such as `Move Tab to Space...` requires a destination Space
- **THEN** the invoking surface supplies that destination explicitly before the
  action mutates shell state

### Requirement: Registry Owns Menu Context And Keyboard Routing
The macOS shell SHALL route shared menu bar, tab/space context menu, and shell
keyboard shortcut actions through the shell action registry where the surfaces
perform the same behavior.

#### Scenario: Menu invokes shell action
- **WHEN** the user chooses a registered shell action from the menu bar
- **THEN** the menu invokes the registry action rather than a separate
  view-local mutation path

#### Scenario: Context menu invokes shell action
- **WHEN** the user chooses a registered Tab or Space action from a context menu
- **THEN** the context menu invokes the registry action with its context target

#### Scenario: Keyboard invokes shell action
- **WHEN** the user presses a registered shell shortcut
- **THEN** the keyboard path invokes the same registry action used by menu and
  context surfaces

### Requirement: Command UI Is Not Expanded By The First Registry
The first macOS shell action registry pass SHALL NOT add or preserve removed
`Go to or Command...`, Ask alan, typed command input, candidate filtering, or
target-selection behavior.

#### Scenario: Registry is introduced
- **WHEN** the action registry is added or updated
- **THEN** removed Ask alan and floating command input behavior remains absent
- **AND** new Tab or Space organization actions are not exposed through a typed
  Command UI

### Requirement: Registry excludes removed alan actions
The macOS shell action registry SHALL NOT register first-party alan tab creation
or Ask alan command-input actions.

#### Scenario: Action descriptors are enumerated
- **WHEN** menus, context menus, keyboard dispatch, or tests enumerate shell
  action descriptors
- **THEN** no descriptor has the removed `newAlanTab`, `commandInputOpen`, Ask
  alan, New alan Tab, or Command-P command input behavior

#### Scenario: Terminal tab creation remains registered
- **WHEN** shell action descriptors are enumerated
- **THEN** normal terminal tab creation remains available through the registry
  without requiring a first-party alan tab action

