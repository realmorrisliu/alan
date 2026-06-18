## MODIFIED Requirements

### Requirement: Managed Terminal Account Provisioning Has Focused Verification
Alan for macOS SHALL require focused verification for Managed Terminal Account
planning, helper diagnosis, helper-backed account repair, helper-owned PTY
spawn verification, legacy sudoers cleanup, rollback, and UI safety wording.

#### Scenario: Dry-run planner tests run
- **WHEN** provisioning planning behavior changes
- **THEN** focused tests cover missing account, existing Alan-managed account,
  existing non-Alan account, missing home, invalid shell, legacy Alan sudoers
  state, missing Terminal Profile, and already-ready account states

#### Scenario: Helper diagnosis tests run
- **WHEN** helper-backed diagnosis behavior changes
- **THEN** focused tests cover helper unavailable, helper outdated, invalid
  helper signature, account not Alan managed, account repairable, legacy sudoers
  cleanup available, and ready account states

#### Scenario: Helper apply tests run
- **WHEN** helper-backed repair behavior changes
- **THEN** focused tests cover typed plan validation, identifier rejection,
  home path derivation, shell allowlist enforcement, no reusable credentials,
  and rejection of raw shell, arbitrary executable, or raw sudoers requests

#### Scenario: Managed-user PTY verification tests run
- **WHEN** managed-user terminal readiness or launch behavior changes
- **THEN** focused tests cover helper-owned PTY spawn success, PTY spawn
  failure, helper rejection, child exit, signal/terminate routing, and no
  `sudo_user` fallback for `managed_user` profiles

#### Scenario: Rollback tests run
- **WHEN** rollback behavior changes
- **THEN** focused tests cover removal of Alan-owned helper/Profile integration
  and verified legacy Alan sudoers cleanup
- **AND** tests confirm account and home-directory deletion require a separate
  destructive confirmation
- **AND** tests confirm non-Alan sudoers files and ordinary macOS accounts are
  preserved

#### Scenario: UI safety tests run
- **WHEN** Settings provisioning UI changes
- **THEN** focused UI or model tests cover helper install/update/invalid
  states, no GUI-autologin wording, privileged plan preview, explicit
  confirmation, password redaction, ready state, repairable state, and account
  not Alan managed state

## ADDED Requirements

### Requirement: Privileged Helper Integration Has Focused Verification
The Apple client SHALL include focused tests and contract checks for privileged
helper signing, channel isolation, typed API behavior, fake-helper seams, and
Managed User no-fallback enforcement.

#### Scenario: Channel isolation tests run
- **WHEN** helper identity or packaging behavior changes
- **THEN** focused checks verify stable and dev helpers use separate labels,
  Mach services, bundle identifiers, data roots, and app code requirements

#### Scenario: Fake helper tests run
- **WHEN** Settings, Managed Users, or terminal launch code calls the helper
  boundary
- **THEN** focused tests can use a fake helper to cover status, diagnose,
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
  privileges`, raw sudoers editing, or `sudo -n -iu <target>` as the
  helper-backed Managed User executor or readiness path
