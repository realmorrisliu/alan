## ADDED Requirements

### Requirement: Repository quality has one canonical interface
Alan SHALL provide one non-mutating repository quality command used by local
development, repository Git hooks, and CI. The command MUST fail when any owned
clean-code or clean-architecture check fails.

#### Scenario: Developer runs the quality gate
- **WHEN** a developer invokes the canonical repository quality command
- **THEN** it checks formatting without rewriting files
- **AND** it runs the curated Rust, source-hygiene, dependency, Alan OS absence,
  and Apple architecture checks

#### Scenario: Gate composition changes
- **WHEN** a required clean-code or clean-architecture check is added or removed
- **THEN** the canonical command changes once
- **AND** local hooks and CI consume the updated interface without duplicating
  its internal command list

#### Scenario: Ambient Cargo output configuration differs
- **WHEN** a developer or CI environment configures another Cargo target
  directory or build target
- **THEN** the quality gate builds into its owned Host-target directory
- **AND** binary-surface guards inspect the executable produced by that run

### Requirement: Rust lint semantics are reproducible
Alan SHALL pin one explicit Rust toolchain including rustfmt and Clippy. Local
quality checks and CI MUST use that pinned version.

#### Scenario: Local and CI lint the same commit
- **WHEN** the same commit is checked locally and in CI
- **THEN** rustc, rustfmt, and Clippy use the same pinned release
- **AND** floating `stable` does not select different lint semantics

#### Scenario: Toolchain is upgraded
- **WHEN** Alan adopts a newer Rust release
- **THEN** the pin and CI installation change together
- **AND** the complete quality gate passes before the upgrade merges

### Requirement: Clippy policy is curated and zero-baseline
Alan SHALL deny default compiler and Clippy warnings for all workspace targets
and features. Optional lints MUST be promoted individually only when their
current baseline is zero and they provide demonstrated correctness,
maintainability, or safety signal.

The initial curated policy SHALL require documented unsafe blocks, reject
redundant clones, reject `dbg!`, `todo!`, and `unimplemented!` in production
targets, and require reasons for explicit source lint suppressions. It MUST NOT
blanket-deny the full `pedantic`, `nursery`, `restriction`, or `cargo` groups.

#### Scenario: New curated finding is introduced
- **WHEN** a commit introduces a curated Clippy finding or an unexplained lint
  suppression
- **THEN** the canonical quality gate fails

#### Scenario: Test double uses an intentionally incomplete operation
- **WHEN** test-only code requires an explicit incomplete-operation stub
- **THEN** the production-target restriction does not misclassify it as shipped
  behavior
- **AND** the all-target default warning gate still applies

#### Scenario: Optional lint is proposed
- **WHEN** a maintainer proposes enabling another optional lint
- **THEN** the change records its signal and reaches a zero current baseline
  before making the lint fatal

### Requirement: Rust source debt ratchets downward
New Rust source files under `crates/` SHALL contain no more than 1,000 lines.
Existing files above that ceiling MUST have exact checked-in maximums, MUST NOT
grow, and MUST lower or remove their maximum whenever they shrink.

Line count is an AI-navigability and review ceiling; it MUST NOT be treated as
proof of module depth or as permission for cosmetic file splitting.

#### Scenario: New Rust file exceeds the ceiling
- **WHEN** a new Rust source file contains more than 1,000 lines
- **THEN** the quality gate fails

#### Scenario: Oversized existing file grows
- **WHEN** a baseline-listed Rust source file exceeds its recorded maximum
- **THEN** the quality gate fails

#### Scenario: Oversized existing file shrinks
- **WHEN** a baseline-listed Rust source file becomes shorter
- **THEN** the same change lowers its recorded maximum or removes the entry when
  the file reaches 1,000 lines or fewer
- **AND** later growth back to the old size fails

#### Scenario: Source and debt ledger grow together
- **WHEN** an existing oversized Rust file and its recorded maximum are both
  increased in one change
- **THEN** comparison with the pre-change ledger reports the debt growth
- **AND** the quality gate fails

#### Scenario: New source-size debt is added
- **WHEN** a file that was not in the pre-change ledger grows beyond 1,000 lines
  and is added to the current ledger
- **THEN** the new debt entry is rejected
- **AND** the quality gate fails

### Requirement: Accepted dependency laws have one complete test surface
Alan SHALL enforce accepted normal-dependency laws for Alan crates through one
repository architecture module over Cargo's graph. The module SHALL cover
Alan Kernel, File-Server Service crates, clients, adapters, composition owners,
and explicitly recorded transitional edges.

Duplicated per-crate manifest parsers MUST NOT be the durable architecture test
surface. Recorded transitional edges MAY shrink but MUST NOT expand without an
OpenSpec or ADR update that changes the accepted ownership model.

#### Scenario: Forbidden dependency edge is added
- **WHEN** an Alan crate adds a normal dependency outside its accepted set
- **THEN** the architecture gate reports the package, expected edges, and actual
  edges
- **AND** the quality gate fails

#### Scenario: Transitional dependency is removed
- **WHEN** a refactor removes an accepted transitional dependency edge
- **THEN** the architecture inventory is tightened in the same change
- **AND** later reintroduction fails

#### Scenario: Development-only dependency supports a contract test
- **WHEN** a crate uses another Alan crate only as a dev-dependency for a public
  contract test
- **THEN** the normal-dependency law does not misclassify that test adapter as a
  production ownership edge

#### Scenario: Workspace crate is added
- **WHEN** a package is added to the Cargo workspace without a recorded
  dependency expectation
- **THEN** the architecture gate reports that the workspace inventory is not
  fully covered
- **AND** the quality gate fails

#### Scenario: Optional production dependency violates ownership
- **WHEN** a forbidden normal dependency is activated only by a non-default
  feature or target-specific declaration
- **THEN** the architecture gate includes that edge in its comparison
- **AND** the quality gate fails

### Requirement: Git hook and CI enforce the same gate
Alan SHALL provide a versioned pre-commit hook and an explicit installer for
the checkout. The hook SHALL check staged whitespace and invoke the canonical
quality interface. Pull requests and protected branches SHALL run the same
interface as a required CI check.

#### Scenario: Developer installs repository hooks
- **WHEN** a developer runs the supported hook installer
- **THEN** the checkout uses the versioned repository hook path
- **AND** later hook changes arrive through normal source updates

#### Scenario: Local hook is bypassed
- **WHEN** a commit is created with `--no-verify` or without installed hooks
- **THEN** required CI still runs the canonical quality gate
- **AND** a failing commit cannot merge through the protected branch gate

#### Scenario: Commit fails quality validation
- **WHEN** staged code violates a quality rule
- **THEN** the pre-commit hook exits non-zero with the owning check's diagnostic

#### Scenario: Unstaged content differs from the commit
- **WHEN** the working tree contains unstaged changes or untracked files
- **THEN** the pre-commit hook validates an index-only snapshot
- **AND** unstaged content cannot make invalid staged code pass

### Requirement: Quality enforcement precedes debt burn-down
The enforcement change SHALL create a separate
`refactor-clean-code-architecture-debt` change, and its first implementation PR
SHALL immediately follow the gate PR before unrelated feature work resumes.

#### Scenario: Gate PR merges
- **WHEN** `enforce-clean-code-architecture-gates` is merged
- **THEN** the next implementation PR begins the recorded source and
  architecture debt burn-down
- **AND** the burn-down proceeds through focused behavior-preserving slices
