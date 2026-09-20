# Spec Delta

## MODIFIED Requirements

### Requirement: Repository quality has one canonical interface
Alan SHALL provide one non-mutating repository quality command used by local
development, repository Git hooks, and CI. The command MUST fail when any owned
clean-code, clean-architecture, or standalone-distribution check fails.

#### Scenario: Developer runs the quality gate
- **WHEN** a developer invokes the canonical repository quality command
- **THEN** it checks formatting without rewriting files
- **AND** it runs the curated Rust, source-hygiene, dependency, Alan OS absence,
  OpenSpec, and standalone CLI/Host distribution checks

#### Scenario: Gate composition changes
- **WHEN** a required clean-code, clean-architecture, or distribution check is
  added or removed
- **THEN** the canonical command changes once
- **AND** local hooks and CI consume the updated interface without duplicating
  its internal command list

#### Scenario: Ambient Cargo output configuration differs
- **WHEN** a developer or CI environment configures another Cargo target
  directory or build target
- **THEN** the quality gate builds into its owned Host-target directory
- **AND** binary-surface guards inspect the executable produced by that run

#### Scenario: Dependency manifest and lockfile diverge
- **WHEN** a dependency manifest requires a Cargo.lock update that is not part
  of the change
- **THEN** dependency-resolving quality commands run in locked mode and fail
- **AND** the quality gate leaves Cargo.lock byte-for-byte unchanged
