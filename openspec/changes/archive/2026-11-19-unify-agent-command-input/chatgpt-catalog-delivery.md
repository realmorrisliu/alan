# ChatGPT catalog delivery — 2026-10-07

Status: final candidate 3 passed native acceptance, final-tree quality and both
independent review axes. PR #1033 subsequently merged as
`b93501aa7206dd9ef70cb6f4978680784cd19ee4` from head
`a239021e78af816203ef5c43164a3020f8724a57`; all reported checks succeeded.
See [current delivery reconciliation](next-delivery.md#current-delivery-reconciliation--2026-10-08)
for the remaining full-task boundaries.
Owner: task 2.20, canonical provider-connection-contract and
provider-request-controls. Baseline: main `59855ebe`.

## Observed cause

`Config::effective_model_info` always returns `None` for ChatGPT.
`ProcessConnection::catalog` requires that information before projecting any
managed models; `capture_model(Some(model))` requires it again before accepting
selection. The current failure is therefore deterministic even when the managed
ChatGPT callable can generate successfully. The bundled catalog covers three
OpenAI API families, not this account-scoped managed surface.

## Smallest owned implementation

- Extend the existing provider/client boundary to obtain optional model metadata.
  ChatGPT uses its existing managed-auth adapter, expected-account check and
  401 refresh behavior. Do not read Codex caches or expose credentials.
- Decode the actual authenticated `GET models?client_version=...` response at the
  provider adapter. Bound request time and response size; reject malformed or
  inconsistent model/reasoning metadata without logging raw response bodies.
  Protocol reference: [OpenAI Codex models endpoint](https://github.com/openai/codex/blob/main/codex-rs/codex-api/src/endpoint/models.rs).
- Connection Service publishes metadata scoped to the exact profile/credential
  publication. Reuse the resolved model catalog for both selection validation
  and request-control resolution; do not add a renderer-owned list.
- Frequent status observations only read published state. Fetch/retry belongs to
  explicit Connection/capture boundaries, with bounded failure and no repeated
  network work in the renderer polling loop. A catalog outage must not destroy
  a functioning original callable or pretend selection succeeded.
- Preserve exact-original recovery when metadata is unavailable. A changed-model
  selection needs current authority; unknown models fail without changing the
  last confirmed binding, profile default or previously captured queued inputs.

## Acceptance before closure

- [x] Adapter checks cover authenticated success, one 401 refresh, bounded failure
  and malformed/unsupported reasoning metadata, without leaking auth material.
- [x] Connection checks prove projection and validation share metadata, repeated
  observation performs no IO, and publication replacement removes stale authority.
- [x] Existing original-binding recovery and queued-binding regressions pass.
- [x] In an isolated native Alan invocation, load the real account catalog and
  switch to another actually listed model; correlate the confirmed binding and
  subsequent real generation rather than trusting a changed UI label.
- [x] Attempt an unavailable model and prove the previous binding still serves
  subsequent work. Restore the user-requested `gpt-6.1-sol` / `medium` binding.
- [x] Admit work under A, select B, and prove the previously queued work uses A
  while subsequently admitted work uses B. Record exact input and binding IDs.
- [x] Run focused checks, full quality and independent review.
- [x] Record current-head remote CI separately from local acceptance (merged #1033, head `a239021e78af816203ef5c43164a3020f8724a57`, all reported checks succeeded; merge receipt above).

No automatic routing, profile-default mutation or UI layout changes belong to
this slice.

## Real endpoint findings

Authenticated read-only probes on 2026-10-07: Alan release version `0.1.0`
returns HTTP 200 with zero models; omitting `client_version` returns HTTP 400.
The explicitly qualified Codex catalog protocol version `0.159.0` returns ten
account models, seven visible, including `gpt-6.1-sol`. The adapter pins that
wire compatibility version independently of Alan's application version.
The real metadata includes `max` and `ultra`, requiring canonical enum support;
unqualified Anthropic, Gemini and OpenRouter projections reject those efforts.
Evidence: `~/Library/Caches/Alan/model-catalog-version-probe.log` (metadata only).

The first native candidate failed to publish a catalog and was exited normally.
Its initial unknown queue state is not evidence of a deadlock: the second
candidate also started with unknown queue state before its first input. The
first candidate is not model-switch acceptance. Failed discovery is now recorded for the current
publication so startup/observation cannot loop on it; explicit model selection
can retry. No renderer catalog or Codex cache is used.

## Candidate 2 native acceptance

Evidence directory: `~/Library/Caches/Alan/model-catalog-20261007/`.
`build.json`, `source.diff` and `new-source.json` pin the candidate; test-only
inspection/controller source is under `inspector/`, outside the product tree.
The old inspector could not decode the extended reasoning enum, so it was
rebuilt against this candidate and its no-attachment selfcheck passed. Earlier
`available:false` from that old inspector is not a product catalog failure.

Native Alan PID 51869, boot `bf8966ff-dc3b-4898-8ca9-5e8af84c8d78`, Root 8,
isolated Herdr session `alan-model-catalog-20261007`, pane `w1:p1`:

- The native `/model` picker lists seven account-authorized visible models.
  Selecting `gpt-6-astra` produced a correlated successful selection and a real
  `MODEL_SWITCH_OK` answer. Reasoning remained `medium`.
- The bounded test controller sends the same `select-model` machine control as
  the renderer, pinned to the native PID/boot/Root. Unlisted model selection
  `83967013-365e-40b3-8b65-7a3457d6c23d` failed; the selected binding was identical
  before/after. Subsequent real input `a4923d41-a97a-48b2-8c17-1645a4bf3e4a`
  ran with Astra and completed with `ORIGINAL_BINDING_OK`.
- An initial queue probe was cancelled by the tester while learning the existing
  chooser's explicit arrow-selection and draft-preservation behavior; its input
  `44e444af-979c-42ff-8cc5-7d9acae18107` never executed. It is not positive evidence.
- Definitive queued input `8c24b02e-ec32-419e-97ae-0568cff3e194` was admitted while
  paused under `gpt-6.1-sol`. The user-visible picker switched selected-next to
  Astra; the queue stayed paused with the same input ID and Sol binding.
  Explicit `/continue` ran that exact input with active Sol / selected-next Astra,
  producing `QUEUED_A_OK`. Later input `fa196a90-ab99-467d-9083-5019fd4f342d`
  ran with Astra and produced `NEW_BINDING_B_OK`. Both have one completed receipt.
- Selected-next was restored to `gpt-6.1-sol` / `medium`, with idle empty queue.
  `/quit` exited 0 and the native PID disappeared. The owned named Herdr server
  was stopped after Alan exit; the default session was untouched.

Correlations are in `rejected-model-request.json`, `rejected-model.json`,
`after-rejection-{active,completed}.json`, `queued-a-{admission,selected-b,running}.json`,
`new-binding-b-running.json`, `queue-model-complete.{json,ansi}` and
`restored-pre-exit.json`. `evidence-sha256.json` records evidence hashes.

Focused checks: 55 protocol tests, 200 provider tests, 139 Service Manager tests,
and 66 Engine model tests passed. Two additional bounded body/timeout tests passed
(462 total distinct checks). Candidate 2 full `just quality` passed. These results
precede the account/publication review fixes below and do not qualify their final tree.

Phase-3 usability observation: the model chooser requires explicit arrow movement
before Enter and retains `/model` as a draft after selection; make its interaction
hint clear without weakening correlation or input stability. During testing use
Ctrl-U for draft clearing; Ctrl-C can cancel an admitted input.

## Independent review follow-up

Spec review found that a profile without explicit `account_id` could discover a
catalog under account A and later create a new model callable under account B
without metadata changes. Standards review found that discovery held the global
callable lock during provider IO, blocking already-published observations.

Fixes:

- Auth owner supplies a non-secret account binding at client construction, even
  for an implicit-account profile. Every managed provider request checks that
  fixed account. A client constructed without a logged-in account stays
  unavailable until publication constructs a newly bound client; it cannot
  borrow a later login.
- Connection publication records the callable's account. Changed-model and
  explicit original-model selections must construct a matching-account callable
  before installation. An implicit account enters the durable binding revision;
  explicit-account profile revisions retain their prior encoding. Old implicit
  revisions lacking account identity cannot prove exact restoration and fail
  visibly rather than being assigned to whichever account is currently logged in.
- Discovery uses a per-publication standard-library OnceLock, populated outside
  the global callable mutex. Pending/failed publication does not cause repeated
  observation fetches; an explicit selection can replace a failed slot to retry.
  Profile replacement detaches its old slot, so a late response cannot overwrite
  the new catalog. Publication identity is checked before starting IO.
- New regression coverage exercises implicit A-to-B auth-file replacement,
  rejection before provider requests, exact-recovery account identity, readable
  old catalog/capture during a blocked discovery, and old completion after profile
  removal/republication. Secrets remain in their Auth/Host owners.

## Final candidate 3 acceptance

Evidence: `~/Library/Caches/Alan/model-catalog-reviewfix-20261007/`.
`build.json` pins the binary and staged source patch; `evidence-sha256.json`
pins snapshots, transcript and the read-only inspector. Native PID `66548`, boot
`418206d7-c0c0-4fe7-b569-52954a346c0b`, Root `8` were checked throughout.
The first launch used an overlong macOS socket path and exited before startup;
shortening only the fixture runtime directory allowed the qualified invocation.

- The real catalog exposed seven visible models. Selecting Astra produced
  `MODEL_SWITCH_OK`. Invalid selection `f41d362f-7918-4e2c-9607-3da80adead79`
  failed once, retained the exact Astra/medium binding and subsequent real work
  produced `ORIGINAL_BINDING_OK`.
- Pending input `e3752339-0b06-4565-b139-e9a44acf14db` captured Sol/medium.
  Selecting Astra left its admitted binding unchanged. After explicit continue,
  the active binding was Sol while selected-next was Astra; it completed once
  with `QUEUED_A_OK`.
- Later input `18858edb-7bcc-4e58-99c5-90ad79d89170` ran under Astra and completed
  once with `NEW_BINDING_B_OK`. The queue was empty and idle afterwards.
- Restored Sol/medium, then `/quit` exited with code 0 and the native PID was gone.
  The owned isolated Herdr test server was stopped. No profile default changed.

Final-tree checks: 18 Auth, 203 LLM and 141 Service Manager library tests passed;
five focused Engine catalog tests passed. Full `just quality`, including
standalone distribution checks, passed. Spec and Standards independent re-review
both passed against source patch SHA-256
`c247a9b8c5c7b4d77e4c22a2d4bf8bdcb4bd0d28b60621caab6a46f0a48f26df`.
Only this acceptance document changed after that frozen production-code review.

Strict OpenSpec validation reports the ten pre-existing long-requirement warnings
on base `59855ebe`; no warning concerns this provider delta. PR #1032 already
contains their text-preserving restructuring. This branch does not duplicate
those edits or claim strict validation passed before that prerequisite lands.


## Task 2.20 closure audit — 2026-10-07

Source `de003b4d80bc6d57ff10c5c3dafc10ae0f53d0d7` was inspected against
merged main `43700bb11e28bc22e3cb099009018e0a494e4e3d`. The generation
restore/capture contract and the Engine admission/recovery tests already exist
on main; this audit adds no production mechanism. The shadow branch's evaluator
capture is a separate opt-in capability, not the evidence for generation recovery.

| Task clause | Owning executable evidence and assertions |
| --- | --- |
| Capture binding and resolved controls before acknowledgement | `engine_model_binding_tests::runtime_api_and_file_admission_capture_callable_before_dispatch` checks both API and AgentFS inputs, durable binding/controls and actual queued model projection before dispatch |
| Earlier queued A survives selection B | `engine_model_contention_tests::observer_selection_preserves_active_guardian_and_admitted_callable` gates real Runtime calls, checks queued and active/guardian requests remain A with low reasoning, and new work uses B with high reasoning; native candidate 3 above independently exercises real ChatGPT A/B |
| Recovery preserves binding and stays paused | `engine_admission_recovery_tests::admission_recovery_follow_up_restores_a_once_after_continue_not_b` loads a durable A input with selected B into a new Process, checks no restore/model call before continue, then exactly one A request with original controls and no B request |
| Unavailable original callable never remaps | Its `follow_up_unavailable_never_falls_back_to_b` sibling requires failed correlated completion, no dispatch record, no A/B generation and durable queue removal; another recovery finds no pending input or retained binding |
| Orphan steering cannot execute after restart | Both `orphan_steer_*_rejected_before_dispatch` cases require rejection before dispatch, regardless of callable availability |
| Selection never resumes paused work | `engine_model_projection_tests::active_and_admitted_a_survive_selection_b_then_settled_pause` selects B through the real Runtime while paused and verifies unchanged queued identity, paused queue and no active model |
| Exact authority belongs to Connection Service | `connection::process_binding_tests::managed_none_catalog_exact_restore_preserves_full_authority` rejects changed model/provider/credential/revision/profile and unpublished callable; `catalog_tests::implicit_account_replacement_cannot_select_or_restore_with_old_catalog_authority` rejects old account identity after replacement |
| Failure preserves selection and does not silently continue | `engine_model_qualification_tests::selection_recorder_failure_retains_a_and_exact_pending_active_bindings` checks failed persistence retains A, captured/active bindings and no dispatch; settlement tests retain pause on unknown durable removal and never requeue after known removal |

Fresh verification: `cargo test -p alan-agent-engine model --lib` passed 68 tests;
`cargo test -p alan-service-manager connection:: --lib` passed 18 tests.
This includes the four durable recovery cases, actual request/control assertions,
selection contention, settlement uncertainty and retry/recovery boundaries.
Provider execution in these boundary tests is controlled Mock generation; real
provider model selection and queued-binding behavior are evidenced separately by
the native candidate 3 acceptance above. No new real-provider recovery claim is
inferred from those fixtures. Logs are in `~/Library/Caches/Alan/` as
`model-binding-closure-tests.log` and `model-connection-closure-tests.log`.

These mapped checks plus merged native model acceptance close task 2.20's
implementation and acceptance clauses. Canonical synchronization and the broader
unified-input lifecycle/authority matrix remain under tasks 4.2 and their owners.
