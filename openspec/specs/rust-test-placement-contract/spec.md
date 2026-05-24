# rust-test-placement-contract Specification

## Purpose
Defines Rust test placement rules for inline unit tests, extracted white-box
suites, crate-level integration tests, migration policy, and behavior-boundary
coverage.

## Requirements
### Requirement: Rust test placement contracts live in OpenSpec
alan SHALL specify Rust test placement rules, extraction triggers, migration
policy, and relationship to integration tests in OpenSpec.

#### Scenario: Rust tests are added or materially edited
- **WHEN** a change adds or materially edits Rust tests
- **THEN** the author chooses inline unit tests, extracted white-box tests, or
  crate-level integration tests based on the OpenSpec placement rules
- **AND** new placement guidance is not authored as a long-form `docs/spec/`
  contract

#### Scenario: Legacy Rust test placement doc is opened
- **WHEN** `docs/spec/rust_test_placement_contract.md` is reached during
  migration
- **THEN** the file points to this OpenSpec capability as a bridge

### Requirement: Test location matches behavior boundary
alan SHALL place tests near the behavior boundary they verify, with larger or
cross-module behavior using extracted white-box or integration suites rather
than oversized inline modules.

#### Scenario: Test needs private implementation access
- **WHEN** a Rust test needs private module access but is too large for a small
  inline unit block
- **THEN** it uses an extracted white-box test file adjacent to the
  implementation module

#### Scenario: Test verifies public crate behavior
- **WHEN** a Rust test verifies cross-module or public crate behavior
- **THEN** it belongs in a crate-level integration test unless private access is
  the core reason for the test

### Requirement: Rust test placement vocabulary is stable
alan SHALL use stable Rust test placement vocabulary across OpenSpec, AGENTS.md,
review guidance, and crate documentation.

Stable terms:

- **Inline unit test**: a `#[cfg(test)] mod tests` block kept in the same file
  as the implementation it exercises.
- **Extracted white-box test file**: a test-only Rust file compiled as a child
  module of the implementation module so it can exercise private details
  without widening production visibility.
- **Integration test**: a crate-level test under `crates/<crate>/tests/`
  compiled outside the library module tree and exercised through the crate's
  external surface or process boundary.
- **Live test**: an integration test that talks to a real provider, daemon, or
  runtime environment and is normally `#[ignore]` plus explicitly opt-in.
- **Test support helper**: fixture-building or assertion helper code that
  exists only to support tests.

#### Scenario: Test placement docs classify a test
- **WHEN** docs, review comments, or OpenSpec changes classify Rust tests
- **THEN** they use inline unit test, extracted white-box test file,
  integration test, live test, and test support helper with these meanings

### Requirement: Rust test placement scope is explicit
alan SHALL apply this placement contract to Rust code under `crates/*` without
turning it into a complete coverage or CI-matrix policy.

Scope rules:

- This contract defines where Rust tests live.
- It does not define every subsystem's test strategy, coverage target,
  provider-harness policy, or full CI matrix ownership.
- Non-Rust clients such as `clients/apple/` are outside this
  contract unless a future OpenSpec owner adopts similar placement rules.
- Existing inline tests are not required to move in one cutover.
- Production APIs must not become `pub` or `pub(crate)` only to satisfy test
  placement.

#### Scenario: Non-Rust client test placement changes
- **WHEN** a change modifies test placement for TUI or Apple clients
- **THEN** it uses the relevant client/spec owner rather than treating this
  Rust crate placement contract as authoritative

### Requirement: Inline unit tests stay small and local
alan SHALL keep inline Rust unit tests only when locality improves
implementation readability and the production file remains primarily
production code.

Inline tests are appropriate only when all of the following are true:

1. Tests are short, local, and directly tied to the file's private helper logic
   or invariants.
2. Setup is lightweight and does not require large fixtures, scenario matrices,
   or long async orchestration.
3. Reading the tests next to the implementation materially improves local
   understanding.
4. The inline test block stays small enough that the implementation file remains
   primarily a production-code file rather than a mixed
   production-plus-harness file.

Typical inline candidates:

- parser and serializer edge cases
- small normalization helpers
- narrow state-transition checks
- simple regression tests for one private helper

Inline tests are not the default home for scenario suites, large async flows,
or fixture-heavy regression packs.

#### Scenario: Small parser helper is tested
- **WHEN** a short test exercises a private parser or normalization helper and
  the setup is lightweight
- **THEN** the test may remain inline beside the implementation

#### Scenario: Inline test block becomes a scenario suite
- **WHEN** an inline `#[cfg(test)] mod tests` grows into scenario matrices,
  fixture builders, or long async orchestration
- **THEN** it is extracted to a white-box test file or moved to integration
  tests based on the behavior boundary

### Requirement: Extracted white-box tests preserve private access without bloating implementation files
alan SHALL move large private-access Rust tests into extracted white-box test
files instead of widening production visibility or embedding large suites in
implementation files.

Use extracted white-box files for:

- async scenario suites against private runtime or daemon internals
- tests with reusable local fixtures or helper builders
- large regression matrices
- tests whose size makes the implementation file difficult to review
- tests that would otherwise force internal helpers to become public

Stable placement rules:

1. For a flat module file such as `foo.rs`, extracted white-box tests either
   live in an adjacent file such as `foo_tests.rs` loaded with
   `#[cfg(test)] #[path = "foo_tests.rs"] mod foo_tests;`, or trigger
   conversion to a directory-backed layout when multiple test files or helpers
   are expected.
2. For a directory-backed module such as `foo/mod.rs`, extracted white-box
   tests live in `foo/tests.rs`, or in a nested test-only tree rooted by
   `foo/tests.rs` with topic files under `foo/tests/*.rs`, loaded from
   `foo/mod.rs` under `#[cfg(test)] mod tests;`.
3. White-box support helpers used only by one module stay adjacent to that
   module's extracted tests rather than moving into production code.
4. Extracted white-box files remain test-only modules and must not be
   referenced by production code.

This tier is the default extraction target for large inline test blocks that
still need private implementation access.

#### Scenario: Flat module needs extracted private tests
- **WHEN** `foo.rs` has private-access tests that are too large to remain
  inline
- **THEN** alan uses an adjacent `foo_tests.rs` loaded with an explicit
  `#[cfg(test)]` path module or converts the module to a directory-backed
  layout when the suite needs multiple files

#### Scenario: White-box helper is test-only
- **WHEN** helper code exists only for extracted white-box tests
- **THEN** it remains in the adjacent test-only module tree and is not imported
  by production code

### Requirement: Integration tests cover public crate and process boundaries
alan SHALL use `crates/<crate>/tests/` for black-box behavior validated through
crate boundaries, process boundaries, or durable external contracts.

Integration tests are the correct location for:

- crate public API behavior
- CLI flows
- HTTP route and websocket contract behavior
- cross-module orchestration that does not need private access
- persistence, restart, and replay flows viewed from the crate boundary
- protocol contract and event-sequence validation
- smoke tests and live-provider or live-runtime harnesses

Stable placement rules:

1. Integration tests must not require widening production visibility solely to
   make the test compile.
2. Shared integration-test helpers belong under `crates/<crate>/tests/support/`
   or a similarly explicit test-only support module under `tests/`.
3. Live tests remain integration tests, normally `#[ignore]`, with explicit
   opt-in environment variables and companion docs or scripts when needed.

#### Scenario: HTTP route contract is tested
- **WHEN** a test validates daemon route, websocket, event-sequence, or protocol
  behavior through the public crate/process boundary
- **THEN** it lives under `crates/<crate>/tests/`

#### Scenario: Live provider test is added
- **WHEN** a Rust test talks to a real provider or live runtime environment
- **THEN** it is an opt-in integration test, normally ignored by default and
  documented with its required environment

### Requirement: Test placement decisions choose the narrowest useful boundary
alan SHALL require each new or materially edited Rust test to choose the
narrowest placement tier that preserves readability and test value.

Decision rules:

1. Start with inline only if the test is genuinely small and local.
2. If the test needs private access but is no longer small and local, extract it
   into a white-box test file instead of leaving it inline.
3. If the behavior can be validated from outside the module boundary, prefer a
   crate-level integration test.

The location choice is part of the design and review surface, not an
afterthought.

#### Scenario: New test needs private access
- **WHEN** a new Rust test needs private implementation access and has enough
  setup or cases to reduce implementation readability
- **THEN** the author uses extracted white-box placement rather than widening
  production visibility or embedding a large inline module

### Requirement: Rust test placement forbids visibility and support-code leaks
alan SHALL forbid placement choices that leak test-only concerns into
production APIs or production modules.

Disallowed for new code:

1. Expanding a production API to `pub` or `pub(crate)` solely so a black-box
   test under `crates/<crate>/tests/` can reach internal details.
2. Keeping large async scenario suites inline once they stop being small local
   unit tests.
3. Placing black-box contract tests inside `src/` when they do not require
   private access.
4. Moving general-purpose test support helpers into production modules when
   they exist only to support tests.

#### Scenario: Integration test cannot reach internals
- **WHEN** a black-box integration test cannot compile without exposing private
  internals
- **THEN** the author either moves the test to extracted white-box placement or
  validates through the public boundary instead of widening production
  visibility solely for the test

### Requirement: Large inline Rust test blocks are extracted when touched
alan SHALL treat substantial inline test blocks as extraction candidates when
they harm implementation readability or are materially expanded.

Mandatory extraction signals:

1. The test block has become a substantial share of the file and production
   implementation is no longer easy to scan top-to-bottom.
2. Tests introduce fixture builders, helper layers, scenario matrices, or
   multi-step async orchestration.
3. Tests are best organized by behavior topic rather than by one flat local
   `tests` module.
4. Reviewing production implementation now requires scrolling through a large
   harness section to recover context.

Very large mixed files are treated as already past the extraction threshold
even if the implementation remains correct.

#### Scenario: Change adds coverage to oversized inline block
- **WHEN** work materially edits a Rust file whose inline tests already harm
  readability or adds more scenario coverage to an oversized inline block
- **THEN** the change extracts the suite when that can be done without
  destabilizing unrelated behavior

### Requirement: Rust test migration is forward-looking and opportunistic
alan SHALL apply this contract immediately to new or materially edited Rust
tests while grandfathering existing inline tests until they are touched or
clearly harm readability.

Migration rules:

1. Existing inline tests are temporarily grandfathered.
2. Move existing tests when the surrounding implementation file is already large
   enough that tests materially harm readability.
3. Move existing tests when a touched change adds more scenario coverage to an
   already oversized inline test block.
4. Move existing tests when work is already refactoring the implementation
   module and the move can be done without destabilizing unrelated behavior.
5. The first migration wave prioritizes the largest mixed
   production-plus-test files in `alan-runtime` and `alan`.

#### Scenario: Legacy inline tests are not touched
- **WHEN** a change does not materially edit a legacy inline test block
- **THEN** the block may remain grandfathered unless it is already blocking
  readability or adjacent work makes extraction low risk

#### Scenario: Refactor already changes module layout
- **WHEN** a refactor already changes a module containing oversized inline tests
- **THEN** the refactor should migrate those tests toward extracted white-box or
  integration placement when the move can stay behavior-preserving
