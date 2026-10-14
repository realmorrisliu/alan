## Closure receipt — 2026-10-14

Implementation PR #1046 is merged at `465e3e474ee8af98a7647b5eee06e748b9fd9aa6` and is an ancestor of
main `ea929481`. GitHub reinspection confirms all sixteen checks succeeded
on reviewed head `a6ea2518ccb214471edb87198bce174eb34993fc` and no unresolved review threads remain.
All planning artifacts are complete. This documentation-only closure synchronizes
the delivered delta into canonical specs and archives the completed change;
canonical adoption of this closure itself remains subject to its PR merge.
Existing later canonical requirements are retained. No new runtime execution,
personal-store migration, deployment or qualification result is claimed.
Earlier status entries below are historical and do not override this receipt.

The subsequent Rust-query repairs are already merged in #1047 and #1056;
repeated model-development evidence remains with its separate qualification
change. This archive retains the bounded Linux fixture and unsupported-platform
limits and does not claim general toolchain support.

# Tasks

## 1. Baseline and reproducible acceptance

- [x] 1.1 Trace Host adapter, plan, trusted helper, backend selection and cancellation callers; inventory the real Linux tools/version/PATH and record missing or unsupported inputs in design.md.
- [x] 1.2 Freeze the positive/negative native fixture matrix, artifact/effect assertions and required identity fields in acceptance-matrix.md; keep all unexecuted slots explicit.

## 2. Supported tool environment

- [x] 2.0 Repair native inherited-submount projection and recursively enforce read-only access; reproduce before fixing, then verify root/child write denial, escaped paths and unavailable handling for failed child remounts.
- [x] 2.1 Carry supported PATH unchanged into user execution, separate from trusted helper PATH; verify same-name ordering, aliases, unset/empty/relative/unsafe entries with focused regressions.
- [x] 2.2 Make readiness and per-command plan construction agree and revalidate changed inputs before effects; verify unsupported paths retain an explicit safe fallback reason.
- [x] 2.3 Project selected installed Rust executables and required runtime files read-only through the existing Host adapter/plan; verify exact selected version, missing runtime, symlink escape and unchanged installed content.
- [x] 2.4 Provide private home/Cargo/build scratch with existing Process lifetime; verify personal credentials/caches remain absent and unchanged, authorized configuration is preserved, and read-only projects build with private output.

## 3. Real Linux execution and authority

- [x] 3.1 Provision a task-owned shell/git/Rust 1.97 fixture and actual backend readiness receipt; verify original personal Rust defaults/stores are unchanged and report unsupported enforcing-backend slots.
- [x] 3.2 Run actual local dependency build, failing/corrected test and git diff under the enforcing backend; assert exact source/artifact diffs and selected executable/version rather than version-only output.
- [x] 3.3 Exercise separate read-only dependency grants, missing/revoked authority and escaping paths against native execution; verify no unauthorized access and no cached false success.
- [x] 3.4 Verify native unrelated-home/credential canaries absent, read-only source/runtime writes denied and network denied; recheck original files and cache isolation after exit.
- [x] 3.5 Cancel/time out a development command with descendants; wait beyond its delayed effect and assert no later or duplicated effect and correct scratch lifetime.

## 4. Review and delivery

- [x] 4.1 Run affected Rust suites, full quality, pinned strict OpenSpec and applicable distribution checks; record exact source/head and native limitations separately from portable skips.
- [x] 4.2 Review every changed caller and positive/negative matrix result; resolve findings and collect passing current-head CI in a reviewable PR.
- [x] 4.3 After user merge, verify revision/checks, synchronize the delivered canonical delta and update disposition and archive-readiness evidence.

## Workflow follow-up

- Archive only after implementation merge and canonical synchronization.
- Start the independent five-family real-development qualification, at least three runs per family on macOS and Linux; retain first attempts, human interventions, duration, artifacts and exact effects.
