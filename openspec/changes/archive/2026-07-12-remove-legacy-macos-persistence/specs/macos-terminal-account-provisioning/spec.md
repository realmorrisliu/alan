## MODIFIED Requirements

### Requirement: Managed Terminal Accounts Are Terminal-Only Local Users
Alan for macOS SHALL provision Managed Terminal Accounts as local standard Unix
accounts for terminal identity isolation and SHALL NOT enable macOS GUI
automatic login as part of this feature. Alan SHALL support multiple Managed
Terminal Accounts on the same Mac. Current Managed User ownership and readiness
SHALL NOT depend on sudoers state.

#### Scenario: Create standard terminal account
- **WHEN** the user provisions Managed Terminal Account `alan`
- **THEN** Alan creates or repairs a local standard account named `alan`
- **AND** the account has a home directory and login shell suitable for terminal
  sessions
- **AND** the account is not granted administrator privileges by default

#### Scenario: Create multiple terminal accounts
- **WHEN** the user provisions Managed Terminal Accounts `alan`, `univer`, and
  `lab`
- **THEN** Alan tracks each account as a separate terminal-only local user
- **AND** each account has independent helper ownership, readiness, PTY
  verification, and Terminal Profile handoff state

#### Scenario: GUI automatic login remains unchanged
- **WHEN** Alan provisions a Managed Terminal Account
- **THEN** Alan does not enable macOS GUI automatic login for that account
- **AND** Alan does not change the Mac's existing GUI automatic-login setting

#### Scenario: Account is hidden from login UI by default
- **WHEN** Alan provisions a Managed Terminal Account with default options
- **THEN** Alan marks the account as hidden from normal GUI login-window user
  lists where the operating system supports that setting
- **AND** helper-managed terminal entry remains the readiness criterion for the
  account

### Requirement: Provisioning Uses Explicit Privileged Plans
Alan for macOS SHALL represent account provisioning as a previewable privileged
plan that the user must explicitly approve before local system state is changed.
The V1 creation form SHALL ask only for Unix user name and display label. The
plan SHALL use current helper account and ownership-marker operations and SHALL
NOT contain sudoers rendering, validation, installation, or cleanup steps.

#### Scenario: Dry run previews changes
- **WHEN** the user starts Managed Terminal Account provisioning
- **THEN** Alan presents a dry-run plan listing account creation or repair,
  home-directory handling, shell selection, hidden-account handling,
  ownership-marker changes, verification, and Terminal Profile creation
- **AND** Alan does not apply privileged changes during dry run

#### Scenario: Creation input stays narrow
- **WHEN** the user creates a Managed Terminal Account
- **THEN** Alan asks for a Unix user name and display label
- **AND** Alan derives home directory, login shell, hidden-login behavior,
  ownership-marker identity, verification operation, and Terminal Profile id
  from structured defaults

#### Scenario: User cancels before apply
- **WHEN** the user cancels the previewed provisioning plan
- **THEN** Alan does not create accounts, write ownership markers, or create
  Terminal Profiles

#### Scenario: Privileged executor is isolated
- **WHEN** Alan applies an approved provisioning plan
- **THEN** privileged account and ownership operations are routed through the
  signed helper's narrow typed boundary
- **AND** ordinary Settings rendering and shell workspace state do not receive
  reusable privileged credentials

#### Scenario: Plan attempts a sudoers operation
- **WHEN** a provisioning or repair plan contains a sudoers path, content,
  validation, install, or cleanup step
- **THEN** current plan validation rejects it as unsupported
- **AND** the helper performs no sudoers inspection or mutation

### Requirement: Provisioning Verification Is Mandatory
Alan for macOS SHALL verify a Managed Terminal Account before marking it ready
or binding it to a Terminal Profile. Helper-backed readiness SHALL require
account lookup, home directory, shell, current helper ownership evidence,
helper diagnosis, and helper-managed PTY spawn verification.

#### Scenario: Account readiness check passes
- **WHEN** helper-backed account lookup, home directory, shell, ownership
  evidence, helper diagnosis, and managed-user PTY smoke verification all pass
- **THEN** Alan marks the Managed Terminal Account ready
- **AND** Alan may create or update the matching managed Terminal Profile

#### Scenario: Helper PTY smoke verification fails
- **WHEN** the helper cannot start a managed-user PTY for account `alan`
- **THEN** Alan does not mark Managed Terminal Account `alan` ready
- **AND** Alan presents repair guidance or a sanitized `ptySpawnFailed` state
  instead of accepting a profile that will fail later

#### Scenario: Partial provisioning is repairable
- **WHEN** account creation succeeds but current helper diagnosis or
  managed-user PTY verification fails
- **THEN** Alan records the account as partially provisioned
- **AND** Alan offers a helper-backed repair plan targeting the failed account,
  home, shell, ownership-marker, or verification step

#### Scenario: Sudoers exists for the account
- **WHEN** an unrelated or historical sudoers entry exists for the same Unix
  account after the hard cut
- **THEN** current Managed User diagnosis does not use it as ownership or
  readiness evidence
- **AND** current provisioning does not inspect, repair, or remove it

### Requirement: Terminal Profile Handoff Follows Successful Provisioning
Alan for macOS SHALL create or update a matching Terminal Profile only after the
Managed Terminal Account is verified ready. Managed Terminal Profiles SHALL use
the helper-backed `managed_user` launch identity, SHALL be read-only from the
Terminal Profile editor, and SHALL be maintained through the Managed Users
surface.

#### Scenario: Ready account creates profile
- **WHEN** Managed Terminal Account `alan` verifies successfully
- **THEN** Alan creates or updates Terminal Profile `alan`
- **AND** the profile uses structured `managed_user` launch identity for Unix
  user `alan`
- **AND** the profile records that it is owned by Managed Terminal Account
  `alan`

#### Scenario: Managed profile is read-only
- **WHEN** the user views Terminal Profile `alan` created by Managed Terminal
  Account `alan`
- **THEN** Alan shows it as a managed read-only Terminal Profile
- **AND** Alan routes repair, refresh, and removal actions through Managed Users
  instead of allowing direct profile edits

#### Scenario: Failed account does not create ready profile
- **WHEN** Managed Terminal Account `alan` fails verification
- **THEN** Alan does not create a ready Terminal Profile that claims `alan` can
  be entered without repair

#### Scenario: Successful provisioning does not bind current Space
- **WHEN** Managed Terminal Account `alan` verifies successfully
- **THEN** Alan makes its Terminal Profile available for explicit Space
  selection
- **AND** Alan does not bind the current Space to `alan` automatically
- **AND** Alan does not change the default terminal identity from `Login shell`

#### Scenario: Retired managed sudo profile is loaded
- **WHEN** a profile marked as Managed-User-owned uses the retired `sudo_user`
  launch identity
- **THEN** Alan does not migrate or treat it as a ready helper-backed profile
- **AND** current Managed User repair requires a canonical `managed_user`
  profile to be authored from current state

### Requirement: Rollback Is Conservative
Alan for macOS SHALL provide rollback for current Alan-owned helper integration
state and SHALL treat account or home-directory deletion as separate destructive
actions. Steady-state rollback SHALL NOT discover, verify, or remove legacy
sudoers state.

#### Scenario: Rollback removes Alan-owned integration
- **WHEN** the user rolls back provisioning for Managed Terminal Account `alan`
- **THEN** Alan removes or disables current helper-owned integration state for
  `alan`
- **AND** Alan removes or disables the matching Terminal Profile when it was
  created by the provisioning flow
- **AND** Alan does not inspect or mutate a sudoers path

#### Scenario: Account deletion requires separate confirmation
- **WHEN** rollback would delete local account `alan` or `/Users/alan`
- **THEN** Alan requires a separate explicit destructive confirmation
- **AND** ordinary rollback does not remove the home directory

#### Scenario: Existing non-Alan account is preserved
- **WHEN** account `alan` existed before Alan provisioning or lacks current
  helper-owned ownership evidence
- **THEN** Alan does not delete or repair that account during rollback
- **AND** Alan limits rollback to current Alan-owned integration unless the user
  chooses a separate destructive operation for an Alan-managed account

#### Scenario: Legacy sudoers entry remains after hard cut
- **WHEN** rollback encounters an account that once used Alan's retired sudoers
  integration
- **THEN** steady-state rollback neither claims ownership from that entry nor
  removes it
- **AND** removal of that historical entry remains outside steady-state Alan
