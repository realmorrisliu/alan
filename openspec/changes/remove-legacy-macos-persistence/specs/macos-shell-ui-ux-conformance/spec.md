## MODIFIED Requirements

### Requirement: Settings Presents Managed Terminal Account Provisioning
Alan macOS Settings SHALL provide a Managed Users surface for creating multiple
terminal-only local users and SHALL distinguish it from macOS GUI automatic
login, operator-managed sudo profiles, and general Terminal Profile editing.
The current surface SHALL present only signed-helper account, ownership,
verification, PTY, and managed-profile operations; it SHALL NOT present
sudoers migration or cleanup state.

#### Scenario: Provision action is local and explicit
- **WHEN** the user opens Terminal Profile or local terminal identity settings
- **THEN** Alan offers an explicit action to create or repair a Managed User
- **AND** Alan labels the flow as terminal account provisioning, not autologin

#### Scenario: Multiple managed users are listed
- **WHEN** the user opens Managed Users
- **THEN** Alan lists every discovered or Alan-managed terminal user by display
  label and Unix user name
- **AND** each row shows current helper readiness or repair state independently

#### Scenario: Creation form is narrow
- **WHEN** the user creates a Managed User
- **THEN** Alan asks for Unix user name and display label
- **AND** Alan does not expose home directory, shell, hidden-login, sudoers, or
  Space binding as primary creation fields

#### Scenario: GUI automatic login is not implied
- **WHEN** the provisioning flow describes the result
- **THEN** Alan states that it does not enable macOS GUI automatic login
- **AND** Alan describes the result as helper-backed terminal entry from the
  current GUI user to the target Unix user

#### Scenario: Privileged plan is reviewed before apply
- **WHEN** the user reaches the apply step
- **THEN** Alan shows current planned account, home, hidden-login,
  ownership-marker, and Terminal Profile changes in compact user-facing
  language
- **AND** the user must confirm before Alan applies those privileged changes
- **AND** the plan contains no sudoers path, content, validation, or cleanup
  operation

#### Scenario: Successful creation is not auto-bound
- **WHEN** Managed User provisioning succeeds
- **THEN** Alan adds the matching Terminal Profile to Settings and Space menus
- **AND** Alan does not automatically bind the current Space
- **AND** Alan does not change the default terminal identity from `Login shell`

### Requirement: Provisioning UI Surfaces Safety State
Alan macOS Settings SHALL surface current readiness, repair, and rollback state
for Managed Users without exposing passwords or raw privileged command payloads
in normal UI. It SHALL NOT expose legacy-sudoers readiness, cleanup, or ownership
states.

#### Scenario: Ready account is shown
- **WHEN** a Managed User is verified ready
- **THEN** Alan shows the account as ready for helper-backed terminal entry
- **AND** Alan links it to the matching read-only Terminal Profile when one
  exists

#### Scenario: Repairable account is shown
- **WHEN** a Managed User is partially provisioned or fails current helper
  verification
- **THEN** Alan shows a repairable state with the failed current step
- **AND** Alan offers to preview a current helper-authored repair plan

#### Scenario: Conflicting account is shown
- **WHEN** a Managed User has an admin account state, missing or conflicting
  helper ownership, or a conflicting unmanaged Terminal Profile
- **THEN** Alan shows a conflict state
- **AND** Alan does not silently overwrite the conflicting local state

#### Scenario: Historical sudoers state exists
- **WHEN** a historical or unmanaged sudoers entry exists for the account after
  the hard cut
- **THEN** the steady-state Managed Users UI does not discover or display it
- **AND** it does not offer a cleanup action

#### Scenario: Passwords are not displayed
- **WHEN** provisioning uses generated or administrator-entered passwords
- **THEN** Alan does not show those passwords in Settings after the operation
- **AND** Alan does not write them into normal shell state, workspace manifests,
  or Terminal Profile definitions
