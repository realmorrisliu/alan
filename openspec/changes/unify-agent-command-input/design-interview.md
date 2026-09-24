# Unified Agent input exploration — 2026-09-24

Status: consolidated design confirmed by the user on 2026-09-24 after all
interview rounds; ADR-0058 is accepted direction. Runtime implementation remains
pending. This record does not reactivate superseded plans; ADR-0058 defines the
target revision of ADR-0056 input semantics. The alan9 naming decision is accepted separately in ADR-0057.

## Accepted prompt refinement

The user confirmed `alan: ` as the default conversation prompt and `alan! ` for
one explicit command. Leading-prefix typing/paste, empty-command Backspace,
accepted-submission reset and explicit `:` follow design.md. This is presentation
of canonical intent, not another execution mode. Future automatic routing must
make direct execution visibly distinct before activation. This refinement does
not change the current separate TUI reliability delivery scope.

## Latest accepted refinement — internal aP and alan9 commands

The user subsequently confirmed that aP should remain internal and requested this
revision. This section supersedes the prior suggestion that normal callers use
explicit protocol clients. Task-oriented alan9 executables encapsulate aP for
Agent workflows, with optional explicit advanced-user invocation. They retain
caller-scoped authority, existing service owners and ordinary execution evidence.
Project read/edit/search and native commands expose consistent public paths and
the same granted backing files; committed edits are visible across tools. Virtual
services need not have public native paths. Structured path adapters perform
mapping internally without shell-string rewriting. Normal users need neither aP
terminology nor mount/descriptor/commit-document knowledge. Detailed command names
and schemas are implementation tasks, not a speculative protocol wrapper catalog.

## Latest accepted revision — native shell, 2026-09-24

The user approved the KISS revision after the filesystem/platform research.
This section supersedes earlier rounds wherever they require a bounded namespace
command grammar, namespace `/bin` lookup, rejection of shell composition, or
blanket prohibition on native shell execution. Earlier rounds remain historical
interview evidence, not competing requirements.

- `!` executes one unchanged script through the governed native shell boundary;
  `:` and staged automatic routing retain their previously accepted meaning.
- aP remains the resource/service protocol. `/mnt` is neither automatic prompt
  inclusion nor a universal native path projection. Use explicit aP clients for
  virtual services; native commands use Host paths and native cwd.
- Host Mount grants feed HostFS and native sandbox authority. The Host adapter
  may expose authorized execution paths without granting access through strings.
- Intercept at launch and enforce with the existing OS sandbox. Do not rewrite
  scripts, translate arbitrary syscalls or add FUSE/VM infrastructure.
- Standalone user cd changes shared cwd; composed-script and Agent-action cd do
  not. Shared cwd keeps a grant-relative identity with adapter-resolved native path.
- Queue, interrupt, evidence, recovery and non-owning attachment decisions remain.

The bounded standalone-cd form and native-path sandbox reconciliation are detailed
in design.md and owning deltas. See [decision report](decision-report.md).

## Product intent

### Interview checkpoint — 2026-09-24

The user has endorsed one interaction and execution model, deterministic command
operations within the Agent Machine direction, and classification that selects
a path without granting authority. [ADR-0058](../../../docs/adr/0058-unify-command-and-agent-interaction.md)
records the accepted direction after final user confirmation.

Decision tree for the interview:

- Settled direction: one user interaction and execution model; TUI is its
  presentation; direct execution may skip generation but preserves governance.
- Round 1 confirmed: alan9 namespace command semantics; a one-submission `!`
  override; command/result evidence shared with subsequent Agent context, with
  no automatic explanatory generation after a command completes.
- Round 2 confirmed: classify without rewriting; clarify ambiguity; on evaluator
  failure use the governed Agent path or report generation unavailability;
  `:` forces Agent interpretation for one submission; shared cwd changes only
  through explicit user `cd`; ordinary work queues in order, controls remain
  responsive and pending interactions consume responses first.
- Round 2 follow-up confirmed: terminal and redirected input share `!`, `:` and
  automatic routing. The entire redirected input is one submission. This replaces
  the earlier recommendation to keep redirected input permanently Agent-only.
- Round 3 confirmed: missing response channels cause an explicit nonzero failure;
  interrupt pauses the ordinary queue for continuation/discard; bounded model
  input references readable command evidence; shared cwd is Agent Process-owned
  and resolved in execution order across attached clients.
- Round 4 confirmed: single-command grammar with quotes/escapes, `cd`, namespace
  and relative path resolution; unsupported composition rejected before effects;
  terminal exit detaches, empty-input Ctrl-D detaches, redirected EOF submits;
  visible route with no unconditional confirmation; no automatic retry after
  execution failure; explicit `!`/`:` delivery precedes automatic classification.
- Round 5 confirmed: shadow qualification and explicit activation; no automatic
  replay after restart, recovered pending work paused and cwd revalidated; one
  prefix parsed at submission start, slash controls preserved, absolute command
  paths use `!`, and request responses keep prefix characters as data.
- Current frontier: empty; final shared understanding confirmed by the user.
- Finalization complete: proposal, design, owning deltas and tasks are recorded;
  ADR-0058 is accepted direction. Existing reliability work retains its own scope.

Alan is an agent operating environment. Requiring users to choose between a
Shell product and an Agent TUI may impose an artificial split. Explore a single
input and execution experience where deterministic commands and Agent reasoning
are operations of the same system. Do not assume that merely sharing a screen
between two independent experiences satisfies this intent.

The user confirms `!<command>` as a one-submission command override while using
a System 1 / Jev-style evaluator to recognize command intent in ordinary input
and select direct execution or Agent reasoning. Subsequent ordinary input returns
to automatic routing. Jev is a candidate adapter, not a
shipped capability or an architectural prerequisite.

The symmetric `:<content>` prefix forces Agent interpretation for one submission.
It does not prohibit Tool use or mean explanation-only. Terminal input and
redirected stdin use the same routing rules. Redirected stdin is one complete
submission; it is not split into requests at newlines. For example,
`printf '%s\n' '!git status' | alan` explicitly selects command handling, while
`printf '%s\n' ':explain git status' | alan` selects Agent interpretation.
An unprefixed `git status` is classified in either entry. Script authors can
force routing using either prefix.

## Current behavior and conflicts

`alan-shell` currently requires an aP-only generic client with no Agent-aware
conversation logic and a separate terminal rendering crate. Bare `alan` attaches
to Root Agent; ordinary entries become Agent tasks and slash commands stay local.
The canonical spec says `!<command>` requests the existing governed bash Tool,
but the inspected TUI submission path forwards it as ordinary Agent text
(`crates/tui/src/file_backed/app.rs`, `handle_submit`); no deterministic `!`
execution branch was found in the audited downstream input path. Do not present
the spec's exact-command intent as a verified runtime guarantee. There is
currently no automatic command classifier or persistent command mode.

Round 1 selects alan9 namespace semantics for direct commands, including paths,
executable resolution and permissions. Host programs use adapters. Commands and
results share the Agent's execution record and subsequent context; successful
completion does not automatically trigger a model explanation. Bounded model-input projection and existing evidence retention apply without
duplicating evidence.

True unification therefore requires revisiting `alan-shell` and
`alan-renderer-host-contract`, with `rust-inline-tui` updated if presentation
changes. It cannot be claimed delivered by renaming the Renderer. Existing
Process lifecycle and non-owning attachment need not be replaced to unify input.

## Candidate execution model

One attached Agent Process could advance its Machine through either a
deterministic command operation or a generation-based task. A recognized command
could skip a generation round while reusing the existing effect lifecycle,
Process runner, file access and output evidence. This is a candidate for
ADR-0055's mixed Machine direction; it does not require a second Shell Process,
a separate System 1 Process or a global input router.

Explicit controls and `!`/`:` overrides are handled deterministically first.
For ordinary top-level input from either entry, bounded typed evaluation returns
command intent, Agent intent or ambiguity without rewriting the input. Active
confirmation/form responses are consumed by that interaction before top-level
routing. Multiline input is one submission; its command body uses the bounded single-command grammar below. Quoted/explanatory commands must not be mistaken
for execution intent.

Model inference provides routing evidence, never additional execution authority.
A command fast path would skip generation, not namespace, Tool policy, sandbox
or required approval. Explicit input constraints win over model suggestions.
The executor validates any proposed command against the submitted text and its
chosen grammar; classification must not silently invent or rewrite a command.
Ambiguous intent requests clarification. Evaluator failure, including malformed
output or unavailability, falls back to the existing governed Agent path; if
generation is unavailable, report failure. Neither case triggers inferred direct
execution. Explicit `!` input can still proceed if its required authorization
checks succeed. The Agent fallback can act under its normal policy and is not
guaranteed effect-free.
Cancellation or an unknown execution outcome must not cause redispatch through
the other path.

The interaction has a shared alan9 cwd. An explicit user `cd` changes it;
per-action Agent working directories do not. Ordinary submissions queue in
order behind active work. Control operations remain responsive. Pending
confirmation/form input is treated as a response before new-work routing.
Interrupt cancels active work and pauses the ordinary queue, preserving queued
content for explicit continuation or discard without undoing completed effects.
The cwd is owned by the Agent Process and shared by attached clients, shown by
the interface and resolved at execution time in submission order. No renderer
owns a private execution cwd.

Noninteractive work that requires clarification or approval without an available
response channel fails with nonzero exit status and a stderr diagnostic; it does
not guess, wait indefinitely or implicitly read from a terminal. Previously
completed effects and remaining work are reported truthfully.

Model context receives bounded command output, a truncation marker when needed,
exit status and an evidence reference. Additional output can be read on demand;
retention follows existing evidence ownership and limits. This does not trigger
automatic model summarization or promise infinite output retention.

## Implementation facts for the next round

The audited `crates/shell/src/lib.rs` parser supports a small set of file
builtins and executable argv with quoting/escaping, but no persistent cwd,
pipelines, conditionals or general shell expansion. Unsupported syntax is not
reliably rejected: operators may become ordinary arguments. Builtins currently
parse paths differently from executable argv. The new contract needs explicit
validation; parsing successfully today does not establish command intent.

Existing namespace execution and Agent Runtime action evidence are reusable
owners. `tool_execution.rs` under the engine's namespace environment already
provides cancellation/timeout and Action projections; `ProcessLaunchContext`
already carries cwd. A separate renderer executor or output database is not
needed to represent the proposed behavior. Exact reuse remains an implementation
decision after the target grammar is agreed.

## Consolidated artifacts

[proposal.md](proposal.md), [design.md](design.md), [specs/](specs/) and
[tasks.md](tasks.md) own the consolidated target and delivery sequence.
No unresolved product decision remains from these rounds. Candidate-specific
qualification budgets are an explicit pre-measurement gate, not an assumed
current performance claim. Final shared-understanding confirmation was received
on 2026-09-24 and ADR-0058 is accepted direction.

## Ownership and evaluation

The interview originated in the
[TUI change's archived handoff note](../archive/2026-09-24-define-alan-interaction-model/unified-input-exploration.md)
and moved here to keep unified execution separate from terminal presentation.
No unimplemented interaction delta was synced by that handoff; the latest
accepted decisions and delivery scope are recorded in this change and
[decision report](decision-report.md).
This change owns unified input semantics, deterministic command operations and
user-visible route feedback. `add-cognitive-model-routing` owns generic typed capability contracts. This
change owns command-specific transitions, ordering and evidence extensions using
the existing runtime owners; any Jev adapter remains separately scoped.
Keep accepted naming, this exploration and delivered runtime behavior distinct.

The first implementation slice delivers explicit `!` and `:` handling through
one Machine, shared cwd, action evidence and cancellation. Unprefixed input stays
on the current Agent path until automatic routing is separately qualified.
The complete target applies automatic classification across input entry points.
This staged delivery does not make Jev a dependency for deterministic commands.

The accepted first command grammar is one command with consistent quoting and
escaping, `cd` and relative paths. Resolve bare executables under `/bin`, relative
executable paths against shared cwd, and absolute paths within the namespace.
Reject unsupported pipeline/conditional/expansion syntax before any effect;
never silently hand it to host bash or split multiline input into independent
submissions. A deterministic parser must preserve quoted literal data.

Exiting an interface detaches; submitted work continues. Ctrl-C interrupts and
pauses the queue. Interactive Ctrl-D detaches only on empty input; redirected
EOF instead completes the single input submission. Interactive submissions show
command/Agent route with no new unconditional confirmation. Failed commands
report errors without automatic correction, retry or fallback execution.

Before enabling effects, compare deterministic and generation-only baselines
against shadow classification of real terminal tasks. Cover literal commands,
natural-language tasks, quoted commands, multiline paste, explicit overrides,
ambiguity, evaluator errors, cancellation and permission denial. Agree on
acceptance thresholds before measuring; classification accuracy alone is not
evidence that automatic execution is acceptable.

## Round 5 boundary decisions

Automatic classification first runs in shadow mode; it does not trigger commands.
Known false classifications of discussion as execution block activation. Record
accuracy, false execution classifications, latency and cost; set candidate budgets
before measurement and explicitly approve activation after evidence review.

Agent/Host restart never automatically replays pending input or unknown effects.
Reliably recoverable queued work stays paused for user review. Restore cwd only
from trustworthy records with current access; otherwise require explicit cwd
selection before directory-dependent work. Ordinary client detach is different:
it does not interrupt the Agent or pause already accepted work.

Parse a prefix only once at the start of the whole submission. `:!text` passes
`!text` to Agent interpretation; `!:` passes `:` to the command parser. Known
slash controls retain their behavior, unknown slash controls fail and absolute
executable paths use `!`. Empty override bodies fail. Pending request/form
responses retain these characters as data.
