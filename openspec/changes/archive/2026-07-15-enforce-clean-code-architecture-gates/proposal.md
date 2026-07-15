## Why

Alan currently has passing rustfmt, default Clippy, tests, and several focused
architecture checks, but they do not share one local/CI interface, use the same
Rust toolchain, or cover the complete accepted ownership model. This lets a
commit pass locally while failing newer CI lint semantics, and lets source-size
or architecture debt grow without a mechanical no-regression rule.

## What Changes

- Add one repository quality gate used unchanged by `just`, the repository Git
  hook, and required CI.
- Pin Rust, rustfmt, and Clippy to one explicit toolchain version.
- Keep `clippy::all` warnings fatal and add only high-signal curated lints;
  reject blanket `pedantic`, `nursery`, `restriction`, and `cargo` groups.
- Make formatting checks non-mutating and require suppression reasons.
- Enforce ADR-0025 dependency laws and existing Alan OS absence guards through
  the same quality gate, with accepted dependency edges compared against the
  pre-change ledger so implementation and allowance cannot expand together.
- Ratchet oversized Rust source files and existing Apple architecture warnings:
  new debt fails, existing debt cannot grow, and every reduction lowers the
  recorded ceiling.
- Add a repository-owned Git hook installer and pre-commit hook for fast local
  enforcement while retaining CI as the non-bypassable merge gate.

## Capabilities

### New Capabilities

- `repository-quality-gate`: Defines the canonical clean-code and
  clean-architecture validation interface, pinned tooling, curated lint policy,
  dependency and source-debt ratchets, Git hook behavior, and CI enforcement.

### Modified Capabilities

- `macos-app-architecture-maintainability`: Requires the repository quality
  gate to run the Apple architecture report and reject warning-count growth.

## Impact

- Affects `Justfile`, Rust/Clippy configuration, CI workflows, repository
  scripts, Git hook setup, selected lint sites, and Apple architecture
  validation.
- Adds no runtime dependency and changes no Alan OS product interface.
- Existing oversized files and the 15 recorded Apple warnings remain explicit
  migration debt; this change prevents growth but does not mix their broad
  refactors into the gate change.
- The immediately following implementation PR SHALL begin the separately
  tracked `refactor-clean-code-architecture-debt` burn-down before unrelated
  feature work resumes.
