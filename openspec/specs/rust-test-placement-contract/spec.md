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

### Requirement: Rust test placement vocabulary uses current runtime owners

Alan SHALL use stable Rust test placement vocabulary across OpenSpec, AGENTS.md, review guidance, and crate documentation.

- **Inline unit test**: a small `#[cfg(test)] mod tests` block in the implementation file.
- **Extracted white-box test file**: a test-only child module that exercises private details without widening production visibility.
- **Integration test**: a crate-level test under `crates/<crate>/tests/` that exercises an external crate, CLI process, or durable file boundary.
- **Live test**: an opt-in integration test against a real provider, Process host, mounted namespace, or other live surviving environment.
- **Test support helper**: fixture or assertion code compiled only for tests.

#### Scenario: Test placement is classified

- **WHEN** current docs, review comments, or OpenSpec changes classify a Rust test
- **THEN** they use these terms with the stated meanings
- **AND** they do not preserve a removed transport as a standard test category

### Requirement: Extracted white-box tests preserve private access for current owners

Alan SHALL place large private-access suites in extracted white-box files adjacent to the implementation owner rather than widening production visibility or bloating implementation files.

#### Scenario: Private Agent Engine suite grows

- **WHEN** an Agent Engine, Process, AgentFS, policy, provider, Tool, or renderer suite needs private access and substantial fixtures or async orchestration
- **THEN** it moves to an adjacent test-only module tree
- **AND** its helpers are not imported by production code

#### Scenario: Flat module needs extracted tests

- **WHEN** `foo.rs` has a large private-access suite
- **THEN** it loads an adjacent `foo_tests.rs` under `#[cfg(test)]` or converts to a directory-backed module layout

### Requirement: Integration tests cover public crate, CLI, Process, and file boundaries

Alan SHALL use `crates/<crate>/tests/` for black-box behavior validated through public crate APIs, CLI processes, Process and AgentFS boundaries, persistence records, or live provider/runtime harnesses.

#### Scenario: AgentFS contract is tested

- **WHEN** a test validates public Process launch, AgentFS IO, request, action, machine, offset, or control behavior
- **THEN** it exercises the public crate or process/file boundary from a crate-level integration test
- **AND** it does not require private production visibility

#### Scenario: Live provider test is added

- **WHEN** a Rust test talks to a real provider or live Process environment
- **THEN** it is an explicitly opt-in integration test, normally ignored by default
- **AND** its required environment is documented
