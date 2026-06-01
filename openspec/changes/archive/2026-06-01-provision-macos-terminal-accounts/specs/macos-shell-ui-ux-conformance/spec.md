## ADDED Requirements

### Requirement: Settings Presents Managed Terminal Account Provisioning
Alan macOS Settings SHALL provide a Managed Terminal Account flow for creating
terminal-only local users and SHALL distinguish it from macOS GUI automatic
login.

#### Scenario: Provision action is local and explicit
- **WHEN** the user opens Terminal Profile or local terminal identity settings
- **THEN** alan offers an explicit action to create or repair a Managed Terminal
  Account
- **AND** alan labels the flow as terminal account provisioning, not autologin

#### Scenario: GUI automatic login is not implied
- **WHEN** the provisioning flow describes the result
- **THEN** alan states that it does not enable macOS GUI automatic login
- **AND** alan describes the result as passwordless terminal entry from the
  current GUI user to the target Unix user

#### Scenario: Privileged plan is reviewed before apply
- **WHEN** the user reaches the apply step
- **THEN** alan shows the planned privileged changes in compact user-facing
  language
- **AND** the user must confirm before alan applies account, sudoers, or Terminal
  Profile changes

### Requirement: Provisioning UI Surfaces Safety State
Alan macOS Settings SHALL surface readiness, repair, and rollback state for
Managed Terminal Accounts without exposing passwords or raw privileged command
payloads in normal UI.

#### Scenario: Ready account is shown
- **WHEN** a Managed Terminal Account is verified ready
- **THEN** alan shows the account as ready for terminal entry
- **AND** alan links it to the matching Terminal Profile when one exists

#### Scenario: Repairable account is shown
- **WHEN** a Managed Terminal Account is partially provisioned or fails
  verification
- **THEN** alan shows a repairable state with the failed step
- **AND** alan offers to preview a repair plan

#### Scenario: Passwords are not displayed
- **WHEN** provisioning uses generated or administrator-entered passwords
- **THEN** alan does not show those passwords in Settings after the operation
- **AND** alan does not write them into normal shell state, workspace manifests,
  or Terminal Profile definitions
