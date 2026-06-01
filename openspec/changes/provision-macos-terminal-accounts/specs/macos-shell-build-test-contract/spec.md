## ADDED Requirements

### Requirement: Managed Terminal Account Provisioning Has Focused Verification
Alan for macOS SHALL require focused verification for Managed Terminal Account
planning, sudoers generation, validation, execution boundaries, repair,
rollback, and UI safety wording.

#### Scenario: Dry-run planner tests run
- **WHEN** provisioning planning behavior changes
- **THEN** focused tests cover missing account, existing account, existing
  sudoers entry, missing Terminal Profile, and already-ready account states

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

#### Scenario: Rollback tests run
- **WHEN** rollback behavior changes
- **THEN** focused tests cover removal of Alan-owned sudoers/Profile integration
- **AND** tests confirm account and home-directory deletion require a separate
  destructive confirmation

#### Scenario: UI safety tests run
- **WHEN** Settings provisioning UI changes
- **THEN** focused UI or model tests cover no GUI-autologin wording, privileged
  plan preview, explicit confirmation, password redaction, ready state, and
  repairable state
