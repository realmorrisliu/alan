## Context

The first `render-alan-shell-in-terminal-ui` slice proved the local
renderer-host path: `alan` can launch a namespace-native runtime, hand
`alan-terminal-ui` a mounted root plus `/agent/<pid>`, and converse through
`io/output`, `io/input`, `requests/`, `actions/`, and `ctl`. Since then the
file-backed path has grown local completion, history, request/form handling,
action projection, and `machine/ctl` support.

The remaining gap is no longer basic terminal plumbing. The daemon-backed path
still owns several user-visible states because they only exist as session
events today: turn lifecycle, thinking text, plan updates, warnings, and
compaction or memory-flush notices. Without projecting those states into agent
files, the local file-backed path cannot become the complete terminal contract,
and removing the daemon-backed local path would regress behavior.

## Goals / Non-Goals

**Goals:**

- Define durable file surfaces for renderer-visible runtime state that is still
  daemon-event-only today.
- Let local `alan-terminal-ui` hydrate and stream that state from mounted agent
  files without session APIs.
- Preserve the renderer-host boundary: the TUI reads files and writes `ctl`,
  rather than reconstructing daemon `EventEnvelope` state machines.
- Make the daemon-backed local path removable once parity tests pass.

**Non-Goals:**

- Do not redesign remote attachment or replace daemon APIs for remote clients in
  this change.
- Do not turn `alan-terminal-ui` into a general shell multiplexer; this remains
  a conversation-oriented renderer-host.
- Do not move model-request assembly or session durability semantics out of the
  runtime layers that already own them.

## Decisions

### Decision: add a dedicated runtime-owned `machine/ui/` subtree

Renderer-visible runtime state that is not plain conversational output will live
under a new `machine/ui/` subtree rather than being hidden in daemon sessions or
squeezed into unrelated files.

- Why: `io/output`, `requests/`, `actions/`, and `machine/tape` already have
  clear owners. Thinking, plan state, notices, and turn activity are runtime UI
  projections, so they need a runtime-owned surface with explicit semantics.
- Alternative considered: overload the top-level aggregate `events` stream with
  richer ad-hoc records. Rejected because the aggregate stream is intentionally
  cross-cutting and coarse; it is a poor hydration source and would mix watcher
  routing with renderer-specific state.
- Alternative considered: append UI-only records into `machine/tape`. Rejected
  because tape is the prompt-assembly truth, while UI activity and notices are
  not always part of the agent's conversational state.

### Decision: pair snapshot files with a watchable `machine/ui/events` stream

The UI subtree will expose both current snapshot files and an append-only event
stream.

- Why: file-backed renderers need immediate hydration on startup plus ordered
  live updates without daemon replay APIs.
- Alternative considered: snapshots only. Rejected because the renderer would
  have to poll, losing the blocking-read/watchable contract.
- Alternative considered: events only. Rejected because hydration would require
  replaying an unbounded history just to learn the current turn, plan, or notice
  state.

### Decision: project redacted renderer-visible thinking, not provider wire data

When thinking is available, the file surface will expose the same renderer-safe
thinking text the runtime already keeps, never raw provider-native reasoning
wire payloads.

- Why: the renderer host needs parity with current TUI behavior, not a second
  provider protocol.
- Alternative considered: expose no thinking in files. Rejected because it
  preserves daemon-only UI behavior.
- Alternative considered: expose raw provider reasoning payloads. Rejected
  because it would couple renderer hosts to provider-specific formats and bypass
  existing redaction policy.

### Decision: keep local compatibility removal gated on parity verification

The daemon-backed path may remain as an explicit compatibility or remote path
until parity tests cover the new file surfaces, but the local terminal contract
will move to file-backed rendering.

- Why: removing the daemon-backed local path before parity would make regression
  diagnosis harder and could strand remote attach flows.
- Alternative considered: delete the daemon path immediately after adding the
  first new files. Rejected because the remaining parity gap is user-visible and
  test-sensitive.

## Risks / Trade-offs

- `[UI state files drift from runtime truth]` → Keep runtime as the only writer
  of `machine/ui/*`, and test hydration plus live updates together.
- `[Too many tiny files make the subtree awkward to consume]` → Use a small
  snapshot set plus one aggregate `machine/ui/events` stream instead of a large
  matrix of single-purpose files.
- `[Thinking or notices leak provider/internal details]` → Project only the
  same redacted, renderer-safe text already emitted to current clients.
- `[Parity work stalls because compatibility path still exists]` → Make local
  file-backed parity a tracked task list and treat daemon-backed behavior as
  removable debt, not the durable contract.

## Migration Plan

1. Extend AgentFS/runtime with `machine/ui/` snapshot files and a watchable
   `machine/ui/events` stream for activity, plan, thinking, warnings, and
   compaction notices.
2. Write focused engine/AgentFS tests proving these surfaces hydrate correctly
   and stream live updates in order.
3. Teach file-backed `alan-terminal-ui` to hydrate and tail `machine/ui/*`
   directly, replacing the remaining daemon-only local behaviors.
4. Run focused TUI and runtime tests for parity.
5. Remove or demote the daemon-backed local terminal path once parity is
   verified; keep explicit remote/compatibility paths only if still required.

## Open Questions

- Should the final local terminal default switch in this change, or only after a
  subsequent cleanup change removes the explicit compatibility selector?
- Which renderer-visible notices belong in durable snapshot files versus only in
  the append-only `machine/ui/events` chronology?
