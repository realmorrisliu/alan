## ADDED Requirements

### Requirement: Terminal Profiles Are Local Startup Identities
Alan for macOS SHALL define Terminal Profiles as machine-local terminal startup
identities that are separate from Alan provider connection profiles.

Terminal Profiles SHALL be stored under the active macOS install channel's
Application Support directory and SHALL NOT be stored in workspace manifests,
agent definitions, `connections.toml`, or provider credential stores.

#### Scenario: Profile store is missing
- **WHEN** Alan for macOS starts and no Terminal Profile store exists for the
  active install channel
- **THEN** alan uses an implicit login-shell Terminal Profile as the fallback
  default
- **AND** terminal creation remains available without user setup

#### Scenario: Corrupt profile store is preserved
- **WHEN** Alan for macOS reads a Terminal Profile store that cannot be decoded
- **THEN** alan preserves the unreadable file as corrupt evidence
- **AND** alan falls back to the implicit login-shell Terminal Profile
- **AND** alan does not prevent the shell workspace from opening

#### Scenario: Terminal profiles do not affect connection profiles
- **WHEN** a user creates or edits a Terminal Profile
- **THEN** alan does not change the selected provider connection profile,
  provider credentials, model selection, or `connection_profile` pins

### Requirement: Terminal Profiles Support Structured Launch Modes
Alan for macOS SHALL support structured Terminal Profile launch modes for the
common terminal identity workflows while preserving an advanced custom command
escape hatch.

Supported V1 launch modes SHALL include:

- `login_shell`
- `sudo_user`
- `sudo_root`
- `custom_command`

#### Scenario: Login shell profile
- **WHEN** a terminal is launched with a `login_shell` Terminal Profile
- **THEN** alan launches the resolved login shell using the existing login-shell
  startup behavior

#### Scenario: Sudo Unix user profile
- **WHEN** a terminal is launched with a `sudo_user` Terminal Profile for Unix
  user `alan`
- **THEN** alan launches `/usr/bin/sudo` with structured arguments equivalent to
  `sudo -iu alan`
- **AND** alan does not require the user to store that sudo invocation as a
  freeform shell command

#### Scenario: Sudo root profile
- **WHEN** a terminal is launched with a `sudo_root` Terminal Profile
- **THEN** alan launches `/usr/bin/sudo` with structured arguments equivalent to
  `sudo -i`

#### Scenario: Custom command profile
- **WHEN** a terminal is launched with a `custom_command` Terminal Profile
- **THEN** alan launches the command through a login-shell command runner such
  as `/bin/zsh -lc`
- **AND** alan treats the profile as an advanced startup mode in user-facing UI

### Requirement: Terminal Profile Definitions Are Validated
Alan for macOS SHALL validate Terminal Profile definitions before saving or
using them for startup.

#### Scenario: Sudo user profile requires a user
- **WHEN** the user saves a `sudo_user` Terminal Profile without a Unix user
- **THEN** alan rejects the profile definition
- **AND** alan explains that a Unix user is required

#### Scenario: Custom command requires a command
- **WHEN** the user saves a `custom_command` Terminal Profile without a command
- **THEN** alan rejects the profile definition
- **AND** alan keeps the previous saved profile definition unchanged

#### Scenario: Missing executable falls back safely
- **WHEN** alan cannot resolve the executable needed for a Terminal Profile
  launch mode
- **THEN** alan marks the Terminal Profile unavailable for startup
- **AND** terminal creation falls back to the login shell with a visible
  unavailable-profile state

### Requirement: Terminal Profile Resolution Is Deterministic
Alan for macOS SHALL resolve a terminal startup profile through a deterministic
order that supports explicit overrides, Space defaults, global defaults, and
safe fallback.

Resolution order SHALL be:

1. Explicit terminal creation request profile.
2. Terminal content stored `terminal_profile_id`.
3. Space `terminal_profile_id`.
4. Global default Terminal Profile.
5. Login-shell fallback.

#### Scenario: Explicit profile wins
- **WHEN** a terminal creation request supplies `terminal_profile_id` `root`
- **THEN** alan launches the new terminal using the `root` Terminal Profile even
  if the Space is bound to `alan`

#### Scenario: Space profile is used by default
- **WHEN** a terminal creation request has no explicit profile and the selected
  Space is bound to `alan`
- **THEN** alan launches the new terminal using the `alan` Terminal Profile

#### Scenario: Missing profile falls back
- **WHEN** terminal startup references `terminal_profile_id` `univer` and no
  local Terminal Profile with that id exists
- **THEN** alan launches the terminal with the login-shell fallback
- **AND** alan preserves the missing profile id for UI and diagnostics

### Requirement: Sudo Configuration Remains Operator Managed
Alan for macOS SHALL NOT edit sudoers files, create Unix users, or silently
configure passwordless sudo as part of Terminal Profile management.

#### Scenario: Sudo requires password
- **WHEN** a `sudo_user` or `sudo_root` Terminal Profile launches and sudo asks
  for a password
- **THEN** the prompt appears inside the terminal session
- **AND** alan does not intercept or store the password

#### Scenario: Passwordless sudo is configured externally
- **WHEN** the operator has configured passwordless sudo for the requested Unix
  user outside Alan
- **THEN** the Terminal Profile launches without additional Alan-specific
  configuration
