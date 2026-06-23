## MODIFIED Requirements

### Requirement: Managed Terminal Account Provisioning Has Focused Verification
Alan for macOS SHALL require focused verification for Managed Terminal Account
planning, sudoers generation, validation, execution boundaries, repair,
rollback, multiple-user catalog behavior, read-only managed profiles, and UI
safety wording.

#### Scenario: Dry-run planner tests run
- **WHEN** provisioning planning behavior changes
- **THEN** focused tests cover missing account, existing account, existing
  sudoers entry, missing Terminal Profile, and already-ready account states

#### Scenario: Multiple managed user tests run
- **WHEN** Managed User catalog, creation, or display behavior changes
- **THEN** focused tests cover multiple users with independent status, Unix user
  name plus display label input, derived home/shell/sudoers/profile values, and
  no automatic Space binding after successful creation

#### Scenario: Sudoers generation tests run
- **WHEN** sudoers rendering behavior changes
- **THEN** focused tests cover generated rule scope, identifier escaping or
  rejection, no passwordless root grant, no unrelated-user grant, and stable
  Alan-owned file paths

#### Scenario: Validation failure tests run
- **WHEN** sudoers validation or non-interactive sudo verification behavior
  changes
- **THEN** focused tests cover validation failure, sudo failure, partial
  provisioning state, and repair-plan generation

#### Scenario: Managed profile tests run
- **WHEN** Terminal Profile handoff or editing behavior changes
- **THEN** focused tests cover read-only managed profiles, editable non-managed
  profiles, missing managed profile repair state, and failed provisioning not
  creating a ready profile

#### Scenario: Space menu default tests run
- **WHEN** Space profile menu or terminal startup resolution changes
- **THEN** focused tests cover `Login shell` selected for unbound Spaces,
  absence of a separate `Default` profile item, managed user selection binding
  the Space, and selecting `Login shell` clearing the binding

#### Scenario: Rollback tests run
- **WHEN** rollback behavior changes
- **THEN** focused tests cover removal of Alan-owned sudoers/Profile integration
- **AND** tests confirm account and home-directory deletion require a separate
  destructive confirmation

#### Scenario: UI safety tests run
- **WHEN** Settings provisioning UI changes
- **THEN** focused UI or model tests cover no GUI-autologin wording, privileged
  plan preview, explicit confirmation, password redaction, ready state,
  repairable state, and conflict state
