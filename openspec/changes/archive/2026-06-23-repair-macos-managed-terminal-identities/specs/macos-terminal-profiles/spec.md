## MODIFIED Requirements

### Requirement: Terminal Profile Resolution Is Deterministic
Alan for macOS SHALL resolve a terminal startup profile through a deterministic
order that supports explicit overrides, Space defaults, and safe login-shell
fallback. Alan SHALL NOT expose a separate user-facing global default Terminal
Profile; `Login shell` is the default terminal identity when no explicit or
Space-bound profile applies.

Resolution order SHALL be:

1. Explicit terminal creation request profile.
2. Terminal content stored `terminal_profile_id`.
3. Space `terminal_profile_id`.
4. Login-shell fallback.

#### Scenario: Explicit profile wins
- **WHEN** a terminal creation request supplies `terminal_profile_id` `root`
- **THEN** alan launches the new terminal using the `root` Terminal Profile even
  if the Space is bound to `alan`

#### Scenario: Space profile is used by default
- **WHEN** a terminal creation request has no explicit profile and the selected
  Space is bound to `alan`
- **THEN** alan launches the new terminal using the `alan` Terminal Profile

#### Scenario: Unbound Space uses login shell
- **WHEN** a terminal creation request has no explicit profile
- **AND** the selected Space has no `terminal_profile_id`
- **THEN** alan launches the terminal using `Login shell`
- **AND** alan does not resolve a separate global default Terminal Profile

#### Scenario: Missing profile falls back
- **WHEN** terminal startup references `terminal_profile_id` `univer` and no
  local Terminal Profile with that id exists
- **THEN** alan launches the terminal with the login-shell fallback
- **AND** alan preserves the missing profile id for UI and diagnostics

## ADDED Requirements

### Requirement: Login Shell Is Built-In Default Identity
Alan for macOS SHALL treat `Login shell` as a built-in Terminal Profile and the
default terminal identity.

#### Scenario: Profile store is shown
- **WHEN** the user views local Terminal Profiles
- **THEN** alan shows `Login shell` as the built-in default and fallback
- **AND** alan does not show `Default` as a separate Terminal Profile

#### Scenario: Legacy global default is present
- **WHEN** a local Terminal Profile store contains a non-login default profile
  from older behavior
- **THEN** alan preserves the profile definitions
- **AND** new unbound terminal startup still uses `Login shell`
- **AND** Settings presents explicit Space binding as the way to use another
  profile by default for a Space

### Requirement: Managed Terminal Profiles Are Read-Only
Alan for macOS SHALL treat Terminal Profiles created by Managed Users as
read-only profiles maintained by the Managed Users flow.

#### Scenario: Managed profile is inspected
- **WHEN** the user opens a Terminal Profile whose definition includes a managed
  terminal account marker
- **THEN** alan shows the profile launch identity and managed status
- **AND** alan does not allow editing the launch kind, Unix user, working
  directory, or managed marker from the Terminal Profile editor

#### Scenario: Non-managed profile remains editable
- **WHEN** the user opens a Terminal Profile that is not managed by a Managed
  User
- **THEN** alan may provide structured editing for the profile according to the
  Terminal Profile editing contract
