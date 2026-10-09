# Tasks

These are implementation tasks. Read disposition.md before applying and implementation-evidence.md for recorded checks. Groups 1–2 can land independently; group 3's migration support must be usable before group 4's channel removal ships.

## 1. Build artifacts and verification workflows

- [x] 1.1 Inventory every repository build/install/release/hook/harness target selection, including configured intermediate directories and external overrides; deliver a path/owner/caller table and verify it against Cargo metadata and script fixtures.
- [x] 1.2 Reuse compatible ordinary checkout output for build/install/release while preserving the quality gate's owned host-target directory; update installer/distribution tests and prove ambient target overrides cannot select an unrelated executable.
- [x] 1.3 Add task-owned output receipts and coordinated producer/cleanup access for managed external targets, rejecting preexisting unowned paths; verify competing build/cleanup, symlink escape and changed-owner cases with a small fixture suite.
- [x] 1.4 Add read-only `just cache-status` and dry-run-first `just cache-clean`, using supported Cargo cleanup scopes and explicit application; verify unknown, dirty-source, active-consumer and recreated paths remain intact and no branch/registration changes occur.
- [x] 1.5 Make disposable validation disable incremental builds, retain selected evidence outside its output, and retire its output only after consumers finish; document the lifecycle and verify interrupted/failed task teardown preserves required diagnostics.
- [ ] 1.6 Map CI rust-cache to each job's actual target including quality-gate output; verify resolved paths in CI logs and keep existing required checks and toolchain pins.
- [ ] 1.7 Measure cold build, unchanged rebuild, source-edit rebuild and two-worktree runs with allocated bytes/file counts; trial lighter debug info, verify stack trace usability and record whether to adopt or retain current defaults without adding sccache.

## 2. Embedded packages and temporary service files

- [x] 2.1 Enumerate all callers of preinstalled source/ID APIs and separate ID enumeration from asset access; verify ID-only calls perform no filesystem writes.
- [x] 2.2 Adapt embedded relative entries into the existing validated PackageSnapshot/seeding path without reversing crate dependencies; compare package IDs, bytes, metadata, exports and revision digests against directory-based fixtures, including bounds and path rejection.
- [x] 2.3 Remove static per-PID Skill extraction and migrate directory-dependent tests to scoped fixtures; verify repeated product boots and test runs leave no per-PID Skill roots while ordinary package references still resolve.
- [x] 2.4 Give ephemeral Connection Service a scoped directory for metadata and lock files, retaining consumer lifetime and observable explicit cleanup; verify normal shutdown, held consumers and cleanup errors alongside existing Package Service guards.
- [x] 2.5 Add narrowly owned stale-scratch maintenance and document its limits; verify abrupt termination, PID reuse, live owners, missing markers and symlink roots never authorize deleting durable or unknown data.

## 3. Explicit adoption of legacy installation data

- [x] 3.1 Extend legacy-state inspection to report stable/dev paired roots and canonical-data presence without exposing secrets; add fixture tests for absent, single-source, dual-source, canonical and malformed layouts.
- [x] 3.2 Implement the explicit `migrate-installation --from stable|dev` operation and non-mutating dry run; verify no automatic selection, no whole-store merge, unchanged sources and rejection of populated canonical destinations.
- [ ] 3.3 Implement source-writer quiescence checks and staged owner validation of metadata, credentials, package references, rollouts/checkpoints, Memory Stores and definitions; verify unknown schema, unsupported path references, missing credentials and changing source fail before publication without provider calls or replay.
- [ ] 3.4 Implement the paired-store transaction journal and incomplete-publication guard in every data opener, including metadata and auth paths; fault-inject interruption before/between/after publication and verify resume, rollback and idempotent committed retry.
- [ ] 3.5 Document source selection, required writer shutdown, source retention and post-write rollback limits in current CLI help/docs; run each documented flow in isolated macOS/Linux fixtures and confirm generated scratch is excluded from adoption.

## 4. One executable and channel-free composition

- [x] 4.1 Replace channel-bearing Host store/boot/service construction with explicit product or temporary test bindings; verify the canonical platform layout and no personal-home access from tests, updating affected public APIs and exports together.
- [x] 4.2 Remove engine/auth argv/environment channel detectors and duplicated auth path construction; reject obsolete selection before writable access while preserving processless version/help, verified by subprocess and credential-path tests.
- [x] 4.3 Remove channel suffixes and identity matching from current runtime endpoint/status handling with a bounded version transition; verify independent invocations, same-directory exclusion, peer authorization, stale/old receipt rejection and no endpoint fallback.
- [x] 4.4 Update legacy authored-content import and historical credential discovery to explicit historical source selection; retain sandbox denies for old secret paths and verify no implicit source overlays or credential fallback.
- [ ] 4.5 Unify install/uninstall manifests, remove dev Just recipes and archive aliases, and preflight both old manifests before retirement; extend distribution tests for modified/unowned files, dev-only/stable-only/dual installs and handled-signal rollback.
- [ ] 4.6 Update current docs, AGENTS.md, local dev-verification guidance, scripts and fixtures to the one-product contract; verify current executable references with scoped search, keeping historical ADRs/archives unchanged and reconciling active input-lifecycle storage wording only.

## 5. Integration, review and delivery

- [ ] 5.1 Run two simultaneous `alan` invocations against the shared canonical product stores in isolated fixtures; verify separate Root/input/cwd/endpoint state, concurrent package/connection/auth safety and explicit recovery without repeated effects.
- [ ] 5.2 Run focused Rust/service/CLI and installer tests, `just quality`, `just test`, standalone distribution checks, and strict OpenSpec validation; record exact head and distinguish local validation from current-head required CI.
- [ ] 5.3 Review the current-head diff for store ownership, crash recovery, cleanup races and complete caller/fixture coverage; resolve findings and obtain passing required CI before implementation merge.

## Workflow follow-up

- After implementation is merged, sync these deltas into canonical specs, reconcile channel-free Purpose text where appropriate, and confirm all acceptance tasks against merge evidence before declaring archive readiness.
- Archive only after merged implementation and spec sync; verify the resulting archived location and retain historical records unchanged.
- Installing the unified binary and adopting the user's selected legacy store are explicit operational actions after implementation; this design session performs neither.
