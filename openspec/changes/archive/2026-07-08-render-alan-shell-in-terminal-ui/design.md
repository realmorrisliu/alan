## Context

`alan-shell` now proves the namespace-native client model, but `alan-terminal-ui`
still owns a daemon/session-first runtime contract: it creates or attaches to a
session, hydrates from daemon APIs, reduces `EventEnvelope` streams, and submits
operations back through the daemon. That is useful as a compatibility path, but
it conflicts with ADR-0025's target client layer where renderer hosts read files
and write `ctl`.

The codebase already has most of the lower layers needed for a first file-backed
slice:

- `alan-shell` provides generic aP builtins over one mounted namespace.
- `alan-agentfs` exposes `/agent/<pid>/io/input`, `/io/output`, `requests/`,
  `actions/`, and `machine/ctl`.
- the namespace-native runtime bootstraps `/proc`, `/agent`, `/srv`, and
  `/mnt/llm`.

What is missing is a narrow launch path that hands a renderer host a live aP
root plus a concrete agent path, and a TUI mode that consumes that surface
without going through daemon sessions first.

## Goals / Non-Goals

**Goals:**

- Add a local file-backed `alan-terminal-ui` mode that renders one live agent
  conversation by tailing `io/output`, writing `io/input`, and writing generic
  process control to `/proc/<pid>/ctl`.
- Keep the existing daemon-backed TUI path intact as a compatibility mode while
  the file-backed path is introduced.
- Expose the minimal runtime bootstrap handle needed by renderer hosts: runtime
  controller lifetime, aP root transport, and the concrete root-agent path.
- Keep the implementation narrow enough to verify end-to-end in one slice.

**Non-Goals:**

- Do not make the file-backed mode the default bare `alan` path yet.
- Do not build full file-native parity for structured yields, rollback,
  compaction, reconnect hydration, or remote attachment in this slice.
- Do not redesign the macOS client or the daemon API in this change.
- Do not turn `alan-terminal-ui` into a generic command-shell REPL yet; the
  first slice is a file-backed conversation renderer over the same file
  operations.

## Decisions

### Decision: introduce an explicit local file-backed TUI mode beside the daemon compatibility path

The first slice adds a backend selector for `alan-terminal-ui` instead of
replacing the current bare-`alan` behavior immediately.

- Why: the current daemon-backed path has richer UX coverage today, while the
  file-backed path needs a smaller end-to-end landing.
- Alternative considered: switch bare `alan` directly to file-backed mode.
  Rejected because it would couple the architecture migration to a broad UX
  parity bet in one step.

### Decision: expose a runtime namespace launch handle from `alan-agent-engine`

The runtime crate will expose a small launch helper that returns:

- the running `RuntimeController`
- the aP root transport for the mounted namespace
- the concrete root agent path (for example `/agent/1`)

This keeps namespace assembly in the runtime layer that already owns it, instead
of re-implementing bootstrap logic in `crates/alan` or `crates/tui`.

- Alternative considered: rebuild `/proc` + `/agent` + `/mnt/llm` wiring in
  `crates/alan`. Rejected because it would duplicate the runtime's accepted
  namespace bootstrap.

### Decision: the first file-backed renderer slice is conversation-scoped, not a generic shell REPL

The file-backed TUI mode will use file operations under the hood, but it will
render a focused conversation surface:

- submit by writing text to `<agent>/io/input`
- read assistant text by tailing `<agent>/io/output`
- interrupt by writing `interrupt` to `/proc/<pid>/ctl`

This is still a renderer-host model because the surface is defined by files and
`ctl`, not daemon sessions or semantic snapshots.

- Alternative considered: render `alan-shell`'s line-oriented REPL directly
  inside Ratatui. Rejected for this slice because it would spend most of the
  work on shell-command presentation rather than proving the file-backed agent
  conversation path.

### Decision: keep the existing daemon-backed TUI code isolated rather than partially merging event models

The new file-backed mode will have its own narrow runner instead of trying to
mix daemon `EventEnvelope` reduction with file polling/tailing inside one state
machine.

- Why: the two modes have different truth sources, and mixing them would make
  the compatibility boundary harder to remove later.
- Alternative considered: synthesize fake daemon events from file reads and feed
  them through the existing reducer. Rejected because it preserves the
  session/event-first architecture instead of making the file boundary explicit.

## Risks / Trade-offs

- `[Reduced feature parity in file-backed mode]` → Keep the mode explicit and
  local-only in this slice; document that daemon-backed mode remains the richer
  compatibility path for now.
- `[Runtime bootstrap surface leaks too much internal detail]` → Expose only the
  root transport, concrete agent path, and controller lifetime, not broader
  runtime internals.
- `[Two TUI paths drift]` → Keep the file-backed runner intentionally small and
  add focused tests around its IO contract.
- `[User confusion about which mode they are in]` → Render a clear live notice
  that the file-backed path is the local namespace-native preview.

## Migration Plan

1. Add the renderer-host spec and update `rust-inline-tui` so daemon-backed
   operation is treated as compatibility, not the terminal contract.
2. Expose the runtime namespace launch handle from `alan-agent-engine`.
3. Add a file-backed TUI runner in `crates/tui` plus an explicit CLI/backend
   selector in `crates/alan`.
4. Verify the new path with focused tests and keep daemon-backed mode intact.
5. In later changes, expand file-backed parity (requests/forms/history/reconnect)
   and only then consider switching the default.

## Open Questions

- When the file-backed path reaches request/form parity, should bare `alan`
  switch defaults or choose mode by local-vs-remote context?
- Should later renderer-host work expose generic shell panes directly, or keep
  conversation-specific and shell-specific renderers separate above the same
  aP/file boundary?
