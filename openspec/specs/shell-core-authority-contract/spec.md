# shell-core-authority-contract Specification

## Purpose
TBD - created by archiving change make-shell-core-authoritative. Update Purpose after archive.
## Requirements
### Requirement: Shell core is authoritative for portable shell domain behavior
Alan SHALL treat Rust shell core as the authority for portable shell-domain
behavior once a domain area has a shell-core implementation and adapter tests.

Portable shell-domain behavior includes workspace manifest defaulting,
migration, pruning, and materialization; workspace reducer mutations; shared
action registry descriptors, availability, shortcuts, and effects; portable
control-command validation and response projection; Terminal Profile validation
and launch-intent resolution; and reusable settings-domain summaries.

#### Scenario: macOS calls a core-owned domain operation
- **WHEN** macOS needs a core-owned shell-domain result
- **THEN** macOS obtains that result from Rust shell core through the versioned facade
- **AND** macOS does not recompute the same domain result through Swift model,
  controller, or service logic

#### Scenario: Future platform implements the shell
- **WHEN** a future Linux client needs the same workspace mutation, manifest, or
  action behavior as macOS
- **THEN** the client reuses Rust shell core semantics through a platform adapter
- **AND** it does not implement a separate platform-native copy of the domain
  algorithm

### Requirement: Core-owned operations fail closed instead of using duplicate domain fallback
Platform adapters SHALL NOT silently fall back to platform-native copies of
core-owned domain behavior when the shell-core facade fails, returns an
unsupported schema, rejects a payload, or reports an internal error.

#### Scenario: Facade cannot load or decode
- **WHEN** Swift cannot load shell-core FFI, detects an ABI/schema mismatch, or
  cannot decode the core response for a core-owned operation
- **THEN** the platform adapter reports an explicit diagnostic or stable failure
- **AND** it does not run a Swift implementation of the same domain operation

#### Scenario: Core rejects a reducer operation
- **WHEN** shell core rejects a workspace reducer operation with a stable error
- **THEN** the platform response is derived from that core error
- **AND** Swift does not infer a different result by applying an alternate
  mutation path

### Requirement: Swift code is classified as adapter, platform effect, or temporary fixture
After a shell-domain area becomes core-owned, Swift SHALL classify and limit
remaining code in that area to adapter projection, platform effect execution,
platform recovery, or temporary parity fixtures with explicit removal tasks.

#### Scenario: Swift code maps core state to app models
- **WHEN** Swift maps a core response into current UI or runtime projection types
- **THEN** the mapping may preserve platform-only fields and invoke app
  notifications
- **AND** it does not decide shell-domain validity, focus, layout, lifecycle,
  action availability, or stable command errors independently from core

#### Scenario: Swift fixture code remains during migration
- **WHEN** a Swift implementation remains only to generate or compare parity
  fixtures
- **THEN** it is not used by normal app runtime paths
- **AND** the owning task list names when it will be removed or downgraded

### Requirement: Platform adapters continue to own OS effects and recovery
Shell-core authority SHALL NOT move platform UI, terminal runtime, OS effect, or
file-system recovery behavior into Rust shell core.

#### Scenario: Core emits a terminal runtime intent
- **WHEN** shell core returns an intent to start, close, focus, send input to, or
  capture state from terminal content
- **THEN** the platform adapter executes that effect with platform runtime
  facilities
- **AND** shell core receives only portable metadata or outcome data in return

#### Scenario: Manifest file is corrupt
- **WHEN** the macOS manifest file cannot be decoded from disk
- **THEN** Swift preserves corrupt-file evidence and owns the filesystem
  quarantine/write flow
- **AND** portable default manifest semantics still come from shell core

### Requirement: Core-owned behavior is tested at the core interface
Tests for portable shell-domain behavior SHALL exercise the Rust shell-core
interface or facade, while Swift tests for replaced areas SHALL focus on adapter
encoding, decoding, error mapping, platform effects, and app integration.

#### Scenario: Domain branch is removed from Swift
- **WHEN** a Swift branch implementing core-owned domain behavior is removed
- **THEN** equivalent Rust shell-core tests or fixture tests cover the behavior
- **AND** Swift tests do not require the removed domain implementation to remain
  available as a runtime fallback

