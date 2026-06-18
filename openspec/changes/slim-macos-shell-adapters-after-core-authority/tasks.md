## 1. Baseline And Scope

- [x] 1.1 Confirm `make-shell-core-authoritative` is merged or otherwise use its
  final accepted shell-core authority boundary as the baseline for this change.
- [x] 1.2 Run `bash clients/apple/scripts/check-architecture-maintainability.sh`
  and record the current warning count, warning classes, and largest
  shell-core-adjacent Swift files in `clients/apple/ARCHITECTURE.md`.
- [x] 1.3 Decide the first implementation batch target: named Swift legacy
  implementations to remove or move out of production sources without relaxing
  shell-core authority checks.
- [x] 1.4 Confirm existing shell-core authority guards still pass before any
  slimming work: `bash clients/apple/scripts/check-shell-contracts.sh`.

## 2. Remove Production Swift Legacy Domain Implementations

- [x] 2.1 Move Swift manifest default/prune/materialize/migration parity helpers
  out of `Models/Shell/ShellWorkspaceManifest.swift` into explicit script
  support so the app target keeps only manifest DTOs, compatibility decode/
  repair, and platform file IO helpers.
- [x] 2.2 Move `ShellActionRegistry.standard`, its action descriptor table, and
  resolver fixture out of `Models/Shell/ShellActionRegistry.swift` into explicit
  script support so production runtime cannot compile against the Swift action
  registry as a fallback.
- [x] 2.3 Update Swift script compile invocations to include the new test-support
  files explicitly instead of depending on production model files for fixture
  helpers.
- [x] 2.4 Tighten `check-shell-contracts.sh` so production Apple sources reject
  `SHELL_MANIFEST_PARITY_FIXTURES`, `ShellWorkspaceMaterializer`, and
  `ShellActionRegistry.standard`, while scripts may still compile dedicated
  support fixtures.
- [x] 2.5 Run the affected focused checks:
  `clients/apple/scripts/test-shell-workspace-manifest.sh`,
  `clients/apple/scripts/test-shell-action-registry.sh`,
  `clients/apple/scripts/test-shell-sidebar-tab-rows.sh`,
  `cargo test -p alan-shell-core`, `cargo test -p alan-shell-core-ffi`,
  `bash clients/apple/scripts/check-shell-contracts.sh`, and architecture
  validation.

## 3. Split ShellCoreFFIAdapter Internals

- [x] 3.1 Move dynamic library loading, symbol lookup, ABI checks, and shared
  adapter caching out of `ShellCoreFFIAdapter.swift` into a focused loader owner.
- [x] 3.2 Move JSON envelope request/response encoding, null-payload handling,
  facade error mapping, and schema-version checks into a shared envelope owner.
- [x] 3.3 Move portable state/content materialization and platform-field
  preservation helpers into a focused materialization owner.
- [x] 3.4 Split manifest, reducer, control-command, action-registry, settings,
  and Terminal Profile operation payload/response adapters into operation-family
  files while keeping the public `ShellCoreFFIAdapter` method surface stable.
- [x] 3.5 Run `bash clients/apple/scripts/test-shell-core-ffi-adapter.sh`,
  `cargo test -p alan-shell-core-ffi`, `bash clients/apple/scripts/check-shell-contracts.sh`,
  and architecture validation after the split.

## 4. Narrow ShellHostController

- [x] 4.1 Extract workspace-manifest startup, shell-core materialization,
  pruning, diagnostics, and persistence-writer construction from
  `ShellHostController.swift` into a manifest/startup coordinator.
- [x] 4.2 Extract manifest write scheduling, persisted shell-state publication,
  and control-plane state flushing into a persistence coordinator that preserves
  the existing debounce semantics.
- [x] 4.3 Extract shell action dispatch and effect execution routing into a
  shell action coordinator while keeping Swift-owned UI and terminal effects
  explicit.
- [x] 4.4 Extract reducer-backed command routing and shell-core failure
  diagnostics into a reducer command coordinator without reintroducing Swift
  domain fallback behavior.
- [x] 4.5 Extract platform pane-field and runtime metadata preservation into a
  named adapter/service so reducer and control results preserve live macOS-only
  data consistently.
- [x] 4.6 Run affected focused checks:
  `clients/apple/scripts/test-shell-workspace-manifest.sh`,
  `clients/apple/scripts/test-shell-runtime-metadata.sh`,
  `clients/apple/scripts/test-shell-action-registry.sh`,
  `clients/apple/scripts/test-shell-automation-command-seams.sh`,
  `clients/apple/scripts/check-shell-contracts.sh`, and architecture validation.

## 5. Tighten Architecture Gates

- [x] 5.1 Update `clients/apple/scripts/check-architecture-maintainability.sh`
  so resolved shell adapter/controller or legacy-production-source warnings
  cannot silently reappear.
- [x] 5.2 Update `clients/apple/ARCHITECTURE.md` after each legacy cleanup or
  owner split with the new warning count, remaining owners, and next follow-up
  boundary.
- [x] 5.3 Add or update shell-contract checks if any moved file creates a new
  place where Swift shell-domain fallback could return.

## 6. Final Verification

- [x] 6.1 Run `git diff --check`.
- [x] 6.2 Run `bash clients/apple/scripts/check-architecture-maintainability.sh`
  and confirm the report has no new warnings and records any remaining debt by
  owner boundary.
- [x] 6.3 Run `bash clients/apple/scripts/check-shell-contracts.sh`.
- [x] 6.4 Run the focused Swift scripts touched by the final owner moves.
- [x] 6.5 Run affected Rust checks, at minimum `cargo test -p alan-shell-core`
  and `cargo test -p alan-shell-core-ffi` when adapter-facing behavior moves.
- [x] 6.6 Run `openspec validate slim-macos-shell-adapters-after-core-authority --type change --strict --json`.
- [x] 6.7 Run `openspec validate --all --strict --json`.

## 7. Review And Archive Readiness

- [x] 7.1 Review the final diff for behavior drift in shell startup, manifest
  persistence, action dispatch, control responses, Terminal Profile launch, and
  terminal runtime metadata preservation.
- [x] 7.2 Ensure the PR summary lists the Swift legacy implementations removed
  from production sources, the warning count before and after, and any remaining
  adapter/controller debt that intentionally stays out of scope.
- [ ] 7.3 After implementation is merged, sync accepted delta specs into
  `openspec/specs/`.
- [ ] 7.4 Archive the change only after implementation, review, validation, spec
  sync, and any required follow-up PRs are complete.
