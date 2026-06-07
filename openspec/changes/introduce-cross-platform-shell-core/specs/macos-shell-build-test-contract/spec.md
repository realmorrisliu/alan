## ADDED Requirements

### Requirement: Shell core migration has parity and adapter validation
Shell core migration slices SHALL include focused Rust tests, Swift-exported
parity fixtures, and Swift adapter validation before replacing macOS Swift shell
domain logic.

#### Scenario: Rust module is introduced
- **WHEN** a Rust shell core module implements existing Swift shell domain
  behavior
- **THEN** focused Rust unit tests cover its local behavior
- **AND** fixture tests compare it against Swift-exported cases for the same
  domain

#### Scenario: Swift call path is replaced
- **WHEN** a macOS Swift call path switches to Rust shell core behavior through
  a binding facade
- **THEN** Swift adapter tests cover request encoding, response decoding, error
  mapping, schema or ABI version mismatch, and fallback removal expectations
- **AND** the focused Apple script tests for the affected domain run in the same
  validation slice

### Requirement: Existing shell checks remain in the migration matrix
Shell core migration SHALL continue to run the existing focused macOS shell
script checks that cover the replaced behavior.

#### Scenario: Split and reducer behavior changes
- **WHEN** split tree or workspace reducer behavior is migrated to Rust
- **THEN** `clients/apple/scripts/test-shell-split-model.sh` and other affected
  reducer-focused checks remain part of validation until their coverage is
  replaced by stricter Rust-backed equivalents

#### Scenario: Manifest behavior changes
- **WHEN** workspace manifest behavior is migrated to Rust
- **THEN** `clients/apple/scripts/test-shell-workspace-manifest.sh` remains part
  of validation
- **AND** existing manifest compatibility cases are represented in parity
  fixtures

#### Scenario: Control or automation command behavior changes
- **WHEN** shell control command reduction or automation command seams are
  migrated to Rust
- **THEN** `clients/apple/scripts/test-shell-automation-command-seams.sh` and
  `clients/apple/scripts/check-shell-contracts.sh` remain part of validation

### Requirement: Binding generator output is pinned and checked
If a binding generator such as UniFFI is used for Swift integration, Alan SHALL
pin the generator version and validate generated Swift, header, and modulemap
output so binding drift is intentional.

#### Scenario: Binding facade is regenerated
- **WHEN** generated binding output changes
- **THEN** validation makes the generated Swift/header/modulemap drift visible
- **AND** the change records whether the drift came from schema changes,
  generator-version changes, or facade implementation changes
