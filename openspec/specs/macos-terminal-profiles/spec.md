# macos-terminal-profiles Specification

## Purpose
Define Alan for macOS Terminal Profiles: local, channel-scoped terminal startup
identities that can be bound to Spaces and panes without becoming provider
connection profiles or workspace-owned secret state.
## Requirements
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
- `managed_user`
- `custom_command`

#### Scenario: Login shell profile
- **WHEN** a terminal is launched with a `login_shell` Terminal Profile
- **THEN** alan launches the resolved login shell using the existing login-shell
  startup behavior

#### Scenario: Sudo Unix user profile
- **WHEN** a terminal is launched with a `sudo_user` Terminal Profile for Unix
  user `alan`
- **THEN** alan launches `/usr/bin/sudo` with structured arguments equivalent
  to `sudo -iu alan`
- **AND** alan does not require the user to store that sudo invocation as a
  freeform shell command
- **AND** alan treats the profile as operator managed rather than Alan Managed
  User state

#### Scenario: Sudo root profile
- **WHEN** a terminal is launched with a `sudo_root` Terminal Profile
- **THEN** alan launches `/usr/bin/sudo` with structured arguments equivalent
  to `sudo -i`

#### Scenario: Managed user profile
- **WHEN** a terminal is launched with a `managed_user` Terminal Profile for
  Unix user `alan`
- **THEN** alan resolves the profile through the Managed User helper-backed
  launch path
- **AND** alan requests a helper-owned PTY session for the Alan-managed account
- **AND** alan does not launch `/usr/bin/sudo` for the managed-user profile

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

#### Scenario: Managed user profile requires an Alan-managed account
- **WHEN** alan saves or uses a `managed_user` Terminal Profile
- **THEN** the profile definition references a Managed Terminal Account
- **AND** terminal launch requires that account to verify ready through the
  privileged helper
- **AND** the profile cannot be converted into a freeform sudo command

#### Scenario: Custom command requires a command
- **WHEN** the user saves a `custom_command` Terminal Profile without a command
- **THEN** alan rejects the profile definition
- **AND** alan keeps the previous saved profile definition unchanged

#### Scenario: Missing executable falls back safely
- **WHEN** alan cannot resolve the executable or helper-backed launch path
  needed for a Terminal Profile launch mode
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
configure passwordless sudo as part of general Terminal Profile management.
Managed User account provisioning SHALL route through the Managed Users helper
surface, not the Terminal Profile editor.

#### Scenario: Sudo requires password
- **WHEN** a `sudo_user` or `sudo_root` Terminal Profile launches and sudo asks
  for a password
- **THEN** the prompt appears inside the terminal session
- **AND** alan does not intercept or store the password

#### Scenario: Passwordless sudo is configured externally
- **WHEN** the operator has configured passwordless sudo for the requested Unix
  user outside Alan
- **THEN** the `sudo_user` Terminal Profile launches without additional
  Alan-specific configuration

#### Scenario: Managed user profile is edited
- **WHEN** the user inspects a Terminal Profile owned by a Managed User
- **THEN** alan presents the profile as read-only managed state
- **AND** account repair, helper readiness, and legacy sudoers cleanup remain in
  the Managed Users surface

### Requirement: Managed User Profiles Use Helper Launch Identity
Alan for macOS SHALL represent helper-backed Managed User Terminal Profiles
with `managed_user` launch identity rather than `sudo_user`.

#### Scenario: Legacy managed profile is migrated
- **WHEN** a previously Alan-managed profile uses `sudo_user` and the matching
  Managed User is migrated to the privileged helper path
- **THEN** alan updates or recreates the managed profile as `managed_user`
- **AND** alan preserves the user-visible label and managed ownership marker

#### Scenario: Manual sudo profile is preserved
- **WHEN** a non-managed Terminal Profile uses `sudo_user`
- **THEN** alan preserves the manual profile as operator-managed startup state
- **AND** alan does not convert it to `managed_user` without Managed User
  ownership evidence
