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
| Readiness | Run actual full namespace/runner/toolchain probes | Required enforcing backend selected; fallback reasons separately retained | Baseline failed library bind; repaired native runner executed; complete toolchain selection remains pending |
| PATH order | Two supported providers of the same executable | Exact original first provider; trusted helpers unchanged | Not run |
| Rust identity | Run selected cargo/rustc through supported installed selection | Exact 1.97 identity and required runtime files; no auto-install or default rewrite | Not run |
| Local build | Build project with independent explicitly read-only mounted dependency | Successful actual compile; source/dependency unchanged; output authorized/private | Not run |
| RED/GREEN | Run failing test, make one authorized bounded source fix, rerun test | Failure observed first; exact one-change diff; successful corrected test | Not run |
| Git diff | Read actual project diff | Exact requested source change; unrelated files absent | Not run |
| Read-only project | Build/test read-only project with private output | Successful compile/test; all project hashes unchanged | Not run |
| PATH rejection | Unset, empty, relative, escaping and unsupported executable entries | Explicit unavailable/refusal before user effect; no substitution | Not run |
| Runtime loss | Remove fixture runtime input or change selection after readiness | Per-command refusal; no stale-startup authority | Not run |
| Dependency missing | Build without separate dependency authority | No dependency read; explicit failure; cache not accepted as new execution | Not run |
| Dependency revoked | Revoke prior dependency then request next build | No stale grant, ambient path or retained-output false success | Not run |
| Dependency escape | Dependency symlink targets an ungranted sibling | Escape denied; no widened common-parent mount | Not run |
| Read-only writes | Attempt source/dependency/runtime mutation | All denied; original byte inventories unchanged | Not run |
| Home isolation | Tool attempts fixture home/credential canaries | Undeclared canaries absent; personal directories never mounted or copied | Not run |
| Private cache | Cargo writes cache/output | Only private or authorized outputs; original toolchain and personal Cargo home unchanged | Not run |
| Network denial | Attempt isolated network operation | OS confinement blocks; existing network approval unchanged | Not run |
| Cancellation | Command descendant schedules a delayed marker then cancel/timeout | Wait past delay; marker absent, no surviving writer, truthful cancelled/unknown state | Not run |
| Fallback | Required namespace/remount/network capability unavailable | Explicit fallback/approval posture; not counted as enforcing-backend pass | Not run |

Before any native row runs, record fixture/tool provisioning, readiness and exact
expected effects. A test that returns early on an unavailable probe is a portable
skip, not a successful matrix row. Qualification must preserve this distinction
in CI and user-facing progress.
