## Context

alan currently has a Rust CLI/daemon binary and a separate TypeScript/Bun/Ink
terminal UI under `clients/tui`. The CLI exposes multiple overlapping
interactive entrypoints (`alan`, `alan chat`, and `alan ask`) and release
packaging embeds a standalone `alan-tui` executable.

Codex's terminal UX demonstrates a stronger model for alan's target workflow: a
Rust terminal application with explicit terminal-mode ownership, typed transcript
cells, inline viewport rendering, real terminal scrollback integration, a bottom
composer, event-loop coalescing, and terminal behavior tests. alan should borrow
that interaction architecture while staying inside alan's existing daemon/session
boundary.

The user-facing target is intentionally breaking: the old TUI, old command
surface, and fallback paths are removed rather than preserved.

## Goals / Non-Goals

**Goals:**

- Ship one command-line executable: `alan`.
- Launch the terminal UI from bare `alan`.
- Implement the TUI in Rust and organize it as a crate that is linked into the
  existing `alan` binary.
- Delete the TypeScript/Bun/Ink TUI and all production fallback paths to it.
- Remove `alan chat` and `alan ask`.
- Match Codex's first-order terminal interaction patterns closely enough that
  the first Rust version feels like the same class of tool.
- Keep runtime execution, persistence, tool policy, and session management
  behind the alan daemon/session APIs for V1.
- Replace TypeScript-TUI-specific daemon helper requirements with a Rust-friendly
  shared contract.

**Non-Goals:**

- Do not embed the runtime directly into the TUI process in V1.
- Do not preserve a compatibility wrapper for `alan-tui`, `alan chat`, or
  `alan ask`.
- Do not attempt to copy Codex-specific product features unrelated to alan's
  daemon/session protocol.
- Do not keep the existing Ink UI design, key model, rendering model, or command
  structure as a migration target.

## Decisions

### Decision: Rust TUI crate, single shipped binary

The new TUI will live in a Rust workspace crate such as `crates/tui`, but that
crate will expose a library entrypoint consumed by `crates/alan`. The release
artifact remains the single `alan` binary.

Alternatives considered:

- Keep a separate `alan-tui` Rust binary. Rejected because the desired
  distribution and mental model is one command, and a second binary preserves the
  packaging split we are trying to remove.
- Keep the TypeScript TUI as fallback. Rejected because it keeps the old UI,
  old runtime constraints, and fallback complexity alive.

### Decision: Bare `alan` is the TUI entrypoint

The CLI parser will treat no subcommand as "launch the TUI". Existing management
subcommands remain explicit subcommands. `alan chat` and `alan ask` are removed.

Alternatives considered:

- Keep `alan chat` as a compatibility alias. Rejected because the command surface
  should be reset now instead of carrying ambiguous entrypoints.
- Recreate `alan ask` as a one-shot mode. Rejected for V1; scripted/noninteractive
  use can be reintroduced later with a clearer contract if needed.

### Decision: Daemon-backed TUI for V1

The Rust TUI will talk to the daemon through alan's session APIs:

```text
alan
  explicit subcommand -> existing CLI command
  no subcommand       -> alan_tui::run()
                         -> resolve daemon URL / local lifecycle
                         -> health check or auto-start
                         -> daemon client
                         -> session create/read/events/submit/resume/interrupt
```

The TUI owns presentation and terminal interaction. The daemon owns runtime
startup, workspace resolution, connection profiles, governance, event
persistence, and session lifecycle.

Bare `alan` must preserve the existing interactive startup behavior before it
uses session APIs. For a default or local daemon URL, the TUI first checks
`/health`, starts the local daemon when it is not already reachable, waits for
readiness, and then creates or attaches to a session. For an explicit remote
daemon URL such as `ALAN_AGENTD_URL`, the TUI treats that daemon as externally
managed: it performs health/connect checks against the configured remote and
does not spawn or stop a local daemon.

Alternatives considered:

- Embed `alan-runtime` directly in the TUI. Rejected for V1 because it would
  duplicate daemon orchestration and change more runtime boundaries than needed
  for the UI reset.
- Keep a direct HTTP-only polling TUI. Rejected because the terminal UI needs
  streaming events and reconnection behavior that should use the existing
  event/WebSocket surfaces.

### Decision: Copy Codex interaction architecture, not Codex internals wholesale

alan should mirror the Codex TUI architecture at the interaction level:

- crossterm/ratatui terminal backend;
- explicit raw-mode, paste, and stdin ownership for the primary scrollback mode;
- an internal `AppEvent` bus;
- a main event loop selecting over terminal input, daemon events, app events, and
  background completions;
- typed history cells for assistant text, thinking, tool calls, plans, warnings,
  errors, and pending yields;
- inline viewport plus terminal scrollback insertion;
- bottom composer with multiline editing, slash commands, and pending input
  modes;
- terminal snapshots and vt100-style behavior tests.

alan should not import large Codex modules verbatim as an architectural rule.
Codex has very large UI files; alan should keep module boundaries smaller while
preserving the same UX primitives.

### Decision: Language-neutral daemon client contract

The current daemon API spec names generated TypeScript helpers because the
deleted TUI was TypeScript. The new contract should require shared endpoint and
payload drift protection for shipped daemon clients without making TypeScript the
authoritative TUI surface.

The Rust TUI can satisfy this through Rust endpoint builders, schema snapshots,
contract tests, or generated Rust helpers. The important invariant is that the
Rust TUI does not hand-write canonical `/api/v1/...` paths or silently drift from
daemon payloads.

### Decision: Packaging removes `alan-tui`

Release app assembly, signing, validation, direct app installs, and future cask
metadata must embed/link only `alan`. If another active app-distribution
OpenSpec change or release-script implementation still describes embedded
CLI/TUI binaries, implementation of this change must update or supersede those
requirements before archiving.

## Risks / Trade-offs

- [Risk] Removing `alan chat` and `alan ask` breaks existing scripts or habits.
  -> Mitigation: treat this as an intentional breaking change and update docs,
  shell integration, and tests in the same implementation.
- [Risk] A Codex-like TUI can grow into large hard-to-maintain files.
  -> Mitigation: define module ownership up front: terminal, app loop, daemon
  client, session reducer, history cells, bottom pane, and tests.
- [Risk] Replacing the TUI and command surface in one change is broad.
  -> Mitigation: keep runtime semantics daemon-backed and avoid changing session
  APIs except where Rust-client contract helpers require it.
- [Risk] Terminal behavior regressions are hard to catch with unit tests alone.
  -> Mitigation: add vt100/snapshot-style terminal tests for scrollback, resize,
  streaming deltas, composer behavior, and pending yield surfaces.
- [Risk] Active or future app-distribution work can conflict over `alan-tui`
  packaging.
  -> Mitigation: audit release OpenSpec artifacts, release scripts, and install
  docs during implementation so the app distribution contract is single-binary
  before related changes are archived.

## Migration Plan

1. Add the Rust TUI crate as a library and wire no-subcommand `alan` to it behind
   a minimal compiling shell.
2. Add daemon URL resolution, local daemon health/auto-start, remote override
   handling, the daemon client boundary, and the session reducer before building
   detailed UI components.
3. Implement terminal infrastructure, history cells, bottom composer, and pending
   input surfaces with focused terminal tests.
4. Remove the TypeScript TUI, Bun build paths, `ALAN_TUI_PATH`, `alan-tui`
   packaging, and `alan chat`/`alan ask`.
5. Update macOS shell launch paths, app release scripts, install validation, and
   documentation to the single-binary contract.
6. Update any active app-distribution OpenSpec artifacts, release scripts, and
   install docs to remove `alan-tui` assumptions before archiving.

Rollback is intentionally source-level only: before release, revert this change
or restore the deleted TypeScript TUI from version control. After release, there
is no runtime fallback to `alan-tui`.

## Open Questions

- Which exact Rust helper shape should daemon endpoint construction use:
  hand-maintained Rust builders from the existing registry, generated Rust code,
  or schema snapshot tests around the registry?
- Should the first Rust TUI include a minimal connection/profile picker, or is it
  enough to surface daemon connection errors and rely on `alan connection`
  subcommands for setup?
- Which Codex interactions are required in the first implementation versus later
  parity work: transcript search, image/file attachments, command palette depth,
  resume picker, or advanced session list management?
