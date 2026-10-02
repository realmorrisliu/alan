# Design

## Context

See `proposal.md` for scope and `assessment.md` for the 2026-09-29 baseline.
The target user is the developer operating Alan in an ordinary terminal, with
Herdr as the preferred host. The task is to develop and inspect real projects,
including Alan itself. The supplied product direction is calm, precise,
readable output with progressive disclosure and usable scrollback.

Keep one foreground instance and one Root Agent Process per invocation.
Process owns identity, cwd and lifecycle; Agent Machine owns advancement;
AgentFS and durable evidence own observable execution; Host Mount Service owns
grants. UI state must not become execution truth.

## Goals / Non-Goals

**Goals:** a user can authorize a project, give a bounded coding task, inspect
the patch and actual checks, interrupt or correct it, and explicitly continue
durable work without an engineer supplying hidden setup. Once that is proven,
use Alan through Herdr to implement a small Alan change under supervision.

**Non-Goals:** unattended merge/release, unrestricted filesystem access,
automatic intent classification, typed evaluation/Jev, a new executor, a
desktop shell, embedded terminal emulator or general session/workspace manager.
Model quality and latency are not scored against fx with different models.

## Decisions

### 1. Three evidence gates, not a percentage-complete score

| Gate | Required evidence | Current status |
| --- | --- | --- |
| G1: usable project agent | Fresh CLI → visible project approval → inspect/edit a disposable project → run a focused check → inspect actual diff; cancel and submit a correction without hidden setup | Passed locally on 2026-09-30; review and CI pending |
| G2: supervised self-development | Alan reads relevant instructions and its own source in an isolated checkout, makes one scoped real fix, runs the failing-then-passing check, reports the actual diff; external review/build/relaunch confirms the fix | Passed locally on 2026-09-30; CI and merge pending |
| G3: repeatable daily use | A second independent task plus explicit recovery of chosen work; no completed effect replay, correct queue/cwd, no manual process rescue | Previous supervised tasks and literal Command recovery passed locally; qualification reopened on 2026-10-03 for exact managed Agent-generation recovery and duplicate external intake. New exact candidate passes real FollowUp Agent-generation recovery locally: full model/medium controls retained, fresh authority, generated effect once, repeat nonreplay and four normal exits without rescue. Independent joined reviews pass; current-head CI and merge remain pending; Native NextTurn recovery is not claimed. Herdr view detach retained another main client |

A mock-provider test, manually inserted file, operator-written patch or model's
claim of passing tests cannot satisfy G2. Human authorization and independent
review are compatible with supervised self-development; supplying the patch
or silently wiring missing runtime state is not. Do not require the running
binary to replace itself: build a candidate and relaunch it explicitly.

### 2. Project entry through the existing native authorization boundary

Add `/project` as a host-local interaction. On a fresh invocation with no
project grant, show an inline invitation to select a project; host cwd can be
offered as the initial candidate. The prompt names the directory, read-only or
read/write scope, and approve/change/cancel actions. Default Enter grants no
authority. Approval goes through Host Mount Service and the existing native
adapter, then selects the Process cwd through the existing command plane.

Agent `request_mount` uses the same native presentation. Raw Host paths stay in
the host-local chooser and never enter Agent-visible files. Ordinary prompts
show a project label and project-relative cwd. Revocation invalidates execution
and file completion immediately. A missing project explains `/project`; it does
not try bash repeatedly or print an internal adapter error as the whole UX.

Reuse the CLI's existing explicitly selected instance to answer requests. Do
not ask users to discover socket paths or PIDs. Alternative rejected: implicit
cwd mounting, which violates current authorization contracts.

Recovery acceptance exposed a paused-entry dependency: the picker currently
submits an ordinary `cd`, which queues behind recovered work, while continuation
requires a current cwd grant. A settled directory choice therefore uses the
existing Agent Runtime file-native control lane, delegating unchanged current
grant/path validation and binding to the Process cwd owner. The renderer waits
for correlated authoritative confirmation before updating cwd. Authorization
alone selects no cwd and continues no work; the user must still explicitly
continue the paused queue. Running or unsettled work rejects this boundary
control. This is narrowly owned by `unify-agent-command-input`, not a privileged
Host command manager or an arbitrary-command FIFO bypass.

### 3. Immediate receipt without inventing task completion

After authoritative admission, render the submitted text once with its actual
disposition: running, queued, or paused. Show queue count and a compact preview
when paused. Rejection retains the editable draft and explains why. Reconcile
receipt and later Tape/Action records by existing submission identity so history
does not duplicate them. Runtime scheduling, correlation, cancellation and
paused-queue semantics stay with `unify-agent-command-input`.

Ctrl+C while an idle draft exists clears that draft; during active work it
requests cancellation. Cancellation acknowledgement distinguishes requested,
settled and unknown outcomes. `/continue` and `/discard` show which pending
inputs are affected. Local help and input editing remain responsive while a
control request is pending. Diagnose the observed stall before choosing a fix;
do not mask it with optimistic "cancelled" text or timeout-driven replay.

### 4. Use the existing composer and authoritative sources

Wire composer history to its owning channel System Store subtree; do not place
it in the project. Seed file candidates only from approved accessible roots and
Skill candidates only from installed/explicit descriptors. Clear stale candidates
on grant or source changes. Persist canonical intent, including `!` and `:`.

Completion candidates appear below the editable composer. Changing candidate
count or wrapping must not move the input/cursor when the terminal, transcript
and input wrapping remain unchanged. Count composer rows and candidate rows
separately in the existing shared inline layout so candidate rows do not consume
the composer limit. Preserve usable bounded disclosure near the terminal edge;
no permanent reserved panel or separate completion viewport owner is introduced.
One row of ordinary blank input spacing is budgeted from initial Ready, together
with retained history, so the input does not consume the terminal's final row
before completion appears. A short menu uses that row for the selected item;
this is not a permanent six-row panel. Candidate-only origin limits do not
constrain forms or the full-terminal Action details view.

Tab inserts a highlighted completion. Enter on a slash-command candidate executes
that command once; a complete slash command does not require a second Enter.
File/Skill completion inserts a reference, never submits the task implicitly.
Keep readline editing; document existing Ctrl+R thinking behavior for this slice
rather than silently remapping it into search. Add help for mode prefixes and
editing keys. Do not introduce a new completion engine or terminal-owned history.

### 5. Terminal design: substance first, details on demand

The user supplied a two-line shell prompt reference on 2026-09-29. Adopt that
shape for Agent interaction: one context/status line immediately above one
editable input line. It follows the transcript rather than being pinned to the
screen bottom. Multiline drafts and temporary choices expand only as needed.
Keep the inline normal buffer and host font/theme/selection. Use terminal-default
foreground for body text, muted secondary metadata, one accent for actionable
focus, and warning/error/success cues paired with words or glyphs. Do not rely
on dim blue for critical text or color alone for diff meaning.

The shared layout rhythm is one blank line between turns, none between a Tool
summary and its child rows, and one blank line before the two-line prompt. The
user subsequently chose compact `: ` / `! ` prompts; keep these route markers
recognizable and align continuation text by display-cell width.
Prefer semantic spans from typed cells over flattening all content to strings
and classifying line prefixes. Reuse HistoryCell and Ratatui types; do not add
a parallel renderer.

Proposed idle and completed-task composition (illustrative, not shipped):

```text
: Fix the completion Enter behavior

  ✓ Read app.rs · 120 lines                         details: Ctrl+O
  ✓ Edit app.rs · +8 −3
  ✓ Test completion · exit 0

Complete. Enter now executes the selected command once.
Changed: crates/tui/src/file_backed/app.rs

alan / · <effective model> · ready
: ▌
```

Proposed paused state:

```text
Cancelled. /continue resumes the queued input; /discard removes it.

alan / · <effective model> · paused · 1 queued
: ▌
```

The status line always uses actual Agent data: project label/relative cwd,
effective model, and state. `ready`, `working`, `waiting for approval`, `paused`
and `failed` are distinct; active work can add elapsed time and queued count.
Expose restricted authority (`read-only`, `no project`) when relevant, rather
than repeating permissions on every normal turn. Token/cost/context usage and
the Connection profile belong in `/status` or details unless needed for an
action. Unknown model is shown as unknown, never a configured-but-unbound value.

At 80+ columns, render the three primary fields on one status line. At 60–79,
shorten cwd first; below 60, retain state and model, abbreviate the project and
move secondary fields to `/status`. On extreme widths the status can wrap rather
than conceal an approval action. Draft input remains its own line. `!`
identifies explicit command intent even with color disabled; model presence does
not imply a model was called for a deterministic command. Do not copy the
reference image's Git branch/dirty indicator into an Agent activity indicator.
Git metadata is optional project detail, not an execution-state source.
Never truncate the cancel/approval action into ambiguity. Limit routine Tool
summaries to three physical rows. Short failures include cause and next action; details retain the
diagnostic rather than leaking it into every turn.

Render headings, emphasis, lists, inline code, fenced code and diff add/remove
lines with semantic styles. Preserve code indentation and copying. No general
HTML renderer or syntax-highlighting dependency is required for the first slice.
Verify light/dark and low-color terminals; host typography remains host-owned.

### 6. Fix Tool presentation at the common projection boundary

Current `action_snapshot_to_history_cell` turns raw Action output/result into
PlainText, even though the engine already has ToolResultPresentation mapping.
Carry the existing structured title/presentation through the authoritative Action
file projection; reuse the same mapping for user commands and Agent Tools.
Keep raw evidence available separately. Do not decode each Tool's arguments in
the TUI or sanitize arbitrary project files to make the display prettier.

Ctrl+O opens a transient details view for retained Action evidence; arrows select
an action, PgUp/PgDn scroll, Escape returns to the exact draft. Existing thinking
toggle remains separate. Closing details restores the inline position, without
copying detail pages into permanent scrollback. Use existing file reads rather
than a new transcript database. Enumerate the owning Action files when opening
details; the pruned inline transcript index is not a retained-evidence catalog.
Bound summaries by physical rows and bytes so a one-line escaped JSON payload
cannot evade collapsing. Reuse the existing evidence projection, retention-expiry
and redaction markers, distinguishing unavailable reads from genuinely empty
output. If retained evidence is truncated, expired or absent, disclose that;
an expand affordance cannot reconstruct it or rerun an effect.

### 7. Effective model, not a decorative picker

`/status` names the effective profile/model and supported request controls;
unknown values are explicitly unknown. `/model` reads the active Connection's
authoritative catalog and applies the user's choice at a serialized admission boundary.
Already-admitted inputs keep their binding; new selection affects subsequent
inputs and never rewrites in-flight work. The owning provider/Connection deltas
permit this explicit model-selection exception to the existing Process-lifetime
model binding. Connection Service validates and publishes the callable; Agent
Runtime Service installs it at a serialized admission boundary before success.
Already-admitted queued work retains its captured callable and resolver-owned
controls, including explicit recovery; unavailable prior bindings fail visibly
rather than silently using the newer model. Selection never resumes the queue.
Status distinguishes the confirmed next-input binding from older admitted work.
An idle-only picker does not complete this requirement. Selection failure preserves the last
confirmed model and shows an error. No new automatic cheapest/fastest router or
preference for an unverified model identifier is part of this milestone.

The existing cognitive roadmap's TUI-picker item transfers here; generic typed
evaluation remains there. Any needed provider/model binding change must be made
in its owning Connection/provider contract before the UI claims it is effective.

### 8. Execution order and ownership

| Slice | Delivery | Owner / dependency |
| --- | --- | --- |
| P0 | Visible authorization, usable project/cwd, stall reproduction and fix if confirmed | This change's host UX; existing Host Mount owners; runtime fixes in unified-input change |
| P1 | G1, immediate receipts, composer wiring, actionable errors | This change; queue semantics depend on unified-input change |
| P2 | G2: one Alan-authored patch/test/diff and reviewed candidate relaunch | Acceptance here, actual fix in its existing owner |
| P3 | Semantic transcript, bounded summaries/details, model/status UI, G3 | This change; Connection/provider owners for binding |

Minimal spacing and intelligible errors ship with P0/P1. Full polish does not
block G2. Once G2 passes, use Alan through Herdr for the next bounded P3 fix;
external review remains required. Do not accumulate a large combined PR: each
vertical slice carries its own verification and current-head review.

`unify-agent-command-input` retains native execution, queue/cancellation, shared
cwd, no-replay and lifecycle deltas, including actual Herdr detach acceptance.
This change adds presentation and an aggregate product gate, not competing
runtime definitions. `expose-agent-rollout-history` remains parked; explicit
recovery discovery is delivered only via the active input/recovery owner.

## Risks / Trade-offs

- Mount-on-start convenience can accidentally amplify authority → display a
  host-local candidate and require an explicit scope choice; test rejection,
  read-only grants, revocation and path confinement.
- A working Tool test can conceal missing production wiring → run all gates
  through the shipped bare CLI; record any operator intervention.
- Model nondeterminism can obscure runtime bugs → preserve the live trace and
  add one focused deterministic regression at the identified shared cause.
- Typed display data may be unavailable in old evidence → bounded plain-text
  fallback, truthful unavailable details, no fabricated structured result.
- A build may need toolchain/cache/network access outside the project → record
  exact denied resources, use the existing least-privilege escalation path;
  do not turn off the sandbox or mount the whole home directory.

## Migration Plan

Ship on the dev channel in small reviewed slices. Keep the known-good executable
until the candidate has passed a fresh relaunch and terminal acceptance. Store
formats remain backward-readable; history failure is visible and non-fatal.
Do not migrate stable credentials/stores as part of UI testing. Sync only merged,
verified deltas into canonical specs and archive only after all owned work is
complete or explicitly handed to an active successor.


### Implementation handoff: truthful queue and model projection

Keep durable accepted queue truth separate from `UiActivitySnapshot`:
`waiting_submission_ids` owns interactive response waiting, not queue admission.
Project-boundary UI must classify transport delivery as unknown until exact
IDs appear in the MachineInputQueue-derived accepted projection. Reuse existing
admitted/settled/active IDs, pause and uncertain-removal state, publish after
durable acknowledgement and preserve monotonic per-Process observation across
snapshot/watch races. Independent activity/heartbeat writers must not overwrite
queue receipts. Missing/legacy/unreadable state stays unknown. The final Runtime
cwd authority still guards project selection; acknowledged Paused accepted work
may change project without execution or implicit continue. Publication failures
must never trigger replay or change durable disposition.

Model projection distinguishes selected-next from actual-active binding and
uses resolver-owned request controls. Read active truth from the existing
independent binding snapshot; contention cannot fall back to launch metadata.
Connection Service remains catalog/validation authority for the authorized
profile, with only callable models selectable: an injected fallback supports
only its original callable, not every model listed by its provider. Selection
is confirmed after validated capture and durable publication, never resumes the
queue, and does not write profile/default configuration. Safe typed projection
excludes credential references, revisions, configs and provider error chains.
TUI hydration/watch and exact selection receipts remain pinned to one Process;
unknown/unavailable is explicit and old Root responses cannot install state. Selection settlement uses the existing same-Process InputCompleted event
for the exact input ID, not an Action result or control-write completion. The
model document has no selection ID: it supplies binding facts independently of
request disposition. A projection update cannot acknowledge an unrelated pending
request; successful receipt plus an unreadable projection cannot fabricate the
new binding's controls. Lost owner/event stream leaves uncertain disposition
without retry.


### Implementation handoff: Process Skill observation

Reuse PromptAssemblyCache mentionable_skill_ids, not listed_skills or the global
Package catalog. Explicit-only enabled available Skills remain mentionable.
Publish only canonical IDs plus safe Process identity/version/known state through
the existing AgentFS machine/ui projection and per-Agent invalidation pattern.
Initialize before Ready; update after actual ensure on prompt build/resolve.
Ensure failure clears old observation, valid empty cache is known-empty. Old
Process immutable leased references and fresh Process explicit references remain
distinct; neither display failure nor display overflow changes execution authority.

Reuse AgentFS MAX_DOC_BYTES (1 MiB) for the whole serialized document, checking
before publication and replacing overflow with a small unknown observation.
Do not add unrelated loader2000 or per-ID256 thresholds, duplicate canonical
normalization in Protocol, or rely on oversized-write rejection to clear stale
state. TUI Root-pinned hydration/watch must invalidate both sources and an open
popup; stale Tab must not insert, selection never submits the task. Backend
qualification precedes the separate consumer slice and native acceptance.

### Cancellation handoff: AgentFS request terminal state

Actual G3 cancellation of a pending bash escalation clears the Machine wait but
leaves AgentFS request pending, so the watcher retains the approval composer and
interprets `/quit` as an answer. Repair the shared reset boundary, not the UI.
AgentFS owns request cancellation through the request object's `ctl`; status
remains read-only and no fake answer is written. NamespaceAgentFiles carries the
owner operation through aP using actual service request IDs under the pinned
Agent Process. Settle exact confirmation/structured waits before reset, preserve
unsettled associations on failure, and keep Host Mount terminal evidence intact.
Response and cancellation use the same state lock; late response clunk must
recheck terminal state. Idempotent cancellation preserves existing answered or
closed truth and unrelated requests. Existing request events invalidate the TUI.
The new file-layout delta and focused lifecycle/Engine/watch regressions qualify
this slice; whole CLI quality and a fresh actual G3 remain separate acceptance.
