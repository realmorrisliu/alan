## ADDED Requirements

### Requirement: Replaced shell-domain Swift logic is deleted or adapter-only
After a shell-domain area is replaced by Rust shell core, the Apple client SHALL
remove the corresponding reusable Swift domain implementation or narrow it to
adapter-only projection code.

#### Scenario: Large Swift shell file is edited after replacement
- **WHEN** a developer edits a large Swift shell model, controller, or service
  file in a replaced core-owned area
- **THEN** the edit does not add a new reusable shell-domain algorithm
- **AND** any remaining Swift code is documented or structured as adapter
  projection, platform effect execution, or platform recovery

#### Scenario: Architecture checks run
- **WHEN** architecture maintainability checks inspect shell-core replaced
  areas
- **THEN** they flag new Swift implementations of core-owned manifest,
  reducer, action, control-command, profile, or settings-domain behavior as
  architecture debt or failures according to the active gate mode

### Requirement: Shell-core authority reduces architecture debt
Each implementation slice MUST remove or narrow the replaced Swift
implementation enough for the architecture debt record to shrink or become more
precise when it makes a shell-domain area core-authoritative.

#### Scenario: Manifest authority slice lands
- **WHEN** the manifest authority slice is completed
- **THEN** Swift no longer contains a runtime manifest default, prune, or
  materialize implementation for the same portable behavior
- **AND** `clients/apple/ARCHITECTURE.md` records the resulting ownership state
