# Ordered delivery — 2026-10-07

User-authorized sequence: lifecycle closure, real model switching, small terminal
presentation improvements, then side-effect-free typed-routing qualification.
Codex implements and tests directly. Baseline: merged main `59855ebe`.
The archived self-development milestone stays closed and immutable.

## 1. Unified input and recovery

The current apply checklist has 49 items: 27 checked, 22 unchecked. This measures
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


## Current delivery reconciliation — 2026-10-07

GitHub state was rechecked after the three baseline collections. The ordered
lifecycle and model slices have merged; their earlier pending descriptions above
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
  were accepted. Task 2.20 remains open for its full recovery/unavailable-binding
  matrix, not because ordinary model switching is absent.
- UI PR #1034 remains open at `942a5288ee942e03c17adc23a7580cdd26fc4b59` with
  all reported checks successful. Local native acceptance is complete; merge and
  canonical sync remain delivery work under its owning change.
- Shadow PR #1035 remains draft and stacked on the UI branch. The
  [qualification report](../qualify-agent-input-routing/shadow-qualification.md)
  now records the native candidate, generation and deterministic prefix baselines.
  Both model candidates fail the frozen command-safety/classification gates.
  Redirected response admission and complete cost/latency qualification remain
  open; automatic execution is disabled.

No broad same-Agent-client, cross-grant, retention, Linux confinement or unknown-
effect requirement is closed by this reconciliation. Keep those existing owning
checklists active; the bounded user's four-stage progress is not a claim that
all historical unified-input requirements are delivered.
