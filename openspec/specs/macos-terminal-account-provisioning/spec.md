# macos-terminal-account-provisioning Specification

## Purpose
Define Managed Terminal Account provisioning for Alan for macOS: previewable,
explicitly authorized creation and repair of terminal-only local Unix accounts,
narrow sudoers entry, readiness verification, Terminal Profile handoff, and
conservative rollback.
## Requirements
### Requirement: Managed Terminal Accounts Are Terminal-Only Local Users
Alan for macOS SHALL provision Managed Terminal Accounts as local standard Unix
accounts for terminal identity isolation and SHALL NOT enable macOS GUI
automatic login as part of this feature. Alan SHALL support multiple Managed
Terminal Accounts on the same Mac.

#### Scenario: Create standard terminal account
- **WHEN** the user provisions Managed Terminal Account `alan`
- **THEN** alan creates or repairs a local standard account named `alan`
- **AND** the account has a home directory and login shell suitable for terminal
  sessions
- **AND** the account is not granted administrator privileges by default

#### Scenario: Create multiple terminal accounts
- **WHEN** the user provisions Managed Terminal Accounts `alan`, `univer`, and
  `lab`
- **THEN** alan tracks each account as a separate terminal-only local user
- **AND** each account has independent readiness, sudoers, verification, and
  Terminal Profile handoff state

#### Scenario: GUI automatic login remains unchanged
- **WHEN** alan provisions a Managed Terminal Account
- **THEN** alan does not enable macOS GUI automatic login for that account
- **AND** alan does not change the Mac's existing GUI automatic-login setting

#### Scenario: Account is hidden from login UI by default
- **WHEN** alan provisions a Managed Terminal Account with default options
- **THEN** alan marks the account as hidden from normal GUI login-window user
  lists where the operating system supports that setting
- **AND** terminal entry remains the readiness criterion for the account

### Requirement: Provisioning Uses Explicit Privileged Plans
Alan for macOS SHALL represent account provisioning as a previewable privileged
plan that the user must explicitly approve before local system state is changed.
The V1 creation form SHALL ask only for Unix user name and display label.

#### Scenario: Dry run previews changes
- **WHEN** the user starts Managed Terminal Account provisioning
- **THEN** alan presents a dry-run plan listing account creation or repair,
  home-directory handling, shell selection, hidden-account handling, sudoers
  changes, validation, verification, and Terminal Profile creation
- **AND** alan does not apply privileged changes during dry run

#### Scenario: Creation input stays narrow
- **WHEN** the user creates a Managed Terminal Account
- **THEN** alan asks for a Unix user name and display label
- **AND** alan derives home directory, login shell, hidden-login behavior,
  sudoers path, verification command, and Terminal Profile id from structured
  defaults

#### Scenario: User cancels before apply
- **WHEN** the user cancels the previewed provisioning plan
- **THEN** alan does not create accounts, modify sudoers, or create Terminal
  Profiles

#### Scenario: Privileged executor is isolated
- **WHEN** alan applies an approved provisioning plan
- **THEN** privileged account and sudoers operations are routed through a
  narrow executor boundary
- **AND** ordinary Settings rendering and shell workspace state do not receive
  reusable privileged credentials

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
- **AND** the profile uses structured `sudo_user` launch mode for Unix user
  `alan`
- **AND** the profile records that it is owned by Managed Terminal Account
  `alan`

#### Scenario: Managed profile is read-only
- **WHEN** the user views Terminal Profile `alan` created by Managed Terminal
  Account `alan`
- **THEN** alan shows it as a managed read-only Terminal Profile
- **AND** alan routes repair, refresh, and removal actions through Managed
  Users instead of allowing direct profile edits

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

### Requirement: Managed Terminal User Catalog Is User Facing
Alan for macOS SHALL present Managed Terminal Accounts as a user-facing catalog
separate from Terminal Profiles.

#### Scenario: Managed users list is shown
- **WHEN** the user opens Settings > Terminal
- **THEN** alan lists Managed Users by display label and Unix user name
- **AND** alan shows readiness, repair, conflict, or partial state for each
  Managed User

#### Scenario: Managed user actions are state based
- **WHEN** a Managed User is missing, ready, partial, repairable, or conflicting
- **THEN** alan exposes only actions that match the state, such as Create,
  Review, Repair, Verify, or Remove
- **AND** alan does not offer raw sudoers or privileged-script editing
