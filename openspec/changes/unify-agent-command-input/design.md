# Design

## Context

See proposal.md for motivation and design-interview.md for confirmed choices.
ADR-0058 is accepted direction following final user confirmation on 2026-09-24. The current
TUI uses Alan Shell for aP IO but forwards ordinary input into generation. Its
Shell parser is partial and does not safely reject all unsupported operators.
No deterministic `!` branch was found in the audited input path.

## Goals / Non-Goals

**Goals:** one Agent Machine for human commands and Agent work; explicit intent,
shared context and cwd, truthful outcomes, and the same authority across entries.

**Non-Goals:** implementing a POSIX shell, transparent aP-to-POSIX translation,
Linux VM/container migration, FUSE, syscall interception, a new Kernel Process kind, a
second execution manager, a new terminal emulator, or universal model review of
human commands. Existing standalone aP Shell use does not need to disappear.

## Decisions

### One runtime authority with native command execution

The Agent Machine accepts and orders work, dispatching a command through the
existing governed native Tool execution boundary or advancing through generation.
The renderer and classifier cannot launch private subprocesses. Both user and
Agent commands reuse approval, Host Mount delegation, sandbox, cancellation and
Action evidence. The command action uses ordinary Alan Process ownership; its
native shell and descendants are tracked by the Host adapter, not automatically
promoted into distinct Alan Processes for every shell fork.

Keep the reusable aP Shell library generic for explicit namespace operations.
Host executable lookup uses the selected shell's PATH and Host filesystem; it
is distinct from Alan `/bin` lookup. In particular `!q list` does not implicitly
invoke Alan Quartermaster. Internal namespace operations are exposed through explicitly installed,
task-oriented alan9 control commands. These commands encapsulate aP access and
launch semantics; callers need not construct protocol documents. Reuse existing
executable packaging/discovery. This change defines their contract, not a new
command framework or the names of a speculative full command catalog.

### One submission and explicit intent

A submission is the complete composer entry or all redirected stdin. Consume
pending request responses before top-level routing. At the beginning of a new
submission, recognize one `!` or `:` prefix; do not recursively interpret its
body. Empty prefixed bodies fail. Preserve existing slash controls and reject
unknown slash commands; absolute executable paths use `!`. An outer prefix at
byte zero is explicit; leading whitespace is not stripped to discover a prefix.
Whitespace-only input creates no work. A trailing stdin line ending terminates
input and does not create a second command. Internal newlines remain part of one shell script, with one submission outcome;
they are not split into separately scheduled submissions.

`!` executes the submitted command, `:` passes the body to Agent reasoning, and
unprefixed input eventually uses typed intent classification. No classifier may
rewrite a command. Syntax errors or command failures are terminal for that route,
not reasons for implicit generation, repair or retry. Agent input remains capable
of governed effects; `:` is not a read-only promise.

### Prompt communicates the submission route

The explicit-prefix slice starts each ordinary composer entry with `alan: `,
meaning Agent conversation. A leading `!` in an empty entry changes the prompt to
`alan! ` and leaves only the command body after it. Backspace on an empty command
body removes the command override and restores `alan: `. After successful
submission/acceptance, a new entry starts at `alan: `; a rejected submission keeps
its draft and intent. Command intent is one-shot, not a persistent shell mode.

Typing and pasting use the same whole-submission prefix contract: a paste starting
with `!` into an empty entry shows command intent, while `!` inserted within an
existing body remains literal text. Explicit leading `:` keeps `alan: ` without
showing a redundant delimiter; it retains forced-Agent intent for future routing.
Pending form/request responses retain their own handling and do not toggle route.

Keep the canonical prefix/intent in the submission record even when the renderer
folds it into the prompt. Display must not strip body characters recursively or
create a second executor/router. Thus `:!text` shows Agent input containing literal
`!text`, and `!!command` passes the second `!` as shell body. History/draft restore
must preserve that intent and show the matching route. Resize, multiline editing
and cursor placement use the actual visible prompt width. Redirected input keeps
the same framing contract without printing interactive prompts.

Future automatic classification must not execute a direct command while presenting
it as conversation under `alan: `. Its visible route behavior must be designed and
qualified before activation; this does not add an unconditional approval dialog
or enable automatic routing in the first slice.

### Host shell and aP resource boundaries

Pass the exact command body as script data to the existing Host shell adapter,
with explicit cwd and controlled environment. Select and record one supported
shell executable/dialect using the existing backend; never silently switch
shells on failure. Shell startup configuration follows that backend's governed
environment contract. Pipelines, quoting, redirection, expansion and multiline
scripts have the selected shell's semantics. Preflight may deny execution, but
must not rewrite path tokens or scripts. Syntax checking cannot promise atomic
execution: a later error can occur after earlier script effects.

`/mnt` organizes reachable aP resources, not automatic prompt inclusion and not a
promise of identical native paths. Host Mount Service remains the grant owner.
Each native shell action uses only the one explicitly delegated local grant
selected by the Process shared cwd. Other grants remain available to structured
Agent file operations, and a standalone `!cd /mnt/<grant>` can select another
already-delegated Host Mount for later commands. A shell action cannot address
two disjoint grants at once; the first slice adds no cross-platform mount-alias
layer. Virtual mounts supply neither native backing nor ambient command rights.
Normal commands use native paths or paths relative to their native cwd. Task-oriented
alan9 commands access virtual resources internally using their protocol and commit contract.
Do not overload native `cat`, redirect syntax, or executable names with aP lookup.

The Host adapter owns grant-to-native-path resolution and supplies native cwd and
paths only as ephemeral Host-adapter spawn/sandbox inputs, outside the Alan Process
exec manifest. Alan-generated Host/execution path metadata, AgentFS path fields
and execution-path references persisted as durable evidence use public
grant-relative project paths or opaque references, never the raw backing path.
Paths in stdout/stderr
captured by Alan are projected relative to that submission's shared cwd (`.` for
the cwd) before reaching the user or Agent. This does not intercept native shell
redirection or rewrite files: a file written within the active delegated grant
remains ordinary project data and may contain a native path. The redirection itself does not copy
those contents into command output or evidence; a later file read returns
ordinary project data under the same grant. This bounds path privacy to
Alan-managed metadata and captured output while preserving the selected Host
shell's file semantics. Switching shared cwd selects another grant without
widening the current action's sandbox. Structured Agent file tools may still
address any delegated grant through their existing access checks. Path strings
confer no authority, and the engine must not build sandbox roots from them.
Keep command text unchanged; redact only paths in output streams captured by Alan.

On Linux, an existing reified sandbox can retain isolation using authorized
paths at their native locations. It must not require namespace-alias rewriting.
Do not add virtualization or bypass confinement to obtain transparent execution.
Backend limitations retain existing denial/escalation and degraded reporting.

### Internal protocol and task-oriented alan9 commands

Normal user flows describe projects, files, work and permissions, not aP, fids,
clunk or internal `/mnt` topology. Agent-facing command descriptions likewise
present supported tasks and results, not recipes for writing protocol documents.
Internal aP paths remain available for developer inspection without becoming a
prerequisite for ordinary work.

Expose only needed operations on Agent work, memory and services as ordinary
alan9 command executables. They are primarily for Agents and may be explicitly
called by advanced users. Discover them through existing Tool/executable metadata;
first inventory and reuse existing operations. Select exact command spellings
and argument/result schemas when implementing the first real use cases, without
creating a second CLI framework or one wrapper for every aP file operation.

These executables delegate to existing service owners over aP with caller-scoped
handles/descriptors or the existing authorized attachment. They must not inherit
a broader ambient Host connection. Native sandbox permissions alone cannot grant
internal service authority. Require the existing rights, validation, commit and
cancellation contracts; return useful output, explicit errors/exit status and
correlated action evidence. Distinguish accepted asynchronous work from completion;
unknown commit outcomes must not trigger automatic duplicate submission.
The same Process/Tool launch boundary records their effects. Host lifecycle,
credential and native authorization commands keep their existing owner.

### One project file identity across editing and commands

Project tools expose grant-relative paths for any delegated mount and
shared-cwd-relative paths for the active shell grant. At a structured
read/edit/search boundary, the Host adapter validates and resolves the path
against delegated Host Mounts, then supplies the existing HostFS/aP or native
file operation. This is explicit path-parameter resolution, not interpretation
or rewriting of arbitrary shell text. To use another grant from the shell, the
user explicitly selects it with standalone `!cd /mnt/<grant>`; the Agent tool and
shell then observe the same backing files. Combining disjoint grants in one
native shell action is outside this slice.

A committed successful edit changes the same backing file observed by subsequent
shell reads, `git diff` and external editors. Subsequent Agent reads observe native
edits too. Do not introduce an isolated project mirror or silently edit a virtual
resource when a native path was requested. Pending editable buffers are not saved
files: expose pending/save failure explicitly, preserve existing stale-content
checks and do not claim cross-tool atomicity during concurrent external writes.
Read-only, symlink containment and revoked-grant checks apply across both access
paths. Purely virtual services remain command-operated resources; they need not
have public Host paths or be materialized as ordinary project files.

### Minimal standalone cd handling

Reserve only a standalone explicit user `cd <directory>` for shared cwd updates.
The initial form accepts one literal relative or Host-absolute directory with
quotes/escapes, or a public `/mnt/<grant>` path for an already-delegated
Host-backed mount; no arguments, `-`, tilde, variables, substitutions or globs
fail with an explicit diagnostic. This deliberately bounded builtin is not a
shell interpreter. Resolve it through delegated Host Mounts, validate access and
update only on success. It cannot change cwd into a purely virtual aP directory.

A composed script such as `cd subdir && make` executes unchanged in the shell;
its directory changes remain local to that action. Agent-generated `cd`, exports,
aliases and functions do not alter future commands. Do not scrape `pwd` or shell
output to reconstruct session state. Each action launches with explicit cwd and
environment; persistent interactive-shell state is outside this slice.

### Shared cwd, ordering and lifecycle

The Agent Process owns one cwd reference (delegated Host Mount plus relative
location); the Host adapter resolves its native execution path. Machine orders
changes made by standalone explicit user `cd`.
All attachments see it and each command resolves it at execution time. Agent
per-action cwd does not mutate this shared value. Ordinary submissions queue
behind active work; responses and controls do not become queued ordinary work.
Process-local submission identity correlates output and cancellation, without
introducing a global Conversation/Session object. Existing client task leases
must be reconciled with ordered acceptance rather than silently bypassed.

### Submission records and result correlation

The input envelope and result IDs below are implemented in this slice. The
activity projection, queue-control verbs, and pre-Tape failure correlation are
the target contract for tasks 2.5–2.8; they are not current runtime guarantees.

The canonical write to /agent/<pid>/io/input is AgentFS's existing outer
length-framed document containing the UTF-8 payload `alan-input-v1\n` followed
by one strict JSON object: `version` (1), `submission_id` (UUID), `intent`
(`agent`, `force_agent`, or `command`), `mode` (`steer`, `follow_up`, or
`next_turn`), and exact `body`. The prefix is removed by the client and retained
as intent; body whitespace and internal newlines are not rewritten. Empty bodies,
unknown fields, unsupported versions, and invalid IDs are rejected. The record's
mode maps only to protocol scheduling; intent never supplies or changes it.
Unframed payloads remain the legacy plain-text path.

The Agent Machine owns accepted order and queue state. Its existing
machine/ui/activity snapshot becomes version 2, retaining activity state and
start time and adding `active_submission` (optional `{submission_id, intent}`),
`pending_submissions` (ordered `{submission_id, intent}` entries), and
`queue_paused` (boolean). machine/ui/events appends the same snapshots; neither
surface owns another copy of the queue. The current activity state remains run
state, not a completion signal. Runtime queue controls continue to use
machine/ctl, one UTF-8 command per write. The versioned queue-control vocabulary
is `queue-v1 interrupt <submission_id>`, `queue-v1 continue`, and
`queue-v1 discard`: interrupt targets one accepted submission and pauses later
ordinary inputs, while continue/discard apply to the paused queue in order. An
unknown or already-settled interrupt target is rejected without affecting later
work. Existing compact, rollback, and legacy interrupt commands retain their
current semantics.

Completion is correlated through existing evidence, not prompt text, time, or
Tape position. The append-only machine/tape projection adds submission_id to
user and assistant message records; a direct command's Action result uses the
same ID as call_id and carries its exit status, while Action output carries
stdout/stderr. A pre-Tape failure is reported in a UI error event carrying that
same ID. The TUI's current shared task lease remains a compatibility guard while
this aggregate activity surface is used, not the result-matching mechanism.
Readers accept a result only when its ID matches their submission; missing
evidence is unknown, never successful completion. Rollout/checkpoint records
remain the recovery authority; Tape is only a projection.

Ctrl-C interrupts current work and pauses remaining queued input for explicit
continuation/discard through the existing machine control surface. It does not
roll back effects or start the next command. Detach, quit and empty-input
terminal Ctrl-D with no pending Agent input leave accepted work running; Ctrl-D
with a pending confirmation or structured-input request keeps the client
attached. Pipe EOF completes a submission. Without a response channel,
clarification/approval fails with stderr and nonzero exit; never read a hidden
terminal. Report prior effects accurately.

### Evidence, display and recovery

Record accepted work, chosen route, cwd, actions, outputs and outcomes through
AgentFS and existing rollout/checkpoint owners. Command output appears directly,
without an automatic explanatory generation. Model input uses a bounded excerpt,
exit status and a readable output reference; identify truncation and retention
loss. Reuse existing redaction and retention rules, not a private transcript DB.
Interactive clients show route and cwd; redirected stdout contains the result,
with routing/diagnostics on stderr. Commands propagate their exit status; Agent
work preserves the existing final-answer success/failure convention.

After Agent/Host restart, recoverable pending work is paused for explicit review.
Never replay unknown effects automatically. Restore cwd only from reliable state
and after checking current reachability/rights; otherwise require a new explicit
directory choice before directory-dependent work. Missing recovery records are
reported, not reconstructed from textual Tape or inferred from a reused PID.

### Classification and staged delivery

Slice 1 implements explicit prefixes with the same Machine, cwd, queue, evidence
and cancellation; unprefixed input still follows the existing Agent path.
Slice 2 consumes generic typed evaluation supplied by the cognition change.
A valid result selects command, Agent or ambiguous; ambiguity asks for
clarification. Evaluation failure triggers one bounded governed Agent fallback;
if generation is unavailable, fail. Cancellation precludes fallback dispatch.

Before activation, run classification in shadow mode without triggering effects,
compare deterministic and generation-only baselines, and record accuracy, false
executions, latency and cost. Known discussion-to-command misclassifications
block activation. Freeze numeric performance/accuracy budgets before evaluating
the candidate and require explicit activation approval after evidence is reviewed.
These adapter qualification values can be chosen later without changing input
semantics. Jev is a candidate, not a delivered capability or required first slice.

## Risks / Trade-offs

- Misclassified intent can cause effects → explicit overrides, typed validation,
  shadow qualification and unchanged execution governance.
- Shared cwd and queued work can surprise another client → file-visible cwd,
  ordered execution and paused queue after interrupt/restart.
- Native and aP paths differ → keep one public project-path identity, resolve native
  paths inside the Host adapter, retain task-oriented internal control commands,
  and never silently translate command text.
- Shell scripts can have partial effects → report actual output/status without
  repair, replay or atomicity claims.
- Standalone cd has a bounded grammar → document it; script-local cd stays local.
- Missing durable state prevents full recovery → report uncertainty and require
  explicit continuation; do not claim a full replay system from Tape alone.

## Migration Plan

Implement and validate the confirmed explicit semantics first, and document
that unprefixed input still uses the Agent baseline. Coordinate with the ongoing
terminal reliability change without replacing its presentation fixes. Add typed
classification only after its owning capability contract and qualification gates
are delivered. Sync only implemented deltas after merge; retain later routing
work as active scope rather than archiving unimplemented guarantees.
Rollback can disable automatic classification to the Agent baseline; it cannot
undo effects already executed or bypass command governance.
