## ADDED Requirements

### Requirement: Debt refactor slices tighten executable budgets
Every refactor PR in this change SHALL reduce at least one recorded Rust source,
dependency, or Apple architecture debt budget and MUST update the owning
baseline in the same PR. It MUST NOT increase another debt budget to compensate.

#### Scenario: Oversized source is split by responsibility
- **WHEN** a refactor moves a cohesive responsibility out of a baseline-listed
  Rust file
- **THEN** the original file's maximum is lowered or removed in the same PR
- **AND** every resulting Rust source remains at or below its applicable maximum

#### Scenario: Transitional dependency is removed
- **WHEN** a responsibility moves to its durable crate owner
- **THEN** the old crate dependency is removed
- **AND** the accepted dependency graph is tightened in the same PR

#### Scenario: A refactor shifts debt elsewhere
- **WHEN** a slice reduces one budget but grows another oversized source,
  transitional dependency set, or Apple warning class
- **THEN** the canonical quality gate fails

### Requirement: Rust oversized-source debt reaches zero
This change SHALL reduce every Rust source under `crates/` to no more than 1,000
lines and SHALL leave the oversized-source baseline empty. Extracted modules
MUST represent coherent owners, adapters, models, or test suites rather than
fixed line ranges.

#### Scenario: Final Rust debt validation runs
- **WHEN** all Rust refactor slices are complete
- **THEN** no Rust source under `crates/` exceeds 1,000 lines
- **AND** the checked-in oversized-source baseline has no entries

#### Scenario: A cosmetic split is proposed
- **WHEN** an extracted module has no named responsibility or narrower review
  boundary
- **THEN** the slice is not considered complete even if line counts decrease
