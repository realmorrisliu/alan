## Context

Alan already separates terminal runtime identity from transient SwiftUI/AppKit
views. Terminal ContentInstances survive tab switches, spaces, split zoom, and
view reconstruction, and the workspace manifest restores spaces, tabs, split
layout, cwd, launch target, title, and lifecycle metadata after app restart.

Two gaps remain:

1. Close paths can finalize terminal runtimes without first asking when the
   affected terminal has active user work.
2. Restart restore creates a new shell at the last restorable cwd but does not
   restore the visible terminal history that gave the user context before
   closing the app.

Ghostty is a useful reference for safe close confirmation: its macOS controller
uses one `NSAlert` before closing a terminal surface, tab, window, or app when a
surface reports it needs quit confirmation. Ghostty's macOS restorable state,
however, currently centers on pwd, uuid, and title rather than terminal
transcript restore. Warp is a better product reference for restart continuity:
it snapshots window/tab/pane structure, terminal cwd and launch metadata, and
separately restores serialized completed command blocks before creating a fresh
input block. Alan should adopt the same product shape without copying Warp's
block model.

## Goals / Non-Goals

**Goals:**

- Prevent destructive pane, tab, window, app, and Quick Terminal close paths
  from silently killing active terminal work.
- Keep the default idle-shell close path fast and confirmation-free.
- Persist a bounded terminal transcript snapshot before confirmed close or app
  teardown.
- Restore terminal panes after app restart with their prior visible transcript,
  scrollback tail, title, cwd, layout, and focus context.
- Start a new shell in the restored cwd after restart so the pane remains usable.
- Avoid extra restored-session banners or normal-mode UI chrome; restored state
  is visible as terminal history, with metadata reserved for debug/control
  surfaces.
- Preserve the existing terminal ContentInstance and workspace manifest
  authority boundaries.

**Non-Goals:**

- Do not keep PTYs, child processes, Ghostty surfaces, or renderer objects alive
  after the app exits.
- Do not implement tmux-like session attach or daemon-owned terminal processes.
- Do not persist unbounded scrollback, binary renderer state, images, selection
  objects, delivery queues, or secrets.
- Do not change pinned-tab structural restore into automatic template mutation.
- Do not add a user preference UI in this change.

## Decisions

### Close guard owns destructive close decisions

Alan will route pane, tab, window, app, Quick Terminal, menu, shortcut, command
UI, and control-plane close intents through a small close guard before applying
the existing shell mutation. The guard gathers affected terminal ContentInstances
and classifies active work from terminal-aware metadata.

Active work includes foreground commands, running alan sessions, pending yields,
and unknown active-task states when the runtime is still alive. Idle shell
prompts, exited processes, and non-terminal content do not require
confirmation.

Alternative considered: let each close button or command path decide
independently. That would be easy to start but would drift quickly across menu,
keyboard, sidebar, pane title-bar, quick terminal, and automation paths.

### Interactive close confirms once per requested scope

For interactive close, Alan presents one confirmation sheet for the requested
scope. Cancelling the sheet leaves shell state, workspace manifest, and terminal
runtimes unchanged. Confirming runs snapshot capture first, then applies the
existing close mutation and runtime finalization.

Alternative considered: confirm per terminal pane. That gives more detail but is
annoying for split tabs and app quit, and it makes app termination harder to
reason about.

### Automation close is non-destructive by default

Control-plane close commands will return a stable confirmation-required result
when a target contains active terminal work. The command does not mutate shell
state or tear down runtimes. A future explicit force-close contract can opt into
destructive behavior, but this change does not add it.

Alternative considered: allow automation commands to bypass confirmation because
they are usually agent-driven. That creates the highest data-loss risk: an agent
or script can close a pane without seeing the user's running command.

### Transcript restore is a bounded product snapshot

Alan will persist a terminal transcript snapshot in the workspace manifest, not
serialize Ghostty surface or renderer objects. The snapshot stores restorable
terminal history only: dimensions, transcript lines or cells, viewport anchor,
cursor approximation, cwd, title, process summary, last command exit code, and
capture metadata. Size is bounded by row count and encoded byte budget.

Alternative considered: serialize Ghostty terminal state. That could be more
faithful but couples Alan's durable manifest to Ghostty internals, risks
renderer-version incompatibility, and still cannot restore dead child processes.

### Runtime service owns capture and replay seams

The terminal runtime service will expose a snapshot capture API for live
ContentInstance handles and a restore API for startup. Capture first asks the
live terminal surface for a text/scrollback range. If that is unavailable, Alan
can use a lightweight transcript ring buffer maintained by the runtime handle.
App/window teardown performs a best-effort flush before finalization.

On restart, materialization passes the saved snapshot to runtime creation. The
new runtime presents the restored transcript as initial terminal history and
then starts a new shell in the restored cwd. The normal UI does not add a
"restored session" banner; debug metadata may record that the runtime was seeded
from a snapshot.

Alternative considered: render snapshots as a separate non-terminal placeholder
until the user explicitly restarts. That would make process loss clearer, but it
adds visible chrome and blocks the user's next terminal input.

### Pinned templates remain stable

Pinned tab structural restore remains governed by explicit pin snapshots. A
close-time transcript snapshot is session continuity metadata, not a pin-template
mutation. When a pinned tab's saved pin snapshot and close-time live snapshot
share matching terminal ContentInstance identities, Alan may overlay the
transcript snapshot during restart. It must not silently update split layout,
launch target, or cwd in the pin snapshot.

Alternative considered: always restore pinned tabs from close-time live
snapshots. That would satisfy short-term continuity but breaks the existing
contract that pinning creates an explicit stable template.

### Snapshot failure is observable but not close-blocking

After the user confirms a destructive close, failure to capture or write a
snapshot records diagnostics and proceeds with the close. Blocking confirmed
close on persistence failure could trap the user during quit, shutdown, or
corrupt-manifest recovery.

Alternative considered: fail closed and keep the pane alive when snapshot
capture fails. That preserves data in theory but can make app quit unreliable
and conflicts with explicit user confirmation.

## Risks / Trade-offs

- Restored transcript may not be byte-for-byte equivalent to the original
  terminal state -> bound the contract to readable transcript/history, not
  Ghostty renderer fidelity.
- Alternate-screen apps such as editors and pagers can be restored only as their
  last captured text -> record active screen metadata and degrade to transcript
  restore instead of claiming live app recovery.
- Snapshot capture during app quit can race late output -> flush runtime state
  before capture and accept best-effort tail fidelity.
- Large scrollback can make manifest writes slow or huge -> cap row count and
  encoded bytes, prefer tail content, and record truncation metadata.
- Pinned live transcript overlay can be confusing if the pin template no longer
  matches live structure -> only overlay when identities match; otherwise keep
  the pin snapshot behavior.
- No visible restored-session banner can make process loss less obvious -> keep
  process/restored metadata available through debug/control surfaces while
  preserving a normal terminal-first UI.

## Migration Plan

1. Add the manifest model changes with optional snapshot fields and verify old
   manifests decode and restore without snapshots.
2. Add close guard classification and non-mutating confirmation-required results
   behind existing close entry points.
3. Add runtime snapshot capture and replay seams with fake runtime coverage.
4. Wire confirmed close/app teardown to capture bounded snapshots before
   finalization.
5. Materialize restored snapshots into newly created terminal runtimes before
   the new shell becomes interactive.
6. Add focused tests and a running-app quit/relaunch smoke.

Rollback is local to the Apple client: ignore optional transcript snapshot
fields during materialization and route close commands directly to existing
mutations. Old manifests remain readable because the new fields are optional.

## Open Questions

No blocking OpenSpec questions. Exact transcript row and byte limits should be
chosen during implementation with focused performance and UI smoke evidence.
