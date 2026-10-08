# Ordered delivery — 2026-10-07

Current continuation: the user-authorized [four-stage goal](continuation-plan.md)
starts from merged main `1c53cf52` on 2026-10-08. The completed sequence below is
historical delivery evidence; it does not close the broader reliability matrices.

User-authorized sequence: lifecycle closure, real model switching, small terminal
presentation improvements, then side-effect-free typed-routing qualification.
Codex implements and tests directly. Baseline: merged main `59855ebe`.
The archived self-development milestone stays closed and immutable.

## 1. Unified input and recovery

The initial apply checklist had 49 items: 27 checked, 22 unchecked.
After the directory-selection and model-binding closure audits it has
30 checked and 19 unchecked after the forced-termination acceptance below. This measures
recorded task closure, not implementation percentage. Broad parent tasks stay
open until every required boundary has evidence. Historical implementation notes
in design/tasks are dated slice reports, not a current absence-of-code inventory.

Initial source inventory:

| Boundary | Existing implementation/check | Remaining closure |
| --- | --- | --- |
| Foreground exit and independent invocations | CLI `host_cli_integration_test`; merged tasks 2.18.1–2.18.2 | Verify active Tool descendants end on actual exit; preserve completed effects and terminal restoration |
| Explicit recovery and non-replay | Host `explicit_command` integration; Engine `engine_recovery_boundary_tests` and `agent_machine_recovery_tests` | Correlate selected durable records, fresh boot/Process, paused pending IDs and effect counts in product acceptance; missing/invalid selection must fail |
| Paused directory selection | Engine `engine_directory_selection_tests`; TUI `file_backed/project_dispatch.rs` sends `project-cwd-v1` | Native chooser plus actual recovered queue acceptance; invalid/revoked selection keeps authoritative binding and paused queue |
| Lost replies and duplicate controls | `engine_directory_publication_tests`; Host `lifecycle/project_reply_loss.rs` | Verify unknown outcomes remain unknown and neither cwd selection nor revoked grants are recreated by retries |
| Herdr detach | Earlier extra-view detach; 2026-10-07 isolated last-view disconnect/reconnect below | Server-stop is distinct and is not claimed from view-disconnect evidence |
| Revocation | Host `local_project_mount_uses_the_root_process_and_revoke_removes_authority` | Queued command under revoked authority must fail before effects; fresh authorization and explicit continue remain separate |
| Broader input contract | Existing framing, FIFO, cancellation and path checks | Audit each unchecked task against its full matrix: same-Agent clients, cross-grant paths, Linux confinement and internal-command unknown commits remain open until verified |

Execution order:

- [x] Inventory all remaining checklist clauses against code, runnable checks and
  source-pinned terminal evidence; classify implementation gap, acceptance gap or
  documentation/merge closure. Do not close parent tasks from a narrow passing case.
  See [remaining checklist audit](remaining-checklist-audit.md); broad qualification
  gaps remain explicitly open even where their underlying implementation exists.
- [x] Run existing focused recovery/directory/authority checks before adding code.
- [x] Exercise actual exit, explicit resume, paused directory replacement,
  revocation and non-replay with disposable project data and exact effect counts.
- [x] Exercise Herdr view disconnect/reconnect in an isolated owned session;
  distinguish a retained process from actual exit and forced termination.
- [x] Fix only reproduced gaps at their existing owner, run applicable quality
  checks, then record exact review/CI/merge evidence and update owning checkboxes.

Current local checks (unchanged Rust source at `59855ebe`):

- `cargo test -p alan-agent-engine recovery --lib`: 54 passed, 0 failed.
- `cargo test -p alan-agent-engine directory --lib`: 20 passed, 0 failed.
- Logs: `~/Library/Caches/Alan/next-goal-{recovery,directory}-tests.log`.
- Added Host integration regression
  `paused_native_command_cannot_reuse_revoked_cwd_authority`: native work starts,
  a second command queues, interruption pauses it, Host Mount authority is
  revoked, and explicit continuation fails without the queued file effect.
  The completed earlier write remains; no model request occurs. This uses the
  ephemeral Host's path-guard adapter, not macOS sandbox product qualification.
  The associated existing explicit-command integration also checks recovered
  completed Action/output evidence and a once-only append after restart.
- A shutdown probe using `$$`/`$!` was rejected by that test adapter's expansion
  guard before native launch. It proves neither shutdown success nor a shutdown
  defect. Keep actual owned-descendant shutdown in the product acceptance matrix;
  do not weaken the sandbox to obtain a passing fixture.
- `cargo clippy -p alan-os-host --test explicit_command -- -D warnings`: passed.

### Product lifecycle evidence — 2026-10-07

Cache: `~/Library/Caches/Alan/lifecycle-20261007/`. Herdr 0.9.1 isolated named
session `alan-lifecycle-20261007`, pane `w1:p1`; default session untouched.
Binary SHA-256 `8572abd772ea14123ba64c591436326b25c835bf2a10b68b6ce80c06bdbd0201`
is the earlier verified product candidate, with unchanged production Rust relative
to this baseline. Current work adds tests and planning only. Connection status
was `gpt-6.1-sol`; explicit native commands made no generation request.

- Project was selected read-write through `/project`, with correlated directory
  confirmation. Alan PID 61248, boot `0849831b-025c-4241-9ed3-577aaa736674`.
- During `printf x >> disconnect-before; sleep 30; printf y >> disconnect-after`,
  the sole isolated view client (60896) received SIGTERM and exited cleanly (0).
  This tests view-client closure, not keyboard-detach: the synthetic PTY did not
  respond to the attempted prefix key. Named server and Alan were retained.
- With zero named view clients, Alan retained its PID/boot and completed the
  command. Files contained exactly `x` and `y`; reconnect preserved the result
  without repeating effects. Evidence: `disconnect-operation.json`,
  `all-views-disconnected.json`, `disconnected-complete.ansi`, `reconnected.ansi`.
- A new command wrote `z` then slept before a later write. External process
  inspection proved shell PID 61519 and descendant `sleep 60` PID 61520 belonged
  to Alan. Empty-composer Ctrl-D exited Alan with code 0. Alan and both native
  children disappeared, endpoint/status were removed, `z` remained, and the later
  file was absent. No manual Alan kill was used. Evidence:
  `exit-owned-descendants.json`, `exit.json`, `exit-verification.json`, `exited.ansi`.
- The named test server was stopped only after Alan's verified natural exit;
  its reconnected test client was closed separately. See `cleanup.json`.

This closes the bounded all-view disconnect and owned native descendant shutdown
acceptance gaps. Server termination and forced Alan termination are distinct.

### Product explicit recovery evidence — 2026-10-07

Cache: `~/Library/Caches/Alan/recovery-20261007/`, same pinned product binary.
Isolated Herdr session `alan-recovery-20261007`, pane `w1:p1`; the read-only
inspector SHA-256 is
`b17336791b4cbf6d372b60e3e40a19abcc39cce496fb4e0c4ba344aa0316cf65`.
Snapshots read the actual AgentFS queue, activity, directory Actions and grants,
with native PID/boot checks. Neither the inspector nor test code submits work;
all inputs and project authorization use the product terminal.

- Source PID 86718/boot `c75ddac7-5a31-4dad-b77b-d9596950fd4b` executed
  `printf E >> effects; sleep 60`, then accepted `printf Q >> effects` as
  submission `47fc96cb-665a-47ac-ad8b-808465d2411a`. Ctrl-C left Q paused;
  the file contained only `E`. Source quit naturally with exit 0.
- `alan --resume` created PID 88000/boot
  `ebe1bae4-f764-42b4-b3d1-976d520a4899`, retaining exactly Q paused and no
  grants. `/continue` was rejected for missing current project authority.
- `/project` read-write selection completed through the directory control
  Action while preserving Q and the paused queue. The file still contained E.
  Only another explicit `/continue` executed Q, yielding exactly `EQ` and an
  empty queue. This invocation also quit with exit 0.
- A second explicit recovery created PID 88215/boot
  `7d53c9c4-00a3-41ac-9e58-c33e96f4fcd0`: no grants, no pending work, effects
  still exactly EQ. It quit with exit 0. Recovered durable admission/dispatch
  records retained each original identity and exactly one dispatch for E and Q.
- `verification.json`, stage-named owner snapshots, selected rollout paths and
  extracted input events retain the correlation. `evidence-sha256.json` pins the
  artifacts. All three native PIDs were absent after their natural exits; the
  isolated Herdr server/client were subsequently closed.

This supplies current native chooser plus paused recovery acceptance for 2.19.2,
alongside the existing invalid/revoked/running control and publication regressions.
Parent 2.19 remains open pending the full missing/invalid-record, unknown-effect
and discovery audit and final review/CI/merge. Do not infer completion of other
unchecked input/path/multi-client matrices.
- Initial strict OpenSpec validation failed on ten pre-existing requirement-length
  warnings (>500 characters), with no error-level findings. This slice preserves
  every original clause in scenarios beneath concise requirements. Strict
  validation now passes with no issues; independent review confirmed all ten
  original bodies remain verbatim modulo whitespace. Before/after evidence:
  `next-goal-openspec-validation.json` and `next-goal-openspec-after.json`.
- Root recovery owner checks: `cargo test -p alan-service-manager root_recovery
  --lib`, seven passed, including missing/invalid selection, concurrent instance
  isolation and preservation of a damaged selected source on failed boot.
- Two-axis source review found an asynchronous test race and an overly broad
  failure assertion. Both were fixed: wait for exact known queued identity and
  settled pause, then require the actual revoked-authority diagnostic. The
  sibling interrupt/continue/discard test now uses that same boundary instead
  of a fixed delay. Both reviewers passed the fix; the two Host integration
  tests passed afterward. Full `just quality` passed, including standalone
  installer/release checks; log: `next-goal-lifecycle-quality.log` in the cache.
  This bounded lifecycle slice was delivered by PR #1032; see the current
  delivery record below. Broader parent tasks remain open.

## 2. Real Connection model switching

Task 2.20 owns admission/binding behavior. Existing model/admission recovery tests
are implementation evidence; the last product run's unavailable catalog is not a
successful live switch. Trace the current profile through catalog publication and
selection before changing provider or renderer code.

Acceptance must record real Connection-confirmed A→B selection, failed selection
retaining the prior binding, and an already-admitted A input executing on A after
selection of B. Check request model identity, not only the status-line label.
Retain existing credentials and avoid exposing secrets in evidence. Resolve the
catalog failure through its existing owner; do not invent a renderer model list.

2026-10-07 diagnosis: the active profile is `chatgpt-main`.
`Config::effective_model_info` explicitly returns None for ChatGPT, while
`ProcessConnection::catalog` requires that metadata before listing choices.
The bundled catalog has only the three OpenAI API families; therefore generation
can succeed while ChatGPT model selection remains unavailable. The existing
managed-no-catalog tests deliberately preserve this failure behavior. This is a
missing provider catalog integration, not a TUI refresh defect. The ChatGPT
adapter already owns authentication and account binding. Follow the upstream
[Codex model endpoint contract](https://github.com/openai/codex/blob/main/codex-rs/codex-api/src/endpoint/models.rs)
through that adapter and publish validated metadata through Connection ownership;
do not treat another application's local model cache as Alan authority.

## 3. Small terminal presentation slice

Use a separate active OpenSpec change under the canonical TUI contracts once the
first two stages close. Scope: fenced-code language label and quiet boundaries,
answer spacing, and model/status prioritization in narrow terminals. Reuse the
current renderer. Verify streaming/incomplete fences, CJK/wrapping, resize,
composer stability, completion below input, and usable terminal scrollback.
Compare fixed text at matching sizes; preserve already-qualified input behavior.

## 4. Typed evaluation and shadow routing

Generic capability remains owned by `add-cognitive-model-routing`; routing corpus,
measurement and activation remain owned by `qualify-agent-input-routing`.
Read their current dispositions and entry gates before implementation. Freeze
labeled cases and numeric false-execution, latency and cost thresholds before
measurement. Compare deterministic and generation baselines, including quoted
commands, discussion, ambiguity, invalid output, outages and explicit overrides.
Shadow decisions have no execution authority and launch no Tools. Automatic
routing remains disabled; a later explicit activation decision requires passing
evidence. Do not add a second execution manager or a speculative adapter framework.


## Current delivery reconciliation — 2026-10-08

GitHub state was rechecked after the four-stage implementation merged. The ordered
lifecycle, model, UI and shadow slices have merged; earlier pending descriptions above
are dated investigation records, not current missing implementation.

- Lifecycle PR #1032: head `4e9c2877baf2d14639d2ecc7140ef0f6af5983c2`, merged as
  `43700bb11e28bc22e3cb099009018e0a494e4e3d`. All reported checks succeeded.
  Independent local Spec/Standards review is recorded above; GitHub has no
  submitted review object for this PR, so no GitHub review approval is claimed.
- Model PR #1033: head `a239021e78af816203ef5c43164a3020f8724a57`, merged as
  `b93501aa7206dd9ef70cb6f4978680784cd19ee4`. All reported checks succeeded;
  independent local reviews and native model/queue evidence are in
  [catalog delivery](chatgpt-catalog-delivery.md). The real catalog is available;
  successful switch, failed switch preserving binding and queued A followed by B
  were accepted. Task 2.20 is now closed by the subsequent recovery/unavailable-
  binding clause audit in the catalog delivery record; canonical sync remains open.
- UI PR #1034 merged as `157d937ea3e2e3660783deae17ff6b86b536b9b7` from head
  `fb6bd45d1cd917c9e8c47943c018f58522995b66`, with 16 passing head checks and
  completed automated review. Source-pinned native acceptance and later header
  regressions are distinguished in its owning delivery record.
- Shadow PR #1035 merged as `cb0ec7ec7746a8f5b2af77782b281bbce7a838f0` from head
  `ac0a8af5b200dcd26c95accd3a6032116e919cce`. Its PR head had only a label check;
  combined main acceptance subsequently passed all 15 reported checks, including
  full CI, CodeQL and Cargo Audit. See the
  [typed delivery receipt](../add-cognitive-model-routing/typed-entry-delivery.md).
  Delivered-only canonical synchronization is in PR #1036 and remains unmerged.
  The
  [qualification report](../qualify-agent-input-routing/shadow-qualification.md)
  now records the native candidate, generation and deterministic prefix baselines.
  Both frozen candidates fail the command-safety/classification gates; later
  reviewed source is unqualified rather than inheriting their scores.
  Redirected response admission and complete cost/latency qualification remain
  open; automatic execution is disabled.

No broad same-Agent-client, cross-grant, retention, Linux confinement or unknown-
effect requirement is closed by this reconciliation. Keep those existing owning
checklists active; the bounded user's four-stage progress is not a claim that
all historical unified-input requirements are delivered.

## Recovery uncertainty display audit — 2026-10-07

Task 2.19's discovery wording does not require a history browser or arbitrary
rollout picker. The accepted `alan-shell` scenario already specifies `alan --resume`,
and CLI help exposes that opt-in entry. The selector owner validates the selected
file and pins the instance's source; a missing list is not itself a product defect.
The remaining audit focuses on exceptional recovery, not adding a new registry.

A real Runtime regression found that persisted Unknown effects survived recovery
without replay, but startup initialized the UI notice to empty. The new assertion
failed with `None` instead of `Warning`. Recovery now projects the Machine's latest
effect index into the existing warning snapshot and notice event before Ready:
unknown outcomes were not replayed and their effects must be checked before retry.
Known outcomes and fresh invocations retain the existing empty notice. A later
acknowledged result supersedes an older Unknown record, so old uncertainty alone
does not produce a warning. New ordinary work retains the existing notice-clearing
behavior; this does not introduce new execution, authority or recovery policy.

The full Agent Engine suite passed 1,374 tests with one existing opt-in test ignored;
independent Spec and Standards reviews passed. Evidence is in
`~/Library/Caches/Alan/recovery-unknown-notice-{red,tests}.log`; the extended
latest-result regression is recorded in `recovery-unknown-latest-test.log`.
This proves the Runtime presentation boundary only. Parent 2.19 remains open for
forced-termination product acceptance and full exceptional-startup correlation;
normal exit, existing no-replay tests and this notice are not substitutes for it.


## Forced termination and task 2.19 acceptance — 2026-10-07

Clean source `7b7b1692c878e350bdf6a535c5c64505467e40ef`, release binary SHA-256
`a14595b771bf43fa88d480016d7006fcc8638e44c2bb7dccb6e323b3b2a3587b`,
was run as the actual product CLI in owned Herdr session
`alan-forced-recovery-20261007`, pane `w1:p1`. Evidence lives under
`~/Library/Caches/Alan/forced-recovery-20261007/`; build/identity, raw rollout
copies, AgentFS snapshots, ANSI captures, verification and hashes are retained.

The first native command wrote `U`, then waited before a trailing `T`. A second
command writing `Q` was durably admitted but not dispatched. The source was killed
with SIGKILL while its effect record was still Unknown. Its remaining shell/sleep
process group was separately killed by the harness before the trailing write;
that cleanup is **not** an Alan cancellation acknowledgement or a claim that
SIGKILL performed orderly descendant shutdown.

| Required boundary | Observed evidence |
| --- | --- |
| Explicit selection/discovery | The accepted CLI entry is `--resume`; the selected filename was checked before invocation. No history-list mechanism is required by the accepted scenarios |
| Fresh execution identity | Source PID 86904 / boot `28d35736-611c-4cdf-98f6-1c5c02637bb1`; resumed PID 87482 / boot `6607a668-a3e9-487d-b8a4-22d049f48235`; repeated PID 87878 / boot `7b2c2d22-19af-4091-a793-b8b27310a4c3`. Reused Root PID 8 is not identity |
| Chosen durable history and Actions | Original admitted/dispatched IDs and Unknown effect survived; restored directory Action evidence was retained, and new directory/Q Action records were correlated separately |
| Unknown outcome is truthful | The native terminal displayed the new unknown-outcome warning on both recoveries; durable original status remained Unknown, never successful cancellation |
| Reliable queue stays paused | Only pending ID `6a448d80-2713-41e7-b5d1-2bd84a6516aa` was recovered; active unknown ID `13b114a8-9cfa-4912-8578-1f604ee47dd3` never re-entered the queue |
| Current cwd authority | No grant survived either recovery. Continue without authority was rejected; native `/project` approval preserved pause; only explicit `/continue` ran Q |
| No repeated effects | File remained `U` through recovery and reauthorization, then became exactly `UQ`; no T or repeated U/Q appeared. Repeated recovery had an empty queue. Each original ID has exactly one durable dispatch in the recovered history |
| Missing/invalid evidence | Expanded Service Manager boot regression covers a selected missing file and an invalid rollout: startup fails, selector is retained, and no fresh rollout is created; all seven root-recovery tests passed |
| One-shot shutdown | The same product binary consumed redirected input and returned exactly `RECOVERY_ONESHOT_OK` using the real Connection, then exited 0; actual `host.json` and `namespace.ap.sock` were absent afterward |
| Normal exit, descendants and Herdr detach | Earlier native lifecycle evidence above verifies Ctrl-D/owned-descendant termination and last-view detach/reconnect. This new SIGKILL trace complements it rather than replacing orderly-exit evidence |

Both resumed invocations exited 0 through `/quit`, all three owned Alan PIDs
were gone, and both test runtime endpoints were removed. The owned Herdr server
was stopped; its separate test UI client was terminated afterward and its PTY
handle completed. No default Herdr session or unrelated process was stopped.

This closes task 2.19 implementation and bounded product acceptance on the recorded
source. The new uncertainty display and expanded boot regression still require
current-head PR delivery under 4.1 and canonical synchronization under 4.2.
It does not close unrelated multi-client, cross-grant or Linux qualification.

## Partial canonical lifecycle sync — 2026-10-07

Canonical `alan-shell`, `alan-os-host-lifecycle` and `service-manager` now describe
explicit `--resume`, fresh recovery identity, paused work with revalidated authority,
instance-pinned Root replacement and recorder flush ordering. They also distinguish
Herdr view detach from native process exit. These behaviors exist on merged main
`43700bb11e28bc22e3cb099009018e0a494e4e3d`; the product acceptance above supplies
the bounded lifecycle evidence. The stale claim that recovery was unimplemented
has been removed. Existing one-shot and restart-budget requirements are preserved.

Canonical `provider-connection-contract` already requires confirmed model selection,
retention of captured callable bindings and controls through queued dispatch and
explicit recovery, and visible rejection of unavailable bindings. No duplicate
requirement is added. Task 4.2 remains open: this partial sync does not include the
unmerged Unknown-outcome UI notice, terminal polish, typed evaluation or automatic
routing guarantees. The broader input-contract audit remains open as recorded above.
