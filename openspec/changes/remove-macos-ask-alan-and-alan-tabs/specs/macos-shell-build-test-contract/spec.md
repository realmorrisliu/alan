## MODIFIED Requirements

### Requirement: Shell Action Registry Is Verified
The Apple client SHALL include focused verification for macOS shell action
registry coverage, target resolution, availability, shortcut conflicts, and the
absence of removed Ask alan and alan-tab actions.

#### Scenario: Action IDs are unique
- **WHEN** shell action registry tests run
- **THEN** every registered shell action has a unique stable action ID

#### Scenario: Shortcut conflicts are rejected
- **WHEN** two enabled shell actions in the same keyboard context declare the
  same default shortcut
- **THEN** focused verification fails with enough detail to identify both
  conflicting action IDs

#### Scenario: Context target is preserved
- **WHEN** a context menu action targets a non-selected Tab
- **THEN** focused verification proves the action resolves the context target
  and does not first select the Tab

#### Scenario: Removed actions are absent
- **WHEN** shell action registry tests run
- **THEN** focused checks confirm `newAlanTab`, `commandInputOpen`, Ask alan,
  New alan Tab, and Command-P command input actions are not registered

## ADDED Requirements

### Requirement: Removed Ask alan and alan tab surfaces are verified
The Apple client SHALL include focused contract checks proving Ask alan,
floating command input, Command-P command input, first-party alan tab creation,
and automatic alan-tab runtime launch paths stay removed from active macOS app
surfaces.

#### Scenario: Active source is scanned
- **WHEN** shell contract checks inspect active macOS app source, menus,
  sidebars, command models, App Intents, and automation helpers
- **THEN** the checks fail if they find `Ask alan...`, Command-P command input
  toggles, New alan Tab, `newAlanTab`, Create Alan Tab, or `.alan` tab creation
  paths

#### Scenario: CLI references are preserved
- **WHEN** removal checks inspect CLI documentation or runtime docs
- **THEN** they do not fail solely because `alan ask` or `alan chat` remains
  documented as a CLI command outside the macOS shell tab product surface

#### Scenario: Terminal-launched agent metadata remains testable
- **WHEN** terminal activity tests model a user-launched Alan or coding agent
  process inside a normal terminal tab
- **THEN** tests may verify agent metadata without requiring a first-party
  `.alan` launch target

## REMOVED Requirements

### Requirement: Command input polish has focused verification
**Reason**: The Command-P Ask alan input surface is removed from the macOS shell
product surface.

**Migration**: Verify absence of Ask alan and command-input surfaces instead of
polishing their keyboard flow or material treatment.

### Requirement: Apple shell launches bare alan
**Reason**: The macOS shell no longer has a default alan terminal tab launch
mode.

**Migration**: Verify that the app does not auto-launch `alan chat`, `alan ask`,
or first-party alan tabs. CLI commands remain available inside normal terminal
tabs when the user runs them.
