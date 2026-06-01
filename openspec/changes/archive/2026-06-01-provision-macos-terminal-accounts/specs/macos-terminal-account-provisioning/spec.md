## ADDED Requirements

### Requirement: Managed Terminal Accounts Are Terminal-Only Local Users
Alan for macOS SHALL provision Managed Terminal Accounts as local standard Unix
accounts for terminal identity isolation and SHALL NOT enable macOS GUI
automatic login as part of this feature.

#### Scenario: Create standard terminal account
- **WHEN** the user provisions Managed Terminal Account `alan`
- **THEN** alan creates or repairs a local standard account named `alan`
- **AND** the account has a home directory and login shell suitable for terminal
  sessions
- **AND** the account is not granted administrator privileges by default

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

#### Scenario: Dry run previews changes
- **WHEN** the user starts Managed Terminal Account provisioning
- **THEN** alan presents a dry-run plan listing account creation or repair,
  home-directory handling, shell selection, hidden-account handling, sudoers
  changes, validation, verification, and Terminal Profile creation
- **AND** alan does not apply privileged changes during dry run

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

### Requirement: Sudoers Rules Are Narrow And Validated
Alan for macOS SHALL configure passwordless terminal entry through Alan-owned
sudoers drop-ins that permit the GUI user to run as only the selected Managed
Terminal Account.

#### Scenario: Sudoers rule targets one account
- **WHEN** GUI user `morris` provisions Managed Terminal Account `alan`
- **THEN** alan generates a sudoers rule allowing `morris` to run commands as
  `alan` without a password
- **AND** the rule does not grant passwordless root access
- **AND** the rule does not grant passwordless access to unrelated users

#### Scenario: Sudoers syntax is validated
- **WHEN** alan writes or updates its sudoers drop-in
- **THEN** alan validates the resulting sudoers configuration with a sudoers
  syntax checker such as `visudo -cf`
- **AND** alan does not mark provisioning ready when sudoers validation fails

#### Scenario: User-controlled sudoers fragments are rejected
- **WHEN** the user chooses account names or labels during provisioning
- **THEN** alan treats those values as structured identifiers
- **AND** alan does not insert raw user-provided sudoers text into the generated
  drop-in

### Requirement: Provisioning Verification Is Mandatory
Alan for macOS SHALL verify a Managed Terminal Account before marking it ready
or binding it to a Terminal Profile.

#### Scenario: Account readiness check passes
- **WHEN** account lookup, home directory, shell, sudoers validation, and
  non-interactive `sudo -iu <target>` checks all pass
- **THEN** alan marks the Managed Terminal Account ready
- **AND** alan may create or update the matching Terminal Profile

#### Scenario: Non-interactive sudo fails
- **WHEN** `sudo -n -iu alan true` fails for the current GUI user
- **THEN** alan does not mark Managed Terminal Account `alan` ready
- **AND** alan presents repair guidance instead of silently accepting a profile
  that will prompt or fail later

#### Scenario: Partial provisioning is repairable
- **WHEN** account creation succeeds but sudoers verification fails
- **THEN** alan records the account as partially provisioned
- **AND** alan offers a repair plan that targets the failed sudoers or
  verification step

### Requirement: Terminal Profile Handoff Follows Successful Provisioning
Alan for macOS SHALL create or update a matching Terminal Profile only after the
Managed Terminal Account is verified ready.

#### Scenario: Ready account creates profile
- **WHEN** Managed Terminal Account `alan` verifies successfully
- **THEN** alan creates or updates Terminal Profile `alan`
- **AND** the profile uses structured `sudo_user` launch mode for Unix user
  `alan`

#### Scenario: Failed account does not create ready profile
- **WHEN** Managed Terminal Account `alan` fails verification
- **THEN** alan does not create a ready Terminal Profile that claims `alan` can
  be entered without repair

#### Scenario: Current Space can be bound after success
- **WHEN** provisioning succeeds from a Space-scoped flow
- **THEN** alan may bind the current Space to the created Terminal Profile after
  explicit user confirmation

### Requirement: Rollback Is Conservative
Alan for macOS SHALL provide rollback for Alan-owned integration state and SHALL
treat account or home-directory deletion as separate destructive actions.

#### Scenario: Rollback removes Alan-owned integration
- **WHEN** the user rolls back provisioning for Managed Terminal Account `alan`
- **THEN** alan removes or disables Alan-owned sudoers entries for `alan`
- **AND** alan removes or disables the matching Terminal Profile when it was
  created by the provisioning flow

#### Scenario: Account deletion requires separate confirmation
- **WHEN** rollback would delete local account `alan` or `/Users/alan`
- **THEN** alan requires a separate explicit destructive confirmation
- **AND** ordinary rollback does not remove the home directory

#### Scenario: Existing non-Alan account is preserved
- **WHEN** account `alan` existed before Alan provisioning
- **THEN** alan does not delete that account during rollback
- **AND** alan limits rollback to Alan-owned sudoers/Profile integration unless
  the user chooses a separate destructive operation
