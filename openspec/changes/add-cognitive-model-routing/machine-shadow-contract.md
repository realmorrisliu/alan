# Machine shadow evidence contract — 2026-10-07

Status: implementation contract for the activated advice-only slice, not delivered
runtime behavior. The Connection/profile boundary is delivered locally at
`858e2535`; Machine consumption and qualification remain outstanding.

## Admission and original input

The collector retains corpus bytes and their SHA-256 before either real client
adapter consumes input. Correlate that immutable source with the resulting
submission UUID or request ID. Do not reconstruct the source by prepending an
intent marker to the body: interactive mode selection and history recall can set
intent without those bytes being present in the composer.

Interactive ordinary input uses `FileBackedApp::handle_submit`, then
`write_agent_input`; redirected ordinary input uses `StdioTaskWaitContext`, then
`submit_stdio_task`. Both commit `UserInputRecord` to AgentFS `io/input` and reach
`NamespaceAgentFiles::read_next_input_submission`. The Machine's existing
`input_queue::admit_input` owns ordinary input durability and duplicate suppression.
Do not put provider calls into that admission helper: it also serves Process-loop
observation while a transition owns the Machine.

Interactive pending responses instead use `FileBackedAction::Resume` and
`requests/<id>/response`. They do not become ordinary input. The shadow collector
must observe this deterministic branch before prefix processing; a normal-input
queue hook alone cannot establish pending-response precedence. Qualification
retains the original response bytes separately from any existing form encoding.

The shipped redirected runner accepts one task and reports that interactive input
is required when that task waits for a response. It has no request-response input
path. Therefore the frozen pending-response cases on the redirected surface are
currently an acceptance gap, not passing bypass observations. Do not substitute a
synthetic `pending_response` flag, label a TTY response as redirected, drop those
rows, or change the frozen denominator. Resolve the real admission contract and
its owning unified-input spec before claiming both-surface qualification.

## Captured evaluation authority

Explicit shadow setup supplies a reachable evaluation Connection separately from
the ordinary generation binding. Connection Service retains profile resolution,
publication and Host credential ownership. Machine receives only captured callable
identity and a namespace handle that reaches that Connection. It must not replace
`ProcessBindings.confirmed`, change the generation default, enumerate a global
profile registry, or read `.env`/Host credentials. Profile removal or secret
revocation never causes selection of a different evaluator.

Candidate IDs are the fixed advice labels command, agent and ambiguous, not
executable names. The command label describes the original input only; it carries
no rewritten command and no permission. Explicit input intent and pending
responses bypass the evaluator. NoMatch, unavailable and failure remain explicit
outcomes, not fabricated successful Agent selections.

## Durable transition ordering

Reuse `RolloutRecorder::persist_batch` and the existing `EventRecord` envelope;
Agent Machine is the single transition writer. An explicitly selected shadow
observation requires durable storage before it can dispatch a paid model call.
Do not silently treat the existing optional-recorder no-op as durable success.

1. Capture admission identity, original-input digest/reference, surface, candidate
   set, schema, exact Connection/provider/model/revision, and deadline. Allocate
   one llmfs evaluation operation without committing its data.
2. Persist a started observation including that operation ID before committing
   `data`. A failed or ambiguous persistence acknowledgement must not dispatch;
   reconcile the same observation rather than allocate/retry another call.
3. Commit once and read the typed terminal result through the captured mounted
   Connection. One deadline covers allocation, persistence, provider-lock wait and
   result validation. There is no retry or generation fallback in this slice.
4. Validate membership and provenance, then durably settle the same observation
   before publishing terminal advice. Record Selected, NoMatch, unavailable,
   malformed, timeout, cancelled or interrupted distinctly; retain usage and
   unknown cost as unknown. Monotonic elapsed time spans the whole attempt.
5. Publish only acknowledged evidence. Cancellation requests abort for an allocated
   operation and prevent later advice from becoming success. A lost abort or
   terminal-write acknowledgement is uncertain evidence, never permission to retry.

Observation identity includes the originating durable execution identity and
submission/request identity. Process PID or ephemeral llmfs operation ID alone is
not a recovery key. Bypass observations persist their reason and zero evaluator
calls without allocating an operation. Repeated delivery reconciles the same
observation. A qualification repeat is a fresh submission with explicit repeat
metadata, not replay of a completed observation.

## Recovery and projection

Rebuild observations from reliable rollout/checkpoint evidence. A started record
without a reliable terminal record projects interrupted; recovery must not reopen,
recommit, regenerate, or dispatch effects. A recorded terminal result remains
historical advice and does not settle or consume the separately governed ordinary
input queue. Pending ordinary work retains explicit recovery/continue semantics.

Expose the latest acknowledged observation as `machine/evaluation`, with a
versioned schema, source observation identity, captured provenance, state and
outcome. This is a bounded snapshot, not another append-only history. Historical
qualification evidence remains in the existing durable rollout. A fresh Machine
reports unknown/no observation, never a default successful selection.

The Agent Runtime Service publishes the Machine snapshot through the AgentFS
owner's in-process update path, analogous to Machine status. The public aP node is
read-only: reject Write/ReadWrite open and direct write; it is not another writable
`machine/ui/*` document. Publication retries may rebuild only from acknowledged
Machine evidence and cannot call the evaluator. Preserve identity when a recovered
Process receives a new PID; do not relabel an old operation as a newly dispatched
one. Process exit, work completion and evaluation completion remain separate.

## Required implementation checks

Use actual mounted evaluation with dispatch counters and a project filesystem
sentinel. Cover typed command advice with zero Tool/command/fallback dispatch,
explicit-prefix and response bypasses, duplicate delivery, cancellation before
commit and after provider completion, deadline during allocation/lock wait,
revocation, both durability barriers, and recovery across each barrier. External
writes to the projection must fail. The real client adapters must prove source
correlation; engine-only fixtures cannot stand in for the two admission surfaces.
These checks remain tasks 1.4, 2.2–2.4 and 3.1–3.2; this contract does not check them
off or activate automatic execution.
