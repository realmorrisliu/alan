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
| Readiness | Run actual full namespace/runner/toolchain probes | Required enforcing backend selected; fallback reasons separately retained | Baseline library-bind failure repaired; normal Sandbox Rust cases select and execute the namespace backend; complete git/toolchain fixture remains pending |
| PATH order | Two supported providers of the same executable | Exact original first provider; trusted helpers unchanged | Passed native fixture: both orders and relative alias; PATH exact; fake setup helpers ignored. See PATH slice evidence |
| Rust identity | Run selected cargo/rustc through supported installed selection | Exact 1.97 identity and required runtime files; no auto-install or default rewrite | Passed actual Sandbox project/environment/+selector cases: rustc/cargo/rustdoc 1.97.0, unit/doctests/fmt/Clippy, original settings/proxy bytes unchanged |
| Local build | Build project with independent explicitly read-only mounted dependency | Successful actual compile; source/dependency unchanged; output authorized/private | Passed explicit lower-runner and normal Sandbox builds, with six exact source files unchanged; RED/GREEN and git follow-up remain pending |
| RED/GREEN | Run failing test, make one authorized bounded source fix, rerun test | Failure observed first; exact one-change diff; successful corrected test | Passed actual normal Sandbox compile/test with independent read-only dependency: failing assertion, one-line fix, green test and exact original-file checks; actual Bash product-entry qualification remains open |
| Git diff | Read actual project diff | Exact requested source change; unrelated files absent | Passed git 2.53 native diff and exact porcelain status: only src/lib.rs modified; Bash product-entry qualification remains open |
| Read-only project | Build/test read-only project with private output | Successful compile/test; all project hashes unchanged | Passed normal Sandbox compile/unit/doctests/fmt/Clippy with private target, exact source files unchanged and no project/dependency target |
| PATH rejection | Unset, empty, relative, escaping and unsupported executable entries | Explicit unavailable/refusal before user effect; no substitution | Passed focused pure/native regressions including dangling/chained aliases, NUL/non-UTF-8 and changed backing; broader selected Rust rows remain open |
| Runtime loss | Remove fixture runtime input or change selection after readiness | Per-command refusal; no stale-startup authority | Passed native missing selectors/components, changed project selection after success and runtime-root alias refusal before marker; executable loss/byte changes, helper alias and component metadata changes/escape have portable regressions |
| Dependency missing | Build without separate dependency authority | No dependency read; explicit failure; cache not accepted as new execution | Passed manual Sandbox declaration absence with fresh private build output; actual HostMountService/Bash admission remains open |
| Dependency revoked | Revoke prior dependency then request next build | No stale grant, ambient path or retained-output false success | Passed declaration removal after cached success: Cargo fails to load the dependency manifest; actual live-grant revocation remains open |
| Dependency escape | Dependency symlink targets an ungranted sibling | Escape denied; no widened common-parent mount | Passed actual Cargo refusal for source symlink into an ungranted sibling; escaped compile_error payload never executed; actual live-grant entry remains open |
| Read-only writes | Attempt source/dependency/runtime mutation | All denied; original byte inventories unchanged | Native source/runtime write denial passed; complete dependency and authority matrix pending |
| Home isolation | Tool attempts fixture home/credential canaries | Undeclared canaries absent; personal directories never mounted or copied | Native private environment/canary tests passed, including private proc root; normal developer entry remains pending |
| Private cache | Cargo writes cache/output | Only private or authorized outputs; original toolchain and personal Cargo home unchanged | Passed lower-runner and automatic Sandbox read-only builds plus fresh per-command cache checks; broad developer lifecycle/isolation row remains pending |
| Network denial | Attempt isolated network operation | OS confinement blocks; existing network approval unchanged | Passed actual compiled test: connection to a live Host loopback listener denied, no connection accepted; normal Sandbox fixture, not a separate live-grant Bash run |
| Cancellation | Command descendant schedules a delayed marker then cancel/timeout | Wait past delay; marker absent, no surviving writer, truthful cancelled/unknown state | Passed actual Cargo/test/shell descendants for abort and timeout: ready first, one start, wait beyond 15-second effect, no marker/live writer and runner root removed; no Agent UI state qualification inferred |
| Fallback | Required namespace/remount/network capability unavailable | Explicit fallback/approval posture; not counted as enforcing-backend pass | Unsupported PATH actual readiness selects Landlock with a retained reason; required namespace execution refuses before marker. Other unavailable-capability slots are separately covered or remain unsupported, never counted as namespace passes |

Before any native row runs, record fixture/tool provisioning, readiness and exact
expected effects. A test that returns early on an unavailable probe is a portable
skip, not a successful matrix row. Qualification must preserve this distinction
in CI and user-facing progress.
