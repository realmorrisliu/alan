## ADDED Requirements

### Requirement: Validation rejects replaced Swift domain fallbacks
Alan's shell validation scripts SHALL reject runtime Swift fallback patterns
that recompute core-owned shell-domain behavior after shell core has a tested
authority path.

#### Scenario: Manifest fallback is reintroduced
- **WHEN** a macOS runtime path calls shell core for manifest defaulting,
  pruning, migration, or materialization
- **AND** the same expression or error path falls back to a Swift manifest
  domain implementation
- **THEN** shell contract validation fails with a message identifying the
  forbidden fallback

#### Scenario: Reducer fallback is reintroduced
- **WHEN** a replaced reducer or control-command path handles a shell-core
  failure by applying a Swift mutation algorithm for the same operation
- **THEN** shell contract or architecture validation fails

#### Scenario: Legitimate platform fallback remains
- **WHEN** Swift code contains fallback behavior for UI labels, Ghostty runtime
  initialization, corrupt-file quarantine, pasteboard input, or diagnostics
  presentation
- **THEN** validation allows that platform fallback when it is not a duplicate
  shell-domain implementation

### Requirement: Adapter tests replace Swift domain oracle tests
For core-owned behavior, Swift tests SHALL verify shell-core adapter behavior
and platform integration rather than preserving the removed Swift implementation
as a runtime oracle.

#### Scenario: Swift manifest materializer test is updated
- **WHEN** the Swift manifest materializer runtime implementation is removed
- **THEN** remaining Swift tests cover adapter encode/decode, error handling,
  corrupt-file recovery, and app startup integration
- **AND** portable manifest default/prune/materialize assertions live in Rust
  shell-core tests or facade fixture tests

#### Scenario: Control command adapter test is updated
- **WHEN** portable control command behavior moves fully behind shell core
- **THEN** Swift tests verify that returned core results and side effects are
  applied correctly
- **AND** portable command validation and stable error assertions live in Rust
  shell-core tests or facade fixture tests
