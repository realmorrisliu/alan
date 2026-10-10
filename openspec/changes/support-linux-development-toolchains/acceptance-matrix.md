# Frozen Linux development acceptance matrix — 2026-10-10

Baseline source: `faf747c2`. Required fixture: disposable project, independent
local Rust dependency, git, selected installed Rust 1.97, unrelated-home and
credential canaries. No personal secret bytes enter test reports. Every row
requires actual runtime evidence; all rows below are initially **not run**.

## Identity and measurement fields

For each run retain source/dirty state, binary digest, machine/kernel/arch/UID,
selected shell/git/cargo/rustc paths and versions, ordered PATH, actual backend
and readiness diagnostics, project/dependency mount identity and access, command,
exit/outcome, stdout/stderr, elapsed time, artifact hashes/diff and intervention.
Initial failures and corrected retries are separate records. Explicit-command
native execution is not real-model code-authorship qualification.

## Positive and negative rows

| Row | Operation | Required assertion | Status |
| --- | --- | --- | --- |
| Readiness | Run actual full namespace/runner/toolchain probes | Required enforcing backend selected; fallback reasons separately retained | Passed actual Sandbox/git/Rust fixture and live-service Bash readiness; complete original installation inventory is exact across execution; unsupported PATH fallback separately retained |
| PATH order | Two supported providers of the same executable | Exact original first provider; trusted helpers unchanged | Passed native fixture: both orders and relative alias; PATH exact; fake setup helpers ignored. See PATH slice evidence |
| Rust identity | Run selected cargo/rustc through supported installed selection | Exact 1.97 identity and required runtime files; no auto-install or default rewrite | Passed actual Sandbox project/environment/+selector cases: rustc/cargo/rustdoc 1.97.0, unit/doctests/fmt/Clippy, original settings/proxy bytes unchanged |
| Local build | Build project with independent explicitly read-only mounted dependency | Successful actual compile; source/dependency unchanged; output authorized/private | Passed explicit lower-runner and normal Sandbox builds, with six exact source files unchanged; RED/GREEN and git follow-up remain pending |
| RED/GREEN | Run failing test, make one authorized bounded source fix, rerun test | Failure observed first; exact one-change diff; successful corrected test | Passed actual normal Sandbox compile/test with independent read-only dependency: failing assertion, one-line fix, green test and exact original-file checks; actual Bash product-entry qualification remains open |
| Git diff | Read actual project diff | Exact requested source change; unrelated files absent | Passed git 2.53 native diff and exact porcelain status: only src/lib.rs modified; Bash product-entry qualification remains open |
| Read-only project | Build/test read-only project with private output | Successful compile/test; all project hashes unchanged | Passed normal Sandbox compile/unit/doctests/fmt/Clippy; actual Bash with selected live read-only grant also builds/tests twice with private output, no project target and six exact source files. Bash dependency is inside that selected grant, not disjoint |
| PATH rejection | Unset, empty, relative, escaping and unsupported executable entries | Explicit unavailable/refusal before user effect; no substitution | Passed focused pure/native regressions including dangling/chained aliases, NUL/non-UTF-8 and changed backing; broader selected Rust rows remain open |
| Command-local Rust selection | Literal selectors, nested transparent wrappers and environment-reset attempts | Correct selection precedence and missing/unsupported input refusal before any earlier command effect | Passed final native literal-selector and CLI-precedence compile/unit/doc/fmt/Clippy; missing inline/persistent selector changes refuse before marker. Normalized wrappers have portable inspection tests; actual wrappers remain refused by the existing full shape guard before marker. Old observed marker and first failed wrapper-fixture assumption are retained |
| Shell-local cwd | Literal/unknown directory changes then Rust invocation | Inspect bounded in-grant possible selections and metadata; reject missing runtime before effects; preserve literal implicit and explicit builds | Old-source marker observed. Final native literal implicit child build and three fixed-selection builds pass compile/unit/doc/fmt/Clippy with exact sources/private output; missing child runtime refuses before marker. Portable unknown/escape/state-bound and changed-selection metadata checks pass; unknown implicit directory states remain unavailable |
| Runtime loss | Remove fixture runtime input or change selection after readiness | Per-command refusal; no stale-startup authority | Passed native missing selectors/components, changed project selection after success and runtime-root alias refusal before marker; executable loss/byte changes, helper alias and component metadata changes/escape have portable regressions |
| Dependency missing | Build without separate dependency authority | No dependency read; explicit failure; cache not accepted as new execution | Passed manual Sandbox declaration absence with fresh private build output; actual HostMountService/Bash admission remains open |
| Dependency revoked | Revoke prior dependency then request next build | No stale grant, ambient path or retained-output false success | Passed declaration removal after cached success: Cargo fails to load the dependency manifest; actual live-grant revocation remains open |
| Dependency escape | Dependency symlink targets an ungranted sibling | Escape denied; no widened common-parent mount | Passed actual Cargo refusal for source symlink into an ungranted sibling; escaped compile_error payload never executed; actual live-grant entry remains open |
| Read-only writes | Attempt source/dependency/runtime mutation | All denied; original byte inventories unchanged | Passed actual Bash compiled source/in-project dependency/runtime write denial plus exact source/runtime checks; full original installation inventory unchanged. Disjoint dependency authority remains open |
| Home isolation | Tool attempts fixture home/credential canaries | Undeclared canaries absent; personal directories never mounted or copied | Passed actual Bash with live selected read-only grant: ungranted home/credential canary unreadable, fresh private Cargo home has no credentials; source canaries unchanged. No personal Cargo registry/credential inventory or mount |
| Private cache | Cargo writes cache/output | Only private or authorized outputs; original toolchain and personal Cargo home unchanged | Passed actual Bash consecutive builds: per-command private cache marker absent at next start, private output and no project target; 379 original installation/settings/proxy nodes exact after native Sandbox and Bash runs |
| Network denial | Attempt isolated network operation | OS confinement blocks; existing network approval unchanged | Passed actual Bash compiled test under live selected read-only grant: connection to live Host loopback listener denied and no connection accepted; no network capability or approval rule changed |
| Cancellation | Command descendant schedules a delayed marker then cancel/timeout | Wait past delay; marker absent, no surviving writer, truthful cancelled/unknown state | Passed actual Cargo/test/shell descendants for abort and timeout: ready first, one start, wait beyond 15-second effect, no marker/live writer and runner root removed; no Agent UI state qualification inferred |
| Fallback | Required namespace/remount/network capability unavailable | Explicit fallback/approval posture; not counted as enforcing-backend pass | Unsupported PATH actual readiness selects Landlock with a retained reason; required namespace execution refuses before marker. Other unavailable-capability slots are separately covered or remain unsupported, never counted as namespace passes |

Before any native row runs, record fixture/tool provisioning, readiness and exact
expected effects. A test that returns early on an unavailable probe is a portable
skip, not a successful matrix row. Qualification must preserve this distinction
in CI and user-facing progress.
