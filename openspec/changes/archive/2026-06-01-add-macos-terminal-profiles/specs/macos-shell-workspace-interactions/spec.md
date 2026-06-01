## ADDED Requirements

### Requirement: Spaces Own Default Terminal Profiles
The macOS shell SHALL allow each Space to reference a default Terminal Profile
used for new terminal content created in that Space.

#### Scenario: New Space with profile
- **WHEN** the user creates a Space and selects Terminal Profile `alan`
- **THEN** alan binds the new Space to `terminal_profile_id` `alan`
- **AND** the Space's first terminal content is created with `terminal_profile_id`
  `alan`

#### Scenario: New tab inherits Space profile
- **WHEN** the selected Space is bound to Terminal Profile `univer`
- **AND** the user creates a new terminal tab without an explicit profile
- **THEN** alan creates the new terminal content with `terminal_profile_id`
  `univer`

#### Scenario: New tab explicit profile override
- **WHEN** the selected Space is bound to Terminal Profile `alan`
- **AND** the user creates a new terminal tab explicitly using Terminal Profile
  `root`
- **THEN** alan creates the new terminal content with `terminal_profile_id`
  `root`

### Requirement: Splits Inherit Current Pane Terminal Profile
The macOS shell SHALL create split terminal content using the current pane's
Terminal Profile by default, so split workflows remain within the same Unix
identity unless the user explicitly overrides them.

#### Scenario: Split inherits current pane profile
- **WHEN** the focused terminal pane was created with Terminal Profile `alan`
- **AND** the user creates a split without an explicit profile
- **THEN** alan creates the new split terminal content with
  `terminal_profile_id` `alan`

#### Scenario: Split falls back to Space profile
- **WHEN** the focused pane has no terminal profile reference
- **AND** the selected Space is bound to Terminal Profile `univer`
- **AND** the user creates a split without an explicit profile
- **THEN** alan creates the new split terminal content with
  `terminal_profile_id` `univer`

#### Scenario: Split explicit profile override
- **WHEN** the focused pane was created with Terminal Profile `alan`
- **AND** the user creates a split explicitly using Terminal Profile `root`
- **THEN** alan creates the new split terminal content with
  `terminal_profile_id` `root`

### Requirement: Space Profile Binding Is Not Retroactive
The macOS shell SHALL treat a Space Terminal Profile binding as a default for
future terminal creation, not as a command to migrate existing terminal content.

#### Scenario: Space binding changes
- **WHEN** a Space changes its Terminal Profile binding from `alan` to `univer`
- **THEN** existing terminal content in that Space keeps its stored
  `terminal_profile_id`
- **AND** new terminal content created after the change uses `univer` by default
