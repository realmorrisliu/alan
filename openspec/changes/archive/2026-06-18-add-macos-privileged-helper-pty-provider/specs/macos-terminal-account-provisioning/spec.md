## MODIFIED Requirements

### Requirement: Managed Terminal Accounts Are Terminal-Only Local Users
Alan for macOS SHALL provision Managed Terminal Accounts as local standard Unix
accounts for terminal identity isolation and SHALL NOT enable macOS GUI
automatic login as part of this feature. Alan SHALL support multiple Managed
Terminal Accounts on the same Mac, and helper-backed management SHALL apply only
to accounts carrying Alan-managed ownership evidence for the active install
channel.

#### Scenario: Create standard terminal account
- **WHEN** the user provisions Managed Terminal Account `alan`
- **THEN** alan creates or repairs a local standard account named `alan`
- **AND** the account has a home directory and login shell suitable for terminal
  sessions
- **AND** the account is not granted administrator privileges by default
- **AND** the account carries Alan-managed ownership evidence for the active
  install channel

#### Scenario: Create multiple terminal accounts
- **WHEN** the user provisions Managed Terminal Accounts `alan`, `univer`, and
  `lab`
- **THEN** alan tracks each account as a separate terminal-only local user
- **AND** each account has independent readiness, helper diagnosis, repair,
  verification, and Terminal Profile handoff state

#### Scenario: GUI automatic login remains unchanged
- **WHEN** alan provisions a Managed Terminal Account
- **THEN** alan does not enable macOS GUI automatic login for that account
- **AND** alan does not change the Mac's existing GUI automatic-login setting

#### Scenario: Account is hidden from login UI by default
- **WHEN** alan provisions a Managed Terminal Account with default options
- **THEN** alan marks the account as hidden from normal GUI login-window user
  lists where the operating system supports that setting
- **AND** terminal entry remains the readiness criterion for the account

#### Scenario: Existing ordinary account is discovered
- **WHEN** a local Unix account exists with the same name as a requested Managed
  Terminal Account but lacks Alan-managed ownership evidence
- **THEN** alan reports the account as not Alan managed
- **AND** alan does not repair, delete, hide, or launch terminals through that
  account as a Managed User

### Requirement: Provisioning Uses Explicit Privileged Plans
Alan for macOS SHALL represent account provisioning as a previewable privileged
plan that the user must explicitly approve before local system state is changed.
The V1 creation form SHALL ask only for Unix user name and display label, and
privileged apply SHALL be performed by the signed privileged helper when the
helper is installed and healthy.

#### Scenario: Dry run previews changes
- **WHEN** the user starts Managed Terminal Account provisioning
- **THEN** alan presents a dry-run plan listing account creation or repair,
  home-directory handling, shell selection, hidden-account handling,
  Alan-managed ownership state, helper verification, legacy sudoers cleanup when
  present, and Terminal Profile creation
- **AND** alan does not apply privileged changes during dry run

#### Scenario: Creation input stays narrow
- **WHEN** the user creates a Managed Terminal Account
- **THEN** alan asks for a Unix user name and display label
- **AND** alan derives home directory, login shell, hidden-login behavior,
  helper ownership state, verification, and Terminal Profile id from structured
  defaults

#### Scenario: User cancels before apply
- **WHEN** the user cancels the previewed provisioning plan
- **THEN** alan does not create accounts, modify legacy sudoers state, or create
  Terminal Profiles

#### Scenario: Privileged executor is isolated
- **WHEN** alan applies an approved provisioning plan
- **THEN** privileged account, home, hidden-login, ownership-marker, legacy
  cleanup, and verification operations are routed through the privileged helper
- **AND** ordinary Settings rendering and shell workspace state do not receive
  reusable privileged credentials

#### Scenario: Helper is unavailable
- **WHEN** the privileged helper is not installed, outdated, invalidly signed,
  or not responding
- **THEN** alan reports helper installation or repair state
- **AND** alan does not fall back to `osascript`, sudoers writes, or
  passwordless sudo as the Managed User apply path

### Requirement: Provisioning Verification Is Mandatory
Alan for macOS SHALL verify a Managed Terminal Account before marking it ready
or binding it to a Terminal Profile. Helper-backed readiness SHALL require
account lookup, home directory, shell, Alan-managed ownership evidence, helper
diagnosis, and helper-managed PTY spawn verification.

#### Scenario: Account readiness check passes
- **WHEN** helper-backed account lookup, home directory, shell, ownership
  evidence, helper diagnosis, and managed-user PTY smoke verification all pass
- **THEN** alan marks the Managed Terminal Account ready
- **AND** alan may create or update the matching managed Terminal Profile

#### Scenario: Helper PTY smoke verification fails
- **WHEN** the helper cannot start a managed-user PTY for account `alan`
- **THEN** alan does not mark Managed Terminal Account `alan` ready
- **AND** alan presents repair guidance or a sanitized `ptySpawnFailed` state
  instead of accepting a profile that will fail later

#### Scenario: Partial provisioning is repairable
- **WHEN** account creation succeeds but helper diagnosis or managed-user PTY
  verification fails
- **THEN** alan records the account as partially provisioned
- **AND** alan offers a helper-backed repair plan that targets the failed
  account, home, shell, ownership, legacy cleanup, or verification step

### Requirement: Terminal Profile Handoff Follows Successful Provisioning
Alan for macOS SHALL create or update a matching Terminal Profile only after the
Managed Terminal Account is verified ready. Managed Terminal Profiles SHALL be
read-only from the Terminal Profile editor and maintained through the Managed
Users surface.

#### Scenario: Ready account creates profile
- **WHEN** Managed Terminal Account `alan` verifies successfully
- **THEN** alan creates or updates Terminal Profile `alan`
- **AND** the profile uses structured `managed_user` launch mode for Unix user
  `alan`
- **AND** the profile records that it is owned by Managed Terminal Account
  `alan`

#### Scenario: Managed profile is read-only
- **WHEN** the user views Terminal Profile `alan` created by Managed Terminal
  Account `alan`
- **THEN** alan shows it as a managed read-only Terminal Profile
- **AND** alan routes repair, refresh, and removal actions through Managed Users
  instead of allowing direct profile edits

#### Scenario: Failed account does not create ready profile
- **WHEN** Managed Terminal Account `alan` fails verification
- **THEN** alan does not create a ready Terminal Profile that claims `alan` can
  be entered without repair

#### Scenario: Successful provisioning does not bind current Space
- **WHEN** Managed Terminal Account `alan` verifies successfully
- **THEN** alan makes its Terminal Profile available for explicit Space
  selection
- **AND** alan does not bind the current Space to `alan` automatically
- **AND** alan does not change the default terminal identity from `Login shell`

### Requirement: Rollback Is Conservative
Alan for macOS SHALL provide rollback for Alan-owned integration state and SHALL
treat account or home-directory deletion as separate destructive actions.
Helper-backed rollback SHALL remove only Alan-managed integration and verified
legacy Alan sudoers state.

#### Scenario: Rollback removes Alan-owned integration
- **WHEN** the user rolls back provisioning for Managed Terminal Account `alan`
- **THEN** alan removes or disables Alan-owned helper integration state for
  `alan`
- **AND** alan removes or disables the matching Terminal Profile when it was
  created by the provisioning flow
- **AND** alan removes a legacy Alan-owned sudoers drop-in only after the helper
  verifies the exact Alan-owned path and contents

#### Scenario: Account deletion requires separate confirmation
- **WHEN** rollback would delete local account `alan` or `/Users/alan`
- **THEN** alan requires a separate explicit destructive confirmation
- **AND** ordinary rollback does not remove the home directory

#### Scenario: Existing non-Alan account is preserved
- **WHEN** account `alan` existed before Alan provisioning or lacks Alan-managed
  ownership evidence
- **THEN** alan does not delete or repair that account during rollback
- **AND** alan limits rollback to Alan-owned integration unless the user chooses
  a separate destructive operation for an Alan-managed account

## REMOVED Requirements

### Requirement: Sudoers Rules Are Narrow And Validated
**Reason**: Helper-backed Managed Users no longer use Alan-owned sudoers
drop-ins as the runtime entry mechanism. The privileged helper owns account
repair and managed-user PTY spawning through typed operations.

**Migration**: Treat existing deterministic Alan-owned sudoers files as legacy
cleanup state. The helper may remove only verified Alan-owned drop-ins matching
the expected path and contents; non-Alan sudoers files are left untouched and do
not make a Managed User ready.
