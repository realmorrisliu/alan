# Project workstreams, read-only ledger auto-load, routefs handoff and stateful cron streams

Status: proposed 2026-11-24 (post-Qualification / M8 planning; not implemented).
Refines [ADR-0056](0056-bare-alan-attaches-to-root-agent.md) and
[ADR-0060](0060-explicit-delegation-separates-approval-and-isolation.md) for
explicitly bound project directories and opt-in stateful scheduled streams,
while preserving [ADR-0054](0054-retire-desktop-client-prefer-terminal-hosts.md),
[ADR-0058](0058-unify-command-and-agent-interaction.md),
[ADR-0061](0061-terminal-native-os-boundary-quartermaster-and-first-party-providers.md),
[ADR-0006](0006-agent-memory-kind-and-authority-are-separate-axes.md), and
[ADR-0050](0050-host-mount-service-owns-host-file-grants.md). Planning and
documentation only; no behavior below is claimed shipped until its owning change
lands, passes review, and syncs into canonical specs.

## Context

Alan (`alan9`) already provides the file-native substrate for multi-agent work:
`/proc` and `/agent/<pid>` child lifecycles, concurrent `spawn_subagent`
batches, `alan-memfs` (content-addressed BM25+CJK memory), `alan-routefs` (typed
message routing), `alan workflow` (`launchd`/`crontab` scheduled runs), and
explicit project continuity (`/summary`, `/resume`). However, these pieces
currently act as isolated tools rather than a cohesive single-conversation
project experience:

- child runs are short-lived, turn-blocking calls rather than named persistent
  or auto-collapsing workstreams;
- scheduled `alan workflow` runs execute out-of-band without surfacing digests
  into the project conversation;
- `routefs` is not yet wired for Root-mediated cross-stream handoffs; and
- starting `alan` in a bound project directory requires manual history lookup
  instead of surfacing a lightweight read-only project overview.

At the same time, external multi-threaded project implementations have revealed
clear failure modes that Alan must avoid: background cloud sessions that burn
budgets silently, concurrent threads overwriting shared `MEMORY.md` state,
uncoordinated parallel edits creating merge conflicts, and the removal of clean
one-off sessions.

## Decisions

### 1. Directory-bound read-only Project Ledger auto-load

When `alan` starts in an explicitly bound project directory, the foreground
instance automatically loads that project's lightweight, read-only **Project
Ledger** (named workstream cards and unread scheduled workflow `inbox/`
digests) under the existing System Store continuity scope.

- **Read-only context, never live replay or authority**: loading the Ledger
  does not resurrect live child Processes, replay side effects, restore revoked
  grants, or widen trust posture. Current Host Mount grants, OS confinement, and
  `task_budget` limits are revalidated on every invocation and resume.
- **Explicit binding only**: project bindings are tied to an opaque continuity
  key created by explicit user binding, never inferred from Git remotes,
  directory names, or foreign checkouts.
- **Clean sessions preserved**: `alan --ephemeral` and `/new` bypass Project
  Ledger auto-load and leave the bound project's Ledger untouched.

### 2. In-instance asynchronous workstream concurrency without a background daemon

Inside a running foreground `alan` instance, Root may asynchronously spawn or
resume named child workstreams while remaining responsive to further user input
and focus changes.

- **Two workstream lifecycles**: `persistent` workstreams represent ongoing
  topics (such as UI polish or feature tracks) that persist their latest
  checkpoint on turn completion and enter `idle` for later resumption; `one_off`
  workstreams automatically transition to `archived` upon completion and
  collapse out of the active roster while leaving durable rollout evidence
  inspectable.
- **No background Alan daemon**: actual `alan` exit flushes checkpoints and
  cleanly terminates all in-instance active child workstreams. A terminal-host
  view detach (for example `tmux` detaching while retaining the native process)
  keeps the foreground instance alive per ADR-0056 and ADR-0061 without
  spawning a replacement background service.

### 3. Root-owned routing and `routefs`-mediated cross-stream handoffs

Root acts as the project coordinator: given incoming user requests, Root decides
whether to answer directly, resume an existing `persistent` workstream, or spawn
a new `persistent` or `one_off` workstream.

- **Cross-stream handoff via `routefs`**: when a user is focused on workstream A
  and introduces work belonging to workstream B (or A discovers a dependency on
  B), A emits a typed handoff message to `/mnt/route/send`. Root consumes the
  routed message, logs it to `/mnt/route/log`, and dispatches it to B (or
  handles dead-letter fallback).
- **Visible receipts and loop bounds**: every dispatch and handoff emits a calm
  inline receipt in the TUI, remains interruptible and user-reassignable,
  enforces a strict hop cap (`max_handoff_hops = 2`), and never grants
  authority or widens mount scopes.

### 4. Tiered write isolation

Concurrency across workstreams sharing a project Host Mount follows a three-tier
isolation rule:

- **Read-only / research workstreams**: run concurrently without write locks.
- **Routine writes on the same Host Mount**: serialize through an in-instance
  single-writer queue over the primary working tree so small edits incur zero
  Git branch or merge overhead.
- **Concurrent long-running writes**: allocate an isolated `git worktree` on
  demand, bind the child workstream to a narrowed Host Mount covering only that
  worktree, and hand the resulting diff back to Root/user for explicit review
  and merge.

### 5. Single-writer `memfs` governance

Child workstreams mount project `alan-memfs` read-only (`recall/<query>` BM25+CJK
search) so they can consume prior architectural and operational decisions
without concurrent write races. When finishing or yielding, a child may emit
structured `memory_proposals` carrying rollout/checkpoint provenance; Root
deduplicates and atomically commits accepted proposals to `memfs`, preserving
content-addressed revision history and `/summary` inspect, edit, delete, and
rollback controls.

### 6. Conversational scheduled workflows and guarded Stateful Cron Streams

Users may describe recurring tasks directly in Root conversation. Root drafts a
`WorkflowManifest` and installs `launchd`/`crontab` triggers only after
explicit user confirmation and grant validation.

- **Project `inbox/` digests**: scheduled runs write bounded, secret-scrubbed
  digests to the bound project's `inbox/` directory in System Store so Root can
  surface unread updates on the next startup.
- **Stateful Cron Streams (opt-in)**: a scheduled workflow may explicitly bind
  to a dedicated `persistent` workstream checkpoint rather than running
  ephemerally. To bound unattended drift and cost, every Stateful Cron Stream
  run MUST:
  1. acquire a per-stream lease lock so it never corrupts a checkpoint being
     resumed interactively (contention defers cleanly and records an `inbox/`
     notice);
  2. revalidate current Host Mount grants and explicit invocation trust posture;
  3. enforce a strict per-run `task_budget` cap and automatic tape compaction
     threshold; and
  4. auto-pause the scheduled stream and emit an `inbox/` alert after
     `max_consecutive_failures`.

### 7. Calm single-viewport TUI presentation

Preserving ADR-0054 and ADR-0061, Alan introduces no permanent sidebar,
workspace manager, or desktop shell. The inline terminal UI adds:

- a compact status-bar badge (active/idle stream counts and unread `inbox/`
  count);
- calm inline routing/handoff receipts in the transcript;
- `/streams` for bounded on-demand inspection and lifecycle actions; and
- `/focus <name>` and `/root` to switch the non-owning TUI file client between
  `/agent/<child_pid>` and `/agent/root` while preserving terminal scrollback
  and narrow-pane reflow.

### 8. Future Multi-Root and `nsh` App-Hosted GUI Alignment (Post-M8 / M9 Horizon)

M8 delivers these workstream capabilities purely through the CLI and standard
terminal TUI without introducing a background daemon or desktop client. At the
same time, M8's System Store and file-surface contracts are deliberately shaped
to support a future pure-Rust `nsh` `Alan.app` (tentatively M9, unimplemented
and subject to its own ADR/OpenSpec when scheduled):

- **Multi-Root by Project Domain**: M8's per-project isolation
  (`projects/<opaque_project_key>/`, independent `memfs`, independent Host
  Mount grants, and per-stream lease locks) establishes a bounded-domain
  Multi-Root model—avoiding both a machine-wide God Root and stateless one-off
  isolation.
- **App-Hosted Runtime instead of a separate `aland` daemon**: a future `nsh`
  `Alan.app` can host multiple isolated `alan9` Project instances in-process
  via `alan-os-host` (`EmbeddedAlan V2`) and expose local aP Unix sockets
  ([ADR-0045](0045-local-hosts-attach-over-ap-unix-sockets.md)) so terminal
  `alan` can attach as a non-owning file client when the app is open, while
  remaining completely standalone when the app is closed. Closing a GUI window
  detaches views ([ADR-0047](0047-closing-an-agent-view-only-detaches.md));
  quitting the app flushes checkpoints and stops all hosted instances cleanly.
- **File-Tree-Native GUI Iron Law ([ADR-0027](0027-north-star-capability-map.md))**:
  any future `nsh` GUI projects directly from aP file surfaces (`/agent/root`,
  `/agent/<pid>`, `requests/*`, `/mnt/mem`, `/mnt/route/log`, `inbox/*`) with
  symmetric human editing and zero private business state or side-channel RPC
  APIs, while the terminal TUI remains a first-class client.

## Alternatives and consequences

- **Always-on background Alan daemon**: rejected; violates ADR-0056/0061, adds
  orphan-process and stale-grant risks, and obscures token consumption.
- **Global Project/Thread runtime executor**: rejected; System Store Ledger +
  checkpoint references deliver project continuity without reintroducing a
  global conversation manager.
- **Multi-writer `memfs` or unconditional `git worktree` per task**: rejected;
  concurrent `memfs` writes corrupt shared memory, while unconditional worktrees
  add unnecessary merge friction for routine single-file edits.
- **Stateless-only cron runs**: rejected as the sole mode because recurring
  monitoring and triage workflows benefit from cross-day continuity when guarded
  by lease locks, compaction, budget caps, and failure auto-pause.

## Owning changes

- [Project workstreams OpenSpec proposal](../../openspec/changes/add-project-workstreams/proposal.md)
- [Personal daily-use roadmap](../../openspec/changes/qualify-personal-daily-development/roadmap.md)
