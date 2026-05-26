## MODIFIED Requirements

### Requirement: Command UI Is Not Expanded By The First Registry
The first macOS shell action registry pass SHALL NOT add or preserve removed
`Go to or Command...`, Ask alan, typed command input, candidate filtering, or
target-selection behavior.

#### Scenario: Registry is introduced
- **WHEN** the action registry is added or updated
- **THEN** removed Ask alan and floating command input behavior remains absent
- **AND** new Tab or Space organization actions are not exposed through a typed
  Command UI

## ADDED Requirements

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
