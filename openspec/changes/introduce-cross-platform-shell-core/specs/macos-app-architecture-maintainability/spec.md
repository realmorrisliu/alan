## ADDED Requirements

### Requirement: Reusable shell domain logic migrates to Rust shell core
The Apple client architecture SHALL treat reusable shell workspace domain logic
as Rust shell core ownership once the corresponding shell core module and parity
fixtures exist.

Swift files in the Apple client SHALL remain platform adapters, presentation
layers, terminal runtime hosts, and compatibility wrappers rather than the
stable home for reusable workspace reducer, manifest, action, control-command,
or Terminal Profile domain semantics.

#### Scenario: New reusable shell behavior is added
- **WHEN** a developer adds behavior that changes platform-neutral shell
  workspace semantics after the shell core module for that domain exists
- **THEN** the behavior is implemented in the Rust shell core
- **AND** the Apple client consumes it through a platform adapter rather than
  adding a separate Swift implementation

#### Scenario: Swift logic is replaced
- **WHEN** a Swift shell domain module is replaced by Rust shell core behavior
- **THEN** the replaced Swift implementation is removed or narrowed to adapter
  code
- **AND** `clients/apple/ARCHITECTURE.md` and architecture validation
  expectations are updated when warning debt decreases

### Requirement: Architecture debt burn-down follows shell core adoption
Apple client architecture warning debt SHALL decrease as Swift shell domain
logic is replaced by Rust shell core modules.

#### Scenario: Rust-backed module lands
- **WHEN** a Rust-backed shell core module replaces a Swift reducer, manifest,
  action, control, profile, or settings domain implementation
- **THEN** the implementation slice records which architecture warning class was
  reduced or explains why the replacement is an intermediate adapter-only step
- **AND** new pure domain logic is not added to the large Swift files that the
  slice is meant to retire
