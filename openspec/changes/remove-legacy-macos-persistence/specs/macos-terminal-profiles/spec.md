## MODIFIED Requirements

### Requirement: Sudo Configuration Remains Operator Managed
Alan for macOS SHALL NOT edit sudoers files, create Unix users, or silently
configure passwordless sudo as part of general Terminal Profile management.
Manually authored `sudo_user` and `sudo_root` profiles SHALL remain
operator-managed. Managed User account provisioning SHALL route through the
signed helper and SHALL NOT inspect or clean legacy sudoers state through the
Terminal Profile editor or Managed Users steady-state UI.

#### Scenario: Sudo requires password
- **WHEN** an operator-authored `sudo_user` or `sudo_root` Terminal Profile
  launches and sudo asks for a password
- **THEN** the prompt appears inside the terminal session
- **AND** Alan does not intercept or store the password

#### Scenario: Passwordless sudo is configured externally
- **WHEN** the operator has configured passwordless sudo for the requested Unix
  user outside Alan
- **THEN** the operator-authored `sudo_user` Terminal Profile launches without
  additional Alan-specific configuration

#### Scenario: Managed user profile is edited
- **WHEN** the user inspects a Terminal Profile owned by a Managed User
- **THEN** Alan presents the profile as read-only current helper-managed state
- **AND** account repair and helper readiness remain in the Managed Users
  surface
- **AND** neither surface offers legacy sudoers discovery or cleanup

### Requirement: Managed User Profiles Use Helper Launch Identity
Alan for macOS SHALL represent helper-backed Managed User Terminal Profiles
with `managed_user` launch identity rather than `sudo_user`. It SHALL NOT
migrate an old Managed-User-owned `sudo_user` profile at load time.

#### Scenario: Current managed profile is loaded
- **WHEN** a Managed-User-owned profile uses `managed_user` and references a
  current helper-owned account
- **THEN** Alan resolves it as a helper-backed Managed User launch identity

#### Scenario: Retired managed sudo profile is loaded
- **WHEN** a profile marked as Managed-User-owned uses `sudo_user`
- **THEN** Alan treats it as unsupported or not ready for helper-backed launch
- **AND** Alan does not rewrite it into `managed_user` through a compatibility
  migration

#### Scenario: Manual sudo profile is preserved
- **WHEN** a non-managed Terminal Profile uses `sudo_user`
- **THEN** Alan preserves the manual profile as operator-managed startup state
- **AND** Alan does not convert it to `managed_user` without current Managed
  User ownership evidence
