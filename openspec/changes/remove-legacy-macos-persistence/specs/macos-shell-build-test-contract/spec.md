## MODIFIED Requirements

### Requirement: Managed Terminal Account Provisioning Has Focused Verification
Alan for macOS SHALL require focused verification for current Managed Terminal
Account planning, helper ownership, execution boundaries, repair, rollback,
multiple-user catalog behavior, read-only managed profiles, PTY readiness, and
UI safety wording. Tests SHALL NOT preserve sudoers-based Managed User behavior
as a current oracle.

#### Scenario: Dry-run planner tests run
- **WHEN** provisioning planning behavior changes
- **THEN** focused tests cover missing account, existing account, missing or
  conflicting helper ownership, missing Terminal Profile, PTY failure, and
  already-ready account states

#### Scenario: Multiple managed user tests run
- **WHEN** Managed User catalog, creation, or display behavior changes
- **THEN** focused tests cover multiple users with independent status, Unix user
  name plus display label input, derived home/shell/ownership/profile values,
  and no automatic Space binding after successful creation

#### Scenario: Helper ownership tests run
- **WHEN** ownership-marker diagnosis or repair behavior changes
- **THEN** focused tests cover current-channel marker creation, missing marker,
  mismatched channel, non-Alan-owned existing account, and refusal to infer
  ownership from a sudoers entry

#### Scenario: PTY verification failure tests run
- **WHEN** helper-managed PTY readiness behavior changes
- **THEN** focused tests cover PTY spawn success, sanitized PTY failure, partial
  provisioning state, and repair-plan generation

#### Scenario: Managed profile tests run
- **WHEN** Terminal Profile handoff or editing behavior changes
- **THEN** focused tests cover canonical `managed_user` profiles, read-only
  managed profiles, editable non-managed sudo profiles, missing managed profile
  repair state, and rejection of retired managed `sudo_user` migration

#### Scenario: Space menu default tests run
- **WHEN** Space profile menu or terminal startup resolution changes
- **THEN** focused tests cover `Login shell` selected for unbound Spaces,
  absence of a separate `Default` profile item, managed user selection binding
  the Space, and selecting `Login shell` clearing the binding

#### Scenario: Rollback tests run
- **WHEN** rollback behavior changes
- **THEN** focused tests cover removal of current helper-owned integration and
  Managed User profile state without sudoers inspection
- **AND** tests confirm account and home-directory deletion require a separate
  destructive confirmation

#### Scenario: Legacy Managed User surface absence is checked
- **WHEN** Apple contract validation runs
- **THEN** it rejects Managed User sudoers state, rendering, validation,
  non-interactive sudo verification, legacy ownership evidence, cleanup plan
  steps, and runtime `sudo_user` migration

#### Scenario: UI safety tests run
- **WHEN** Settings provisioning UI changes
- **THEN** focused UI or model tests cover no GUI-autologin wording, privileged
  plan preview, explicit confirmation, password redaction, ready state,
  repairable state, and conflict state
- **AND** no current UI fixture requires a legacy-sudoers status or cleanup row

### Requirement: Privileged Helper Integration Has Focused Verification
The Apple client SHALL include focused tests and contract checks for privileged
helper signing, channel isolation, current typed API behavior, fake-helper
seams, Managed User no-fallback enforcement, and absence of legacy-sudoers
operations.

#### Scenario: Channel isolation tests run
- **WHEN** helper identity or packaging behavior changes
- **THEN** focused checks verify stable and dev helpers use separate labels,
  Mach services, bundle identifiers, data roots, and app code requirements

#### Scenario: Fake helper tests run
- **WHEN** Settings, Managed Users, or terminal launch code calls the helper
  boundary
- **THEN** focused tests can use a fake helper to cover status, current diagnose,
  apply, start PTY, terminate PTY, remove integration, and request denial
  without requiring a live root helper

#### Scenario: Integration smoke runs
- **WHEN** helper-backed Managed User implementation is marked ready for review
- **THEN** validation includes a dev-channel install/status roundtrip and a
  helper-backed managed-user PTY smoke where local signing and authorization
  prerequisites are available

#### Scenario: Forbidden fallback checks run
- **WHEN** Managed User helper-backed code changes
- **THEN** contract checks reject using `do shell script ... with administrator
  privileges`, raw sudoers editing, `sudo -n -iu <target>`, legacy-sudoers
  diagnosis, or legacy cleanup as the helper-backed Managed User executor or
  readiness path
