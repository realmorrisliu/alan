## Context

The current workspace passes Rust 1.97 default Clippy with warnings denied, but
the local checkout was using Rust 1.93 while CI selected floating `stable`.
`just check`, CI jobs, architecture tests, Apple validation, and absence guards
also expose different validation interfaces. The repository has 64 Rust files
over 1,000 lines and 15 accepted Apple architecture warnings, so switching
directly to zero-debt thresholds would block all delivery behind one broad
refactor.

Clean architecture is not a Clippy lint. ADR-0025 dependency laws, Alan OS
absence guards, and the Apple ownership report are the executable architecture
test surface. Clippy remains responsible for compiler-visible correctness and a
small set of high-signal code findings.

## Goals / Non-Goals

**Goals:**

- Give developers, Git hooks, and CI one non-mutating quality interface.
- Make Rust lint semantics reproducible with an explicit toolchain.
- Deny all default warnings plus curated high-signal Clippy findings.
- Concentrate accepted dependency laws in one repository architecture module.
- Prevent existing source-size and Apple architecture debt from increasing.
- Record the debt burn-down as the immediately following change and PR.

**Non-Goals:**

- Enable whole `pedantic`, `nursery`, `restriction`, or `cargo` groups.
- Claim that file length, function length, or cognitive complexity proves
  module depth.
- Split the 64 oversized Rust files or eliminate the 15 Apple warnings in this
  gate PR.
- Change an Alan OS runtime or product interface.

## Decisions

### Decision: One repository quality interface

A repository script is the implementation module. `just`, the pre-commit hook,
and CI call that same interface without copying its command list. The interface
runs rustfmt in check mode, Rust quality checks, dependency and source-hygiene
guards, current Alan OS absence guards, and the Apple architecture report.
It resolves the pinned toolchain's Host target, forces Cargo artifacts into a
repository-owned quality target directory, and passes that exact freshly built
`alan` executable to binary-surface guards. Ambient Cargo target-directory or
build-target configuration therefore cannot make a guard inspect a stale file.

`just check` becomes non-mutating and delegates to the quality interface before
tests. CI keeps behavioral test/build/harness jobs separate but makes the
quality job required for pull requests and protected branches.

Alternative considered: keep independent fmt, Clippy, and architecture jobs.
Rejected because the local hook and CI would continue to have different test
surfaces and command drift.

### Decision: Pin Rust 1.97.0

`rust-toolchain.toml` owns the Rust, rustfmt, and Clippy version. CI installs the
same version rather than floating `stable`. Dependency updates remain separate
from compiler/lint-semantic updates.

Alternative considered: retain floating stable. Rejected because the same
source already passed locally on 1.93 while prior CI exposed newer 1.97 lints.

### Decision: Curate Clippy rather than deny broad groups

All targets and features keep default Clippy warnings fatal. The gate adds
`undocumented_unsafe_blocks` and `redundant_clone` across all targets, and bans
`dbg!`, `todo!`, and `unimplemented!` in production targets. Explicit source
lint suppressions require a reason.

Broad `pedantic`, `nursery`, `restriction`, and `cargo` groups remain disabled;
the audit produced roughly two thousand findings dominated by documentation
format, `must_use`, package metadata, casts, and subjective style. Individual
lints may be promoted later only after the current baseline is zero and the
signal is demonstrated.

Alternative considered: deny all optional groups. Rejected because lint volume
would reward mechanical churn and shallow modules without enforcing Alan OS
ownership.

### Decision: Use built-in Cargo graph output for dependency laws

One repository architecture script checks exact normal Alan-crate dependency
edges with `cargo tree` across all features and target-specific declarations.
It also compares its recorded package inventory with the complete Cargo
workspace so a newly added crate cannot bypass ownership review. It replaces
duplicated manifest parsers and covers Kernel, File-Server Service crates,
clients, adapters, and current transitional owners. Current transitional edges
are a ratcheting ceiling: they may shrink, and any expansion requires an
explicit ADR/OpenSpec update.

Source-token guards that encode accepted absence rules remain in their owning
scripts or focused tests; the graph module does not become a source-code parser.

Alternative considered: add a Cargo metadata parsing dependency. Rejected
because Cargo already exposes the direct graph needed by this gate.

### Decision: Ratchet debt instead of grandfathering it silently

New Rust source files may not exceed 1,000 lines. Each existing oversized Rust
file has an exact maximum in a checked-in baseline. The current tree must match
that baseline, and the baseline is compared with the pre-change Git reference:
new debt entries and increased limits fail even when source and ledger are
changed together. Reduction also requires lowering or removing the baseline
entry, preventing later regrowth.

The existing Apple report remains authoritative for its 15 warning instances.
A structured ledger records each stable warning key and the exact line ceiling
for every large file. The report must exactly match that ledger, and the ledger
is compared with the pre-change Git reference. A new warning or larger line
ceiling therefore fails even when code and ledger are raised together; every
reduction tightens the ledger and documented count in the same change.

Alternative considered: immediately require zero oversized files and strict
Apple mode. Rejected because it would combine a large behavior-preserving
refactor with the enforcement change and make review unsafe.

### Decision: CI is the hard gate; Git hook is fast feedback

The repository owns `.githooks/pre-commit` and an explicit installer sets
`core.hooksPath` for the checkout. The hook checks staged whitespace and runs
the canonical quality interface. If the working tree differs from the index,
the hook materializes an index-only snapshot so unstaged fixes or untracked
files cannot make invalid staged code pass. It documents that `--no-verify` can
bypass a local hook; protected-branch required CI is the non-bypassable merge
condition.

Alternative considered: copy a hook directly into `.git/hooks`. Rejected
because Git does not version that directory and updates would drift.

### Decision: Debt burn-down is the immediate next delivery

This PR creates the separate `refactor-clean-code-architecture-debt` OpenSpec
change. Its first implementation PR starts the ratchet burn-down before
unrelated feature work, then continues through focused, behavior-preserving
slices rather than one repository-wide rewrite.

## Risks / Trade-offs

- [First hook run is slow] → Reuse Cargo's incremental cache; keep behavioral
  tests outside pre-commit while CI runs them.
- [Pinned Rust ages] → Upgrade it in an explicit maintenance PR with the same
  quality gate green before and after.
- [Line count can incentivize cosmetic splits] → Treat it only as an
  AI-navigability ceiling; architecture review still uses module depth,
  interface, seam, locality, and leverage.
- [Baseline files can be edited to hide debt] → Compare current ledgers with the
  pre-change Git reference so new entries and budget increases fail even when
  source and ledger change together.
- [Apple warnings remain non-zero] → The immediate follow-up change owns their
  reduction; this PR forbids silent growth.

## Migration Plan

1. Add the pinned toolchain and make the current curated lint baseline clean.
2. Add source-size and dependency graph checks, then replace duplicated graph
   parsers.
3. Add the canonical quality interface and route `just` and CI through it.
4. Add and install the repository pre-commit hook.
5. Create and validate the immediate debt burn-down change.
6. Run the complete gate, workspace tests, and strict OpenSpec validation.

Rollback removes the hook path configuration and reverts the gate commit; it
does not require runtime data migration.

## Open Questions

None for this enforcement slice. Debt split order belongs to
`refactor-clean-code-architecture-debt`.
