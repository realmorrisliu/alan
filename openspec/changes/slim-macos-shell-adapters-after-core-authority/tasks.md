## 1. Baseline And Scope

- [ ] 1.1 Confirm `make-shell-core-authoritative` is merged or otherwise use its
  final accepted shell-core authority boundary as the baseline for this change.
- [ ] 1.2 Run `bash clients/apple/scripts/check-architecture-maintainability.sh`
  and record the current warning count, warning classes, and largest
  shell-core-adjacent Swift files in `clients/apple/ARCHITECTURE.md`.
- [ ] 1.3 Decide the first implementation batch target: named warnings to remove
  and the warning-count budget to reach without relaxing the checker.
- [ ] 1.4 Confirm existing shell-core authority guards still pass before any
  slimming work: `bash clients/apple/scripts/check-shell-contracts.sh`.

## 2. Split ShellCoreFFIAdapter Internals

- [ ] 2.1 Move dynamic library loading, symbol lookup, ABI checks, and shared
  adapter caching out of `ShellCoreFFIAdapter.swift` into a focused loader owner.
- [ ] 2.2 Move JSON envelope request/response encoding, null-payload handling,
  facade error mapping, and schema-version checks into a shared envelope owner.
- [ ] 2.3 Move portable state/content materialization and platform-field
  preservation helpers into a focused materialization owner.
- [ ] 2.4 Split manifest, reducer, control-command, action-registry, settings,
  and Terminal Profile operation payload/response adapters into operation-family
  files while keeping the public `ShellCoreFFIAdapter` method surface stable.
- [ ] 2.5 Run `bash clients/apple/scripts/test-shell-core-ffi-adapter.sh`,
  `cargo test -p alan-shell-core-ffi`, `bash clients/apple/scripts/check-shell-contracts.sh`,
  and architecture validation after the split.

## 3. Narrow ShellHostController

- [ ] 3.1 Extract workspace-manifest startup, shell-core materialization,
  pruning, diagnostics, and persistence-writer construction from
  `ShellHostController.swift` into a manifest/startup coordinator.
- [ ] 3.2 Extract manifest write scheduling, persisted shell-state publication,
  and control-plane state flushing into a persistence coordinator that preserves
  the existing debounce semantics.
- [ ] 3.3 Extract shell action dispatch and effect execution routing into a
  shell action coordinator while keeping Swift-owned UI and terminal effects
  explicit.
- [ ] 3.4 Extract reducer-backed command routing and shell-core failure
  diagnostics into a reducer command coordinator without reintroducing Swift
  domain fallback behavior.
- [ ] 3.5 Extract platform pane-field and runtime metadata preservation into a
  named adapter/service so reducer and control results preserve live macOS-only
  data consistently.
- [ ] 3.6 Run affected focused checks:
  `clients/apple/scripts/test-shell-workspace-manifest.sh`,
  `clients/apple/scripts/test-shell-runtime-metadata.sh`,
  `clients/apple/scripts/test-shell-action-registry.sh`,
  `clients/apple/scripts/test-shell-automation-command-seams.sh`,
  `clients/apple/scripts/check-shell-contracts.sh`, and architecture validation.

## 4. Move Fixture-Only Swift Helpers

- [ ] 4.1 Move or gate `ShellActionRegistry.standard` fixture data so production
  runtime code cannot compile against it as a fallback registry.
- [ ] 4.2 Move or gate Swift manifest parity helpers so default app builds do not
  carry runtime-accessible duplicate manifest default/prune/materialize logic.
- [ ] 4.3 Update Swift script compile invocations to include any new test-support
  files explicitly instead of depending on production model files for fixture
  helpers.
- [ ] 4.4 Verify equivalent Rust shell-core or FFI tests cover every moved
  fixture-only Swift domain helper.

## 5. Tighten Architecture Gates

- [ ] 5.1 Update `clients/apple/scripts/check-architecture-maintainability.sh`
  so resolved shell adapter/controller warnings cannot silently reappear.
- [ ] 5.2 Update `clients/apple/ARCHITECTURE.md` after each warning reduction
  with the new warning count, remaining owners, and next follow-up boundary.
- [ ] 5.3 Add or update shell-contract checks if any moved file creates a new
  place where Swift shell-domain fallback could return.

## 6. Final Verification

- [ ] 6.1 Run `git diff --check`.
- [ ] 6.2 Run `bash clients/apple/scripts/check-architecture-maintainability.sh`
  and confirm the report meets the target warning budget for this change.
- [ ] 6.3 Run `bash clients/apple/scripts/check-shell-contracts.sh`.
- [ ] 6.4 Run the focused Swift scripts touched by the final owner moves.
- [ ] 6.5 Run affected Rust checks, at minimum `cargo test -p alan-shell-core`
  and `cargo test -p alan-shell-core-ffi` when adapter-facing behavior moves.
- [ ] 6.6 Run `openspec validate slim-macos-shell-adapters-after-core-authority --type change --strict --json`.
- [ ] 6.7 Run `openspec validate --all --strict --json`.

## 7. Review And Archive Readiness

- [ ] 7.1 Review the final diff for behavior drift in shell startup, manifest
  persistence, action dispatch, control responses, Terminal Profile launch, and
  terminal runtime metadata preservation.
- [ ] 7.2 Ensure the PR summary lists the warning count before and after, the
  warnings removed, and any remaining large-file debt that intentionally stays
  out of scope.
- [ ] 7.3 After implementation is merged, sync accepted delta specs into
  `openspec/specs/`.
- [ ] 7.4 Archive the change only after implementation, review, validation, spec
  sync, and any required follow-up PRs are complete.
