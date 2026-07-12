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

### Requirement: Sudo Configuration Remains Operator Managed
Alan for macOS SHALL NOT edit sudoers files, create Unix users, or silently
configure passwordless sudo as part of general Terminal Profile management.
Manually authored `sudo_user` and `sudo_root` profiles SHALL remain
operator-managed. Managed User account provisioning SHALL route through the
signed helper and SHALL NOT inspect or clean legacy sudoers state through the
Terminal Profile editor or Managed Users steady-state UI.

#### Scenario: Sudo requires password
- **WHEN** an operator-authored `sudo_user` or `sudo_root` Terminal Profile
  launches and sudo asks for a password
- **THEN** the prompt appears inside the terminal session
- **AND** Alan does not intercept or store the password

#### Scenario: Passwordless sudo is configured externally
- **WHEN** the operator has configured passwordless sudo for the requested Unix
  user outside Alan
- **THEN** the operator-authored `sudo_user` Terminal Profile launches without
  additional Alan-specific configuration

#### Scenario: Managed user profile is edited
- **WHEN** the user inspects a Terminal Profile owned by a Managed User
- **THEN** Alan presents the profile as read-only current helper-managed state
- **AND** account repair and helper readiness remain in the Managed Users
  surface
- **AND** neither surface offers legacy sudoers discovery or cleanup

### Requirement: Managed User Profiles Use Helper Launch Identity
Alan for macOS SHALL represent helper-backed Managed User Terminal Profiles
with `managed_user` launch identity rather than `sudo_user`. It SHALL NOT
migrate an old Managed-User-owned `sudo_user` profile at load time.

#### Scenario: Current managed profile is loaded
- **WHEN** a Managed-User-owned profile uses `managed_user` and references a
  current helper-owned account
- **THEN** Alan resolves it as a helper-backed Managed User launch identity

#### Scenario: Retired managed sudo profile is loaded
- **WHEN** a profile marked as Managed-User-owned uses `sudo_user`
- **THEN** Alan treats it as unsupported or not ready for helper-backed launch
- **AND** Alan does not rewrite it into `managed_user` through a compatibility
  migration

#### Scenario: Manual sudo profile is preserved
- **WHEN** a non-managed Terminal Profile uses `sudo_user`
- **THEN** Alan preserves the manual profile as operator-managed startup state
- **AND** Alan does not convert it to `managed_user` without current Managed
  User ownership evidence

### Requirement: macOS delegates Terminal Profile domain semantics to shell core
Alan for macOS SHALL delegate Terminal Profile document validation, editor
semantics, deterministic resolution order, missing/unavailable profile state,
and terminal launch-intent construction to the Rust shell core after the
Terminal Profile module has Rust contract tests and adapter tests.

The macOS platform layer SHALL continue to own profile store file IO, channel
Application Support path selection, terminal runtime spawn translation, and
privileged Managed Terminal Account apply operations.

#### Scenario: Terminal Profile is resolved for pane startup
- **WHEN** macOS creates terminal content with a Terminal Profile reference
  after shell core profile integration
- **THEN** the Rust shell core resolves the profile and returns a launch intent
- **AND** the macOS terminal runtime adapter translates that intent into the
  concrete Ghostty or shell startup operation

#### Scenario: Profile store is saved
- **WHEN** a Terminal Profile document is edited after shell core profile
  integration
- **THEN** the Rust shell core validates document semantics
- **AND** the macOS platform layer writes the document to its channel-scoped
  store location

### Requirement: Privileged account effects stay platform-owned
Privileged account effects SHALL remain platform-owned.

Managed Terminal Account privileged apply, sudoers writes, AppleScript
authorization, account lookup commands, and platform verification executors SHALL
remain outside the shell core and MUST stay platform-owned.

The shell core MAY own portable request, plan, validation, handoff, or profile
intent semantics only when those semantics do not execute OS effects directly.

#### Scenario: Managed account apply is approved
- **WHEN** the user approves a Managed Terminal Account provisioning plan on
  macOS
- **THEN** macOS executes privileged account and sudoers operations through the
  platform-owned executor
- **AND** shell core does not receive reusable privileged credentials or invoke
  AppleScript, sudoers writes, or account-management commands directly

### Requirement: Terminal Profile domain decisions use shell core
The macOS shell SHALL use Rust shell core for Terminal Profile validation,
editor-domain results, deterministic profile resolution, and terminal launch
intent construction once those operations are exposed through the shell-core
facade.

Swift SHALL continue to own profile file storage, corrupt-file preservation,
process spawning, privileged helper readiness checks, and user-interface
presentation.

#### Scenario: Profile definition is validated
- **WHEN** Swift validates or creates a Terminal Profile definition
- **THEN** the domain validation result comes from shell core
- **AND** Swift does not maintain a separate validation implementation for the
  same profile fields

#### Scenario: Terminal launch intent is resolved
- **WHEN** a terminal is created with an explicit, Space, content, global
  default, or fallback profile reference
- **THEN** Swift asks shell core to resolve the launch intent
- **AND** Swift translates the returned intent into macOS process or helper
  startup behavior

#### Scenario: Core profile resolution fails
- **WHEN** shell core cannot resolve a Terminal Profile launch intent because
  the facade fails or the payload is invalid
- **THEN** Swift reports an explicit profile-resolution failure
- **AND** Swift does not silently run a duplicate profile resolution algorithm
  for the same launch request

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
