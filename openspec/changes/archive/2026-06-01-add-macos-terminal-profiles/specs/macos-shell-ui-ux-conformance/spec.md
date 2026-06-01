## ADDED Requirements

### Requirement: Settings Manages Terminal Profiles Locally
Alan macOS Settings SHALL provide a local Terminal Profiles surface for
creating, editing, and choosing the default Terminal Profile without presenting
Terminal Profiles as provider accounts.

#### Scenario: Terminal Profiles appear in Settings
- **WHEN** the user opens Settings
- **THEN** alan shows Terminal Profiles as local terminal startup configuration
- **AND** alan does not place Terminal Profiles under provider Accounts or label
  them as connection profiles

#### Scenario: Structured profile editing
- **WHEN** the user edits a Terminal Profile
- **THEN** alan provides structured controls for login shell, sudo Unix user,
  sudo root, and custom command launch modes
- **AND** required fields are validated before the profile is saved

#### Scenario: Sudo behavior is explained without raw sudoers editing
- **WHEN** the user configures a sudo Unix user or sudo root Terminal Profile
- **THEN** alan explains that sudo prompts and passwordless sudo behavior are
  controlled by the operating system
- **AND** the Terminal Profile editor does not offer raw sudoers-file editing

### Requirement: Spaces Expose Terminal Profile Binding
The macOS shell UI SHALL let users view and change a Space's default Terminal
Profile through compact shell-native affordances.

#### Scenario: Space profile can be selected
- **WHEN** the user opens a Space action menu or profile selector
- **THEN** alan lists local Terminal Profiles by title and launch kind
- **AND** selecting one updates the Space's default `terminal_profile_id`

#### Scenario: Space profile hint stays quiet
- **WHEN** a Space has a Terminal Profile binding
- **THEN** alan may show a compact icon, color, or label hint in the sidebar
- **AND** alan keeps the sidebar scannable and avoids turning Space rows into
  dense configuration panels

#### Scenario: Missing profile is visible
- **WHEN** a Space or terminal content references a missing Terminal Profile
- **THEN** alan shows a missing-profile state with the missing id
- **AND** alan keeps terminal creation available through login-shell fallback

### Requirement: Terminal Profile Details Stay Appropriately Redacted
Alan macOS shell UI SHALL expose Terminal Profile identity in normal shell
surfaces without leaking unnecessary command details.

#### Scenario: Custom command is not shown in normal sidebar rows
- **WHEN** a terminal content uses a `custom_command` Terminal Profile
- **THEN** normal sidebar and pane chrome use the profile title or kind
- **AND** the full custom command is shown only in Settings or explicit
  diagnostics

#### Scenario: Root profile is visibly distinct
- **WHEN** terminal content uses a sudo root Terminal Profile
- **THEN** alan presents a restrained but clear root identity indicator in
  terminal chrome or status surfaces
