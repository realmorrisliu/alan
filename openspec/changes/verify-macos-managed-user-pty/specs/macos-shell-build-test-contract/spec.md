## MODIFIED Requirements

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

#### Scenario: Integration smoke prerequisites are operator-deferred
- **WHEN** signing is available but the operator defers selection or provisioning
  of an Alan-managed local account
- **THEN** validation records the live Managed User PTY smoke as not run rather
  than passed
- **AND** an active OpenSpec verification change tracks the deferred smoke until
  sanitized pass or failure evidence is recorded
- **AND** Alan does not infer ownership of or automatically adopt an unmarked
  existing account to satisfy the smoke prerequisite

#### Scenario: Forbidden fallback checks run
- **WHEN** Managed User helper-backed code changes
- **THEN** contract checks reject using `do shell script ... with administrator
  privileges`, raw sudoers editing, `sudo -n -iu <target>`, legacy-sudoers
  diagnosis, or legacy cleanup as the helper-backed Managed User executor or
  readiness path
