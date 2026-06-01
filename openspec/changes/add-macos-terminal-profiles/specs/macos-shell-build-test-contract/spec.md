## ADDED Requirements

### Requirement: Terminal Profile Changes Have Focused Verification
Alan for macOS SHALL require focused verification for changes to Terminal
Profile storage, profile-aware workspace persistence, launch resolution,
inheritance, and UI before those changes are accepted.

#### Scenario: Profile store tests run
- **WHEN** Terminal Profile store behavior changes
- **THEN** focused tests cover missing-store fallback, corrupt-store quarantine,
  profile validation, default profile selection, and profile lookup

#### Scenario: Workspace manifest tests cover profile references
- **WHEN** workspace manifest fields for Terminal Profile references change
- **THEN** focused manifest tests cover old-manifest compatibility, Space
  profile references, terminal content profile references, missing local
  profile references, and the rule that profile definitions are not embedded

#### Scenario: Boot resolution tests cover launch modes
- **WHEN** terminal boot-profile resolution changes for Terminal Profiles
- **THEN** focused terminal runtime tests cover login shell, sudo Unix user,
  sudo root, custom command, missing profile fallback, and non-secret
  environment projection

#### Scenario: Interaction tests cover inheritance
- **WHEN** shell mutations or action routing change for Terminal Profiles
- **THEN** focused shell action or automation tests cover Space profile binding,
  new tab inheritance, split inheritance, explicit profile override, and
  non-retroactive Space binding changes

#### Scenario: UI smoke covers dev-channel restore
- **WHEN** Terminal Profile Settings or sidebar affordances change
- **THEN** validation includes a dev-channel fresh relaunch smoke path that
  confirms profile references survive restart and missing-profile fallback is
  visible without blocking the shell
