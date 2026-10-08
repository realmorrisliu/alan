# Read-only mixed-Machine entry — 2026-10-08

## Status and entry gate

The user-authorized ordered continuation selects this one task after the
reliability stage. PR #1038, reviewed head `bbc407c5`, merged as `3bee6689` on 2026-10-08.
All 16 head checks passed and automated review completed without findings.
Post-merge CI run 37733031263 passed on that merge SHA; CodeQL run 37733031260 and Security run 37733031265 also passed. This activates only the bounded runtime work
below. The reliability receipt records those final checks. Bounded implementation
and native acceptance are now recorded in [mixed-task-acceptance.md](mixed-task-acceptance.md);
current-head CI, merge and canonical synchronization remain open.

This entry belongs to `add-cognitive-model-routing`. It does not activate the
rest of [next-planning.md](next-planning.md) or qualify input auto-routing.
`--shadow-evaluator` retains its advice-only contract; it must not become a
silent live-work selector. Its explicitly captured Root evaluator is also available
for an independently submitted `owner-work-v1` control; request profile names grant
no Connection authority, and children inherit neither the capture nor shadow setup.
No new global router, executor or Kernel type is needed.

## Task and immutable input

Identify which existing crate owns one described behavior, and return a
structured owner plus namespace citations. The project is a six-file read-only
snapshot of the reviewed prerequisite source. It contains no credentials,
Generated runtime evidence, installed-package mutation, or unrelated project data.
Runtime/store evidence remains outside this Host project.

Frozen semantic case `owner-revoked-fid-v1`:

> Which crate prevents an already opened Host directory file handle from reading
> or committing a buffered edit after its Host Mount grant is revoked?

Candidates are `hostfs`, `kernel`, `service-manager`. Expected semantic owner is
`hostfs`; Service Manager triggers revocation, and Kernel mount routing alone is
not the owner of invalidating old HostFS fids or dropping buffered writes.
The literal deterministic control is `Which crate defines HostDirFs?`.

Source commit: `bbc407c5e09635d43e865799c1889be9294f75a9` (reviewed PR #1038 source, now merged).
Runtime source manifest: `33a8bc12e3b3b2fb84aa8bf79ccd5609db4eb2688f3822172442fcb8b3c704a8`.
Native release binary: `73c324a62f499c908188c35af0e3e9ecc5ed2b74cd6be481274aa15db16d3979`.
The following source bytes must be identical for each baseline and mixed run.
Changes require a new candidate/version; do not rewrite an observed failure.

| Project file | Bytes | SHA-256 |
| --- | ---: | --- |
| `crates/hostfs/Cargo.toml` | 358 | `56ec6a2f895182d1f9ab6846668336886c2bc43ad999eef790f64ad617b909b4` |
| `crates/hostfs/src/lib.rs` | 33099 | `2e43b6e7e8e48025ed96d106552323107398d8dd4179320577e68d5743c0d8b6` |
| `crates/kernel/Cargo.toml` | 439 | `7d3a07ee14cb1157f19e968b56573058d64b1436082d2cc03b3c150fd1d2b117` |
| `crates/kernel/src/mountfs.rs` | 34075 | `d8b012e708f4a873a5123ea920ea12c3839182ef7d3279659f81254440efa6c1` |
| `crates/service-manager/Cargo.toml` | 1060 | `24914dad1bd29c9cb5f2babecfcf95e9d2545ea80f19045a40a2be46855d32cb` |
| `crates/service-manager/src/host_mount.rs` | 30264 | `7b5796c0cc4e59b4c57beedd230057961f406fc86245a866fed2acd5c6621812` |

Only explicit source descriptors within the authorized project may populate a
candidate. Resolve mount rights and file visibility before sending evaluation
input. Absence/revocation is unavailable, not an empty file or model abstention.
Candidate IDs select evidence already available to the owning Process, not global
Tools, grants or profiles. Installing a definition/Skill supplies no extra authority.

## Baselines observed before implementation

Evidence directory: `/Users/morris/Library/Caches/Alan/mixed-machine-entry-20261008`.
`baseline-manifest.json` SHA-256:
`59634e4165697ee22ab5f4fdf3ebb52b8c034d709bb2ea412774fefe5592ab1c`.

| Baseline | Actual boundary | Observation | Limits |
| --- | --- | --- | --- |
| Deterministic | Offline stdlib reference over the supplied source | Exact public struct/enum/trait token with one owning crate selects it; otherwise NoMatch. Literal `HostDirFs` selects `hostfs`, semantic case returns NoMatch. Zero provider dispatch. | Reference algorithm only; not native Machine acceptance. No semantic guessing. |
| Existing generation/Tool | Native bare Alan in Herdr, dev `chatgpt-main`, `gpt-6.1-sol medium`, `/project` read-only | Two failed reads (`.` and absent root `Cargo.toml`), then a real StructuredInput request for source paths. Explicit response resumes the same Machine; five source reads then valid owner/citations JSON. Nine assistant generation responses in the durable Tape, no project mutation. | Initial submission did not produce completion within the frozen 60s collection window. The observed continuation is retained separately, not a retry or a latency pass. Subscription per-call billing remains unknown. |

Generation evidence: rollout `a1f83f70-de88-4bc2-bc87-200bc2de9227`, `/proc/8`.
`generation-rollout.jsonl` SHA-256:
`360190a5d3db9f2f9f7eed4e0de1cc8f6714b58d367975f661e7de5d17f6c586`.
`generation-wait.terminal` and `generation-completed.terminal` retain actual UI.
The startup probe with default low effort was closed before model dispatch; it
is not a second attempt. The configured fresh invocation's durable header confirms
medium. The owned pane `w58:pS` was closed after `/quit` returned to its shell.

The resumed answer correctly cites HostFS `lib.rs` lines 101–118, 292–313 and
438–455; all ranges and source hashes were checked. This is JSON in assistant
text, not structured-only Machine completion. The missing project-discovery
information is an observed baseline limit, not a runtime fix authorized by this
entry. A later comparison supplying explicit evidence descriptors must supply
identical descriptors to both baselines and freeze a distinct case before calls.

## Mixed transition and result contract

Use the existing Agent Machine and its accepted submission identity:

1. Validate the explicit task/descriptor boundary, current rights, candidate
   membership, document sizes and budgets. Resolve deterministic read-only source
   evidence through the existing Tool/Process namespace path.
2. Exact unique literal ownership may complete without any model call. Otherwise
   submit one finite-choice evaluation through the separately captured reachable
   Connection using the existing `choice.v1` allocation/commit/result lifecycle.
3. Valid Selected supplies advice for a member of the captured set. Deterministic
   validation must bind the selected owner to real retained source/citations before
   recording completion. Confidence never grants a read, write or Tool permission.
4. NoMatch may enter one explicitly allowed generation fallback. Unavailable,
   malformed, timeout or uncertain evaluation cannot silently become successful
   selection. Use a typed failure/wait or the explicitly recorded fallback policy.
5. Unresolved selection or missing user context uses the existing
   Confirmation/StructuredInput request and response identity. Request response
   resumes the same work, not another ordinary input or a new evaluator attempt.
6. Completed result needs no final prose/model call. Publish a bounded read-only
   Machine work snapshot after acknowledging its durable terminal event. Process
   stays governed by `/proc`; completed work does not imply Process exit.

The result is a version-1 bounded document with submission/work identity,
completion state, an optional validated `owner`, and citations containing namespace
path, inclusive line range and captured content digest. Waiting carries an owned
request reference; failed/cancelled/interrupted remain distinct from completed.
Result ownership is Machine → rollout/checkpoint → read-only AgentFS projection.
Use the existing `machine/evaluation` observation for evaluation provenance;
`actions/<id>/result` remains Tool completion metadata, not a second work history.
Define the precise work projection and control DTO in the owning deltas before
writing runtime code. Do not add a public endpoint simply to manage test evidence.

## Frozen budget policy and fallback boundary

Initial ceiling: one evaluator attempt, at most one additional generation
fallback, 30,000 ms active-work deadline per attempt phase, no implicit retries.
Keep captured source content <=128 KiB, each candidate citation descriptor document
<=4096 bytes, each candidate evidence projection <=4,096 UTF-8 bytes, total task document
<=64 KiB, <=16 candidate IDs, and completed result <=8 KiB. Record truncation and
keep exact full source references; omitted bytes cannot be invented as evidence.
Human wait remains explicit and is recorded separately from active model latency;
resume does not replenish spent model attempts or monetary allowance.

Fallback monetary ceiling: 1,000 micro-USD from verified billing/quote provenance.
Quote provenance is nonempty and <=1024 UTF-8 bytes. If its maximum cost cannot be established before dispatch, fallback is budget
unavailable and waits/fails without calling generation. Unknown subscription cost
is not zero. The existing generation baseline remains a diagnostic with unknown
cost; it does not prove a hard monetary bound or provider benefit. Exercise a
successful budgeted fallback with a priced fixture and a real unavailable-cost
boundary; record live fallback success only if actual bounded billing is available.

Once cancellation is accepted, no next evaluation, generation or effect dispatch
may start. Allocation/start/terminal acknowledgement uses the existing durability
barriers. Recovery preserves operation, source and request identities; interrupted
or uncertain model work is not repeated. Unknown Tool effects use current
reconciliation and cannot be replayed by reevaluating the decision.

## Acceptance and closure

- Same native Shell/LocalAttachment/Root entry as the baselines; no renderer
  execution bypass or fixture-only product mode counted as shipped behavior.
- Deterministic completion, typed Selected/NoMatch, unavailable authority,
  malformed result, timeout, cancellation, explicit wait/resume, recovery across
  each durability barrier and finite fallback. Assert calls, identities and file
  effects, not only messages. Cite shared owner tests for unchanged effect fences.
- Structured-only completion and a successful later task in the same live Process;
  projections reject external writes and do not manufacture an assistant answer.
- Freeze a source/binary-bound real mixed run after implementation. Preserve the
  existing failed/blocked first baseline and all additional attempts separately.
- Focused tests, strict OpenSpec, quality, independent review, current-head CI,
  merge and implemented-only spec synchronization remain delivery obligations.
  Fixture timing/cost is not provider qualification; routing remains off and its
  fresh baseline comparison belongs to `qualify-agent-input-routing`.
