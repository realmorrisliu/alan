## ADDED Requirements

### Requirement: macOS delegates portable manifest semantics to shell core
Alan for macOS SHALL delegate portable workspace manifest semantics to the Rust
shell core after the manifest module has Rust contract tests and adapter tests.

The macOS platform layer SHALL continue to own Application Support path
selection, file reads and writes, atomic persistence, corrupt-file evidence, and
diagnostics presentation.

#### Scenario: macOS loads a workspace manifest
- **WHEN** Alan for macOS reads a workspace manifest from disk after shell core
  manifest integration
- **THEN** macOS passes the manifest bytes and platform context to the shell
  core for decode, upgrade, materialization, and pruning semantics
- **AND** macOS remains responsible for preserving corrupt evidence and choosing
  the file path used for persistence

#### Scenario: Manifest output is persisted
- **WHEN** the shell core returns an updated manifest or manifest sync hint
- **THEN** the macOS platform layer writes the result through its persistence
  store
- **AND** the shell core does not directly access the user's file system

### Requirement: Manifest compatibility is preserved during migration
Rust-backed manifest behavior SHALL preserve compatibility with existing macOS
workspace manifest JSON unless a later spec explicitly changes the manifest
schema.

#### Scenario: Existing manifest is read by Rust-backed path
- **WHEN** Alan for macOS reads a manifest written by the current Swift
  implementation
- **THEN** the Rust-backed path decodes or upgrades it according to the existing
  manifest contract
- **AND** Space, Tab, PaneSlot, ContentInstance, selection, pin/live snapshot,
  lifecycle, Terminal Profile reference, and quick terminal restore semantics
  remain intact
