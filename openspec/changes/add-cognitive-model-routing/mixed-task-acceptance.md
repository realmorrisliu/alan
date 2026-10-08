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

## PR #1039 wait-ownership correction

Automated review of head `9a938265` reproduced one further failure boundary:
AgentFS request creation could succeed before durable wait persistence failed,
leaving an unowned pending prompt. The full-handler regression closes the actual
recorder in the test Connection's quote callback after NoMatch, and fails on the
original implementation because `r0/status` remains pending. The fix reuses the
existing request cancellation operation and verifies terminal owner evidence
before returning the persistence error; registration and Yield still follow only
acknowledged wait ownership. Cleanup failures propagate. Similar ordinary
interaction callers have no fallible wait-persistence step between request creation
and synchronous registration, so they do not receive unrelated changes.

Review also identified Waiting bytes surviving a failed flush. The Machine now
queues a matching acknowledgement witness in the same rollout only after the
Waiting persistence call returns success, and registers pending ownership before
another await. Recovery requires that correctly ordered witness to restore the
wait. Missing acknowledgement evidence becomes Interrupted, retaining attempts
and request identity without restoring a prompt. Later witness flush failure may
lose recoverability conservatively; it cannot falsely prove a Wait acknowledgement
that never returned. No sidecar or second evidence owner is introduced.

The owner-work suite now has 15 tests, including cancelled `r0`, no Machine
pending interaction or Yield, live unsettled work and evaluator 1 / generation 0.
The real `batch_failure_probe` covers both pre-write failure and complete Waiting
bytes written before acknowledgement failure; a fresh Machine restores neither
as an answerable request. Logs: `mixed-owner-wait-persistence-red.log`,
`mixed-owner-wait-persistence-green.log` and `mixed-owner-wait-ack-tests.log`.
Both independent review axes rechecked the correction. Native measurements above
retain their v4 binary/source identity; this later persistence-failure fix is
regression evidence, not a rewritten native measurement or new routing
qualification. The updated head requires fresh CI.

## Corrected-source native recheck, v5

The post-ack-witness production source was frozen before dispatch in
`explicit-descriptors-v5/manifest.json`. Binary SHA-256:
`d8b10d0a438830bfc8b3c415958f5fb3c3cd945c85b487abac447df73a00fdd5`;
source manifest SHA-256:
`d3e2af7975466e9fbc5fcb12c1e52ba066f45df68232a6f87b98cd16dea6bbd3`.
The source manifest and all six project hashes were rechecked unchanged afterward.

Fresh native PID 42425 produced real NoMatch work
`ee3bb22d-f6f4-444c-908a-5fd3209d9051`, request `r0`, evaluator 1 / generation 0,
and a matching acknowledged-wait witness in rollout
`befe52ff-5cc0-49c3-a316-7bd1b8b62c12`. After stopping only that owned process,
fresh explicit resume PID 42844 restored the same wait and paused activity. The
client verified absent recovered Host project authority before explicitly granting
ReadOnly. Native single-select input `hostfs` completed that original work without
another model call; this verifies human selection plumbing, not implementation of
the intentionally missing Windows API used to induce NoMatch.

The same recovered Root then completed semantic work
`c62e4acf-1f15-4c46-b1ac-91407799d39e` with one real evaluation / zero generation,
and literal work `6e83fce5-d2ce-4687-b878-7785521051d6` with zero / zero.
Client tests passed (waiting 1.51 s, recovered wait 0.02 s, semantic 1.51 s);
these remain diagnostic timings. All citations were rechecked against exact
source ranges, with no assistant prose. The final native rollout
`ec76d52a-e06e-4831-94c5-14758c85fea2` archive SHA-256 is
`778ba936f9bdf69f3845412027d000d61e3d41438b37124723d4f44f85f6fef2`.
`pre-crash-rollout.jsonl`, `crash-receipt.json` and `final-acceptance.json`
retain witness, identity, attempts and source bindings. An initial copied-binary
executable-mode mistake prevented startup; the no-socket test failure is preserved,
and mode was corrected before any task/model dispatch. No submitted task or model
attempt was repeated. Native exited normally after completion; owned pane `w58:pY` was closed.

Updated local verification: full workspace 2,849 passed, zero failed, 14 ignored
across 97 summaries (`mixed-owner-wait-ack-workspace.log`). The subsequent
white-box suite extraction and extra witness-identity assertions pass all 15
focused tests (`mixed-owner-wait-ack-extracted-tests.log`). Production code is
unchanged from that complete run and v5 freeze. Pinned strict OpenSpec still
validates 65 surfaces; the staged commit hook and updated remote CI remain required.

## Control transport envelope correction

The next automated review found that valid `owner-work-v1` controls could be
encoded within the 64 KiB protocol bound but rejected by the former 8 KiB
AgentFS `machine/ctl` limit. The public `agent_work select_owner` test reproduced
this with an 8 KiB question before correction (`mixed-owner-ctl-size-red.log`).
AgentFS now admits one complete nonempty UTF-8 record through 64 KiB, retaining
the existing CR/LF/NUL rejection and runtime-owned semantic validation. The Shell
offers the complete buffer; the imported aP transport chunks at 128 KiB, so a
valid owner control remains one write. The runtime event reader reads the full
record and preserves its UUID.

Related crate suites passed 290 tests, zero failed or ignored
(`mixed-owner-ctl-size-green.log`), including exact 64 KiB acceptance, overflow
and invalid records leaving events unchanged, and a public maximum-question
submission producing exactly one complete control event without `io/input`.
All three runtime selector tests passed (`mixed-owner-ctl-size-selectors.log`);
pinned strict OpenSpec again passed 65 surfaces (`mixed-owner-ctl-size-openspec.log`).
Both independent review axes passed the correction. The preceding full workspace
run and v5 native artifacts retain their own source identity; this later transport
fix has focused regression evidence rather than a repeated live model measurement.
The final commit hook and current-head remote CI remain required.
