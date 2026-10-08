# Read-only mixed-Machine acceptance — 2026-10-08

## Scope and delivery state

This implements the bounded entry in [mixed-task-entry.md](mixed-task-entry.md),
on merged prerequisite `3bee6689`. It is one explicit source-owner program in the
existing Agent Machine, not a general composer or input router. Local acceptance
is complete; current-head remote CI, merge and canonical synchronization remain
open. Automatic routing stays disabled.

`agent_work` now accepts `select_owner` JSON through its existing namespace
executable and supports `result`. The versioned `owner-work-v1 ` control enters
the ordinary durable input queue. A client can inspect the read-only
`/agent/root/machine/work` projection. Neither ordinary JSON-looking prose nor
input-shadow advice selects the program. The live acceptance client attaches to
the Root of a freshly launched native Alan in Herdr; it does not launch a second
executor or bypass Root ownership. Host bash `!agent_work` is not a qualified
entry for this namespace executable.

The Root evaluator must already be explicitly granted at launch. The current
`--shadow-evaluator typesafe-main` selection captures that authority once and
shares it with independently submitted explicit work. Request profile names do
not look up or grant ambient channel capabilities; children do not inherit the
capture. Connection default remains `chatgpt-main`, `gpt-6.1-sol medium`.
`typesafe-main` uses the managed Host credential and pinned `jev-1.13.0`.

## Frozen comparisons and native receipts

Local artifacts are under
`/Users/morris/Library/Caches/Alan/mixed-machine-entry-20261008`.
The six source files and their hashes are those frozen in the entry document;
all were rechecked unchanged after final acceptance. Inclusive ranges are:
HostFS `lib.rs` 41–45, 101–118, 292–313, 438–455; Kernel `mountfs.rs`
564–584, 770–790; Service Manager `host_mount.rs` 582–620.

The original v1 generation run's failed reads and 60-second unresolved wait
remain failures, with its later valid continuation recorded separately. The v2
comparison froze identical explicit descriptors before either run. Native
generation rollout `24191433-f1c0-4480-9911-3d4d67cc3401` produced valid HostFS
JSON after two generation responses (one seven-read Tool batch and a final
answer). Its archive SHA-256 is
`9f40cca1d75dbc18f5000d354441755273cf5c583c1ff0a74487ad7332e3c247`.
The observed 13.236-second duration is a single-run diagnostic. Generation used
ordinary TTY admission; explicit mixed work used aP control through LocalAttachment.
These are different admission surfaces, not a matched latency benchmark.
The v2 prototype predates final capability/cancellation fixes and is retained
as prototype evidence only.

Final production candidate v4 binary SHA-256:
`1ba6fc24569449454d65702f0eb8828946aed02fb94d9e6f5d2e7d9278a7dc1d`.
Its pre-dispatch source manifest SHA-256:
`b26dde7c623e018e34748baa8cbf3186341b659617f16be3a3e3d4af9adf9b91`.
`explicit-descriptors-v4/manifest.json` freezes controls, binary and source;
`client-correction-manifest.json` records later acceptance-client-only corrections.
Production source did not change between that freeze and the following runs.
The manifest's `client_sha256` is the executed test binary digest, not its source:
`8e4ebdf890eeed941472052dbe884c9b91f40ac7ba14f7112c09e6ce2cd83526`.
The final helper source digest is
`6ddaaa707bb462bc41f736db61d3ea258c0fe08b84ba19c8476407a7f96caf34`.
`final-client-receipt.json` and archived helper source/binary retain both identities;
the earlier and final executed binary hashes agree.

| Case | Actual native observation | Calls and boundary |
| --- | --- | --- |
| Semantic owner, v4 | Work `07b0fb25-1fc6-429c-80f3-4aae2ff4e26b` completed with `hostfs` and four verified source citations | One real TypeSafe evaluation, zero generation. Evaluation 1244 ms, 1904 input/53 output tokens; cost unknown. Client test passed in 1.56 s. |
| Literal declaration, v4 | Work `b9db334a-5953-4810-be3d-7a5192389878` completed in the same Root after the semantic task | Zero evaluation/generation; client test passed in 0.29 s. Literal recognizer is restricted to line-oriented public Rust struct/enum/trait declarations, not a Rust parser. |
| Ungranted profile, v3 | Work `a7ca7121-9d72-4112-9d2d-0e4f40c05fe6` failed outside captured Process authority | Zero evaluation/generation; a globally configured profile did not grant access. |
| NoMatch, v3 | Work `f6c156b5-84ce-4927-bead-e338527b7aff` waited on owned request `r0` with `generation_budget_unavailable` | One real evaluation, zero generation; subscription billing unknown cannot satisfy the hard fallback ceiling. |
| Normal exit while waiting, v3 | Existing Host stop cancelled the owned wait; explicit recovery retained Cancelled | This is normal-exit cancellation, not crash recovery. The initial test's Waiting expectation failed and is retained. |
| Crash then explicit recovery, v3→v4 | Work `3d190424-81ac-45d7-aaf0-42b513cf333c` retained original source rollout `40d776c4-b249-4388-84ea-f84b7629f438`, request `r0`, and Waiting | Exact owned native PIDs were stopped only after durable Waiting. No evaluator repeat or generation; no recovered Host grant. Final v4 UI activity is paused. |
| Reauthorize and answer, v4 | Explicit public Host control granted the same project ReadOnly; typing `hostfs` answered the existing single-select request and completed the same work | Source ranges were reread under current rights. One spent evaluator attempt remains, zero generation, no final assistant prose. |
| Terminal recovery, v4 | A fresh explicit resume retained the literal Completed result; corrected client verified absent Root project mounts before explicit reauthorization, then `/mnt/project-request-1 ro` | No new work, source dispatch or model call. Client passed in 0.02 s. |

Final v4 native Process PID 3644, Root `/proc/8`, rollout
`717a1cf1-ea4d-4fae-b2a2-dc90a6378c5b`. Archived
`explicit-descriptors-v4/native-final-rollout.jsonl` SHA-256:
`560e544c08e8e90d22a3df8c50d805d5b5706a661917cc12a9517c5b88056a16`.
The three completed work snapshots retain exact source-range bytes/digests,
zero generation, and zero assistant prose messages. Empty assistant Tool envelopes
remain ordinary Tape evidence, not generated answers. `final-acceptance.json`
records the final source/digest recheck and public projections. Terminal recovery
used native PID 10804. Both owned invocations exited normally afterward and the
owned pane `w58:pX` was closed; no other pane or service was stopped.

All timings above are diagnostics, not p50/p95 or cost-benefit qualification.
Successful priced fallback is fixture-only: production Connection quote defaults
to unavailable until verified pre-dispatch billing can bound the whole request,
including reasoning. Unknown costs remain null rather than zero.

## Failures preserved and corrections

- The normal-stop recovery test expected Waiting and failed; durable evidence
  correctly showed Cancelled. A separately frozen crash case tests retained wait.
- First recovery UI displayed ready with a pending request. Machine recovery now
  restores Paused, and startup projects pending interactions as paused. Final v4
  native acceptance verifies that state.
- First reauthorization helper incorrectly read a Root source through the
  processless client namespace. The grant succeeded, but that assertion failed.
  The corrected helper inspects the actual Root `/proc/<pid>/namespace` before
  and after the same public Host operation; terminal recovery verifies it.
- Raw JSON entered into a native single-select field was rejected as a value;
  typing `hostfs` produced the native keyed response. The first observe-only
  completion helper also stopped at Waiting too early. Both failed logs remain;
  the corrected helper waits for the expected state and passed on actual completion.
- Independent review found cancellation could hide uncertain model settlement,
  ordinary input could be reported Completed on generic cancellation, and a large
  async test harness was placed inline. Shared cancellation now preserves unknown
  settlement for Interrupted recovery, correlates input cancellation, and the
  public executable's tests use an adjacent suite. Both review axes rechecked fixes.

Pending response text retains literal prefix precedence. The reauthorization
above uses the existing programmatic Host control, not an ordinary TTY `/project`
flow while a request is pending. That user-flow limit is not claimed as solved.

## Verification and remaining gates

- `just test`: 2,847 passed, zero failed, 14 ignored across 97 test summaries.
  Final-source log: `~/Library/Caches/Alan/mixed-owner-delivery-tests.log`.
  This rerun includes the Paused startup correction and final acceptance helper.
- Focused fixtures cover literal/typed completion, NoMatch, unknown/over-budget
  billing, one priced fallback, revoked sources, original wait/response identity,
  cancelled and unacknowledged starts, stale terminal responses, malformed modes,
  missing captured authority, shadow coexistence, actual generation abort without
  retry, and cancellation with confirmed versus uncertain model settlement.
  `mixed-owner-ui-recovery.log` records all 13 passing focused cases.
- AgentFS tests verify read-only bounded projection and restored request identity
  without overwriting an accepted response; executable tests distinguish admission
  receipts from completed work. Existing full-workspace effect reconciliation
  coverage passes; this slice introduces only governed read effects.
- Final pinned OpenSpec 1.4.1 `just openspec-check` passed all 65 strict surfaces
  and its current-surface guards (`mixed-owner-delivery-openspec.log`). The
  mandatory commit hook runs the full quality/distribution gate for the staged
  delivery snapshot; remote CI still requires its own immutable head evidence.
  Prior full quality/distribution log: `mixed-owner-v4-quality.log`.
- Independent Spec and Standards reviews found no remaining issues on the final
  production/test diff after corrections. Neither reviewer substitutes for CI or
  provider qualification. Final delivery SHA and check receipts remain pending.

General Skill/package candidate exposure, arbitrary mixed programs, immutable
package-reference qualification and automated effect routing remain outside this
bounded entry. Task 2.4 and delivery task 3.3 stay open. Only implemented owning
deltas may synchronize after merge. Do not archive the whole change until its
unfinished roadmap and gates have a discoverable active owner.
