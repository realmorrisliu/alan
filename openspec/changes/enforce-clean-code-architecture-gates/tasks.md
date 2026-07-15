## 1. Reproducible Rust Quality

- [x] 1.1 Pin Rust 1.97.0 with rustfmt and Clippy for local and CI use.
- [x] 1.2 Add the curated all-target and production-target Clippy lanes with
  warnings denied, without enabling broad optional lint groups.
- [x] 1.3 Resolve current redundant-clone findings, document unsafe blocks, and
  give every explicit source lint suppression a reason.
- [x] 1.4 Keep rustfmt and rustdoc validation non-mutating and fatal on warnings.

## 2. Source And Architecture Ratchets

- [x] 2.1 Record exact maxima for the 64 Rust source files currently over 1,000
  lines and add a guard for new, growing, reduced, and removed files.
- [x] 2.2 Add one Cargo-graph architecture module covering accepted normal Alan
  crate dependencies and current ratcheting transitional edges.
- [x] 2.3 Remove duplicated per-crate manifest parsers after the graph module
  preserves their accepted ADR-0025 checks.
- [x] 2.4 Run existing Alan OS retired-host/workspace/legacy absence guards and
  the exact, pre-change-ratcheted Apple architecture warning ledger through the
  repository gate.

## 3. Canonical Gate, Git Hook, And CI

- [x] 3.1 Add one repository quality script and route `just lint`, `just check`,
  and a dedicated `just quality` command through it without formatting writes.
- [x] 3.2 Add a versioned pre-commit hook plus an explicit installer, install it
  in this checkout, and verify staged-whitespace and quality failures block a
  commit.
- [x] 3.3 Replace duplicated CI fmt/Clippy/doc command lists with one required
  quality job using the pinned toolchain while preserving behavioral jobs.
- [x] 3.4 Update contributor-facing validation text and the pull request
  checklist to name the canonical gate and the CI-versus-hook enforcement model.

## 4. Immediate Debt Follow-up

- [x] 4.1 Create the separate `refactor-clean-code-architecture-debt` OpenSpec
  change covering oversized Rust sources, all 15 Apple warnings, Agent Runtime
  Service/Agent Execution Engine seam debt, and Connection Service ownership.
- [x] 4.2 Record that its first behavior-preserving implementation PR immediately
  follows this gate PR before unrelated feature work.

## 5. Verification And Delivery

- [x] 5.1 Run the canonical quality gate from a clean environment and verify the
  repository hook invokes the same interface.
- [x] 5.2 Run workspace tests, current focused architecture tests, and the full
  OpenSpec strict validation set.
- [x] 5.3 Verify CI workflow syntax, `git diff --check`, and a clean scoped diff.
- [ ] 5.4 Open the focused gate PR, keep Codex review and required checks clean,
  and sync delta specs into canonical specs after merge before archive.
