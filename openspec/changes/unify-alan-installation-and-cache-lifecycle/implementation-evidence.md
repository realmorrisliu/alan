# Implementation evidence

## Activation and baseline

Implementation authorized by the user's goal request on 2026-10-09. Working branch: `codex/unify-installation-cache-20261009`; baseline `aac0e6117c3e2a629267113e3de6a418022c3b83` includes the routing delivery documentation merged after the design baseline. No installed executable or personal store is changed by this work.

## Build-output inventory — task 1.1

| Caller | Current output selection | Owner / required treatment |
| --- | --- | --- |
| `just test`, `build`, `release-check`, `smoke`, direct Cargo, live provider/runtime scripts | Effective Cargo configuration, default checkout `target` | Checkout; retain iterative reuse and discover overrides with metadata. |
| `just coverage*` | Cargo llvm-cov instrumentation output; explicit HTML `target/coverage` | Checkout; distinguish instrumentation configuration, preserve selected coverage reports. |
| `scripts/check-quality.sh` | `ALAN_QUALITY_TARGET_DIR` or checkout `target/quality-gate`, forced host triple | Quality run; retain isolation and verify that run's executable. |
| `.githooks/pre-commit` | Index-only temporary source snapshot, but original checkout `target/quality-gate` | Original checkout owns output; temporary snapshot is a consumer, not a second output owner. |
| `scripts/check-rust-quality.sh` | Inherits Cargo output; invoked independently on macOS CI and through the canonical gate | Resolve actual invocation configuration, not just script defaults. |
| `scripts/install-cli.sh` | `ALAN_STANDALONE_TARGET_DIR` or `target/standalone-release` | Reuse compatible ordinary checkout artifacts; explicit skip-build callers must identify the intended artifact. |
| `scripts/assemble-cli-release.sh` | Same standalone target, explicit target triple; transient `cli-stage.*`; archive under `ALAN_RELEASE_OUT_DIR` or `target/distributions` | Separate final archive from compiler output; stage is transient. |
| `scripts/test-standalone-cli-distribution.sh` | `target/standalone-test.*` fixture root; inherited standalone target or fixture-private target | Fixtures/install destinations are temporary; an inherited gate target is not owned by the fixture cleanup. |
| `scripts/harness/run_{autonomy,repo_worker,compaction,coding_steward}_suite.sh` and `lib.sh` | Fixture command inherits Cargo configuration; reports under `target/harness/<suite>/latest` | Reports currently share the target parent; move retained evidence outside disposable compiler trees. |
| `scripts/harness/run_self_eval_suite.sh` | Per-profile `cargo-target`, optional detached worktree, reports under `target/harness/self_eval/latest` | One-shot per-profile build output; preserve runner/result evidence before teardown. Existing force-removal paths need ownership review. |
| `scripts/repo-worker/run_smoke.sh` | Generated fixture workspace under `target/repo-worker/smoke/latest`, Cargo invoked in fixture | Disposable fixture source/build; preserve reported task evidence separately. |
| Embedded Skill tooling scripts | Harness commands can run Cargo in explicit fixture/project workspaces | Do not claim unrelated project targets; repository fixture lifetimes own only their generated workspace. |
| `.github/workflows/ci.yml` quality job | Gate uses nested quality target, rust-cache currently defaults to outer target | Map cache to actual nested root for dependency pruning. |
| Other CI jobs and release matrix | Ordinary target or explicit target-triple subtree; release upload uses `target/<triple>/release/alan` | Keep target layout synchronized with upload and cache mapping. |
| `just clean` | `cargo clean` followed by unconditional `rm -rf target/` | Replace with explicit, coordinated artifact cleanup; this currently conflates evidence and compiler output. |
| Ad hoc external `direct-review-target` | Outside repository script defaults | Unmanaged historical input; status may report it, but a workflow must not retroactively claim or delete it by name. |

Read-only Cargo 1.97 metadata checks passed for default output, `CARGO_TARGET_DIR`, and distinct `CARGO_BUILD_BUILD_DIR`; metadata exposes both `target_directory` and `build_directory`. No builds or external-directory cleanup were needed for these checks. Repository/script search covered `scripts`, `.githooks`, CI, Just recipes and embedded Skill tooling. Benchmarks and full CI remain pending.

## Compatible CLI build output — task 1.2

Installer and release assembly now resolve Cargo's configured output (unless their explicit standalone override is supplied) rather than creating `target/standalone-release`. Both consume Cargo's `compiler-artifact` executable from the successful current build. Skip-build callers must supply an explicit source; the quality gate passes its own just-built host executable. Installer fixtures compile/select once and reuse that source across ownership and rollback checks.

`python3 scripts/test_cargo_cli_output.py`: 5 tests passed, covering actual Cargo metadata config/override resolution, reported cross-target artifact selection over a stale guessed path, failed-build rejection, missing explicit skip-build source, and the full distribution/ownership/handled-signal fixture with simulated Cargo artifact events. Shell syntax checks and `git diff --check` passed. These tests do not constitute a real full Rust build or release qualification; those remain group 5 acceptance tasks. No user installation occurred.

## Managed output and cleanup — tasks 1.3–1.4

`scripts/build_artifacts.py` resolves both Cargo output directories, records only newly owned directories, and keeps receipts in the owning checkout's Git metadata plus a sidecar next to output. Producers hold inheritable shared file locks; cleanup needs exclusive access, unchanged ownership/inode, known Cargo entries, and an idle open-file scan before scoped `cargo clean`. Registered concurrent producers can share access; nested quality/installer processes retain the outer lease. Replaced/symlink/unknown directories fail closed. Existing nonempty checkout caches remain unmanaged, and external unowned output is rejected. `just clean` now previews rather than deleting the entire target tree.

Integrated ordinary Just build/test/coverage/smoke, CLI install/release consumers, quality and index-snapshot ownership, standalone Rust quality, live-test commands and shared harness execution. Disposable self-eval/repo-worker teardown remains task 1.5. Quality also forces its intermediate directory, preventing ambient `build-dir` from crossing its correctness boundary. CI quality cache now explicitly maps `. -> target/quality-gate`; remote path evidence remains task 1.6.

15 output-lifecycle fixture tests and 5 CLI artifact/distribution fixture tests passed locally. They cover shared producer exclusion of cleanup, surviving child handles, nested quality leases, external-owner rejection, replaced inodes, symlinks, read-only inventory, unmanaged existing output, active consumers, authored patches, preview/application, and a distinct ambient intermediate directory. Applied cleanup runs only in temporary fixtures; source and Git state assertions pass. Shell syntax and diff whitespace checks pass. Full real Rust workflows and remote CI remain unverified at this stage.

## Embedded packages and scoped Connection scratch — tasks 2.1–2.4

All production preinstalled-source callers were the Service Manager seed loop and initial reference projection. Projection now enumerates IDs only. Seeding adapts immutable embedded entries into the existing PackageSnapshot validator/materializer, preserving the source leaf, tooling exclusion, executable bits and sorted entries. The static OnceLock and per-version/PID extraction code are removed. Engine registry/capability/prompt tests now receive a caller-owned temporary directory; no helper keeps a directory alive globally.

Equivalence fixtures compare all five embedded packages against directory snapshots, including bytes, paths, executable bits, revision hash, materialized fingerprint and exports. Invalid relative paths, VCS entries, duplicate/empty/over-limit snapshots are rejected at the same boundary. Repeated ID/source enumeration leaves the historical current-PID directory unchanged. Existing boot/package lifecycle tests cover normal package reference resolution.

ConnectionService::ephemeral is now fallible and owns a TempDir containing metadata and its lock. All constructors/callers/tests were updated. The final consumer's release removes both; explicit close rejects active consumers and reports filesystem cleanup failures. Persistent stores have no temporary guard. Abrupt-exit scratch sweeping remains task 2.5.

Local validation: 132 Skill tests, 32 prompt-cache tests, 150 Service Manager unit tests plus 2 integration tests passed. `cargo clippy --locked -p alan-agent-engine -p alan-service-manager --all-targets -- -D warnings`, formatting, source-size and architecture gates passed. Full workspace/quality/distribution and current-head CI remain group 5 work. Build sidecar receipts/locks are ignored as generated state.
