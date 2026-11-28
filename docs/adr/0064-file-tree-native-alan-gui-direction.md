# File-tree-native Alan GUI direction (Calm HITE Workbench on nsh)

Status: proposed 2026-11-28 (post-Qualification horizon; not implemented).
Expands [ADR-0062](0062-project-workstreams-ledger-and-stateful-cron-streams.md) §8 while
preserving [ADR-0027](0027-north-star-capability-map.md),
[ADR-0045](0045-local-hosts-attach-over-ap-unix-sockets.md),
[ADR-0047](0047-closing-an-agent-view-only-detaches.md),
[ADR-0054](0054-retire-desktop-client-prefer-terminal-hosts.md),
[ADR-0061](0061-terminal-native-os-boundary-quartermaster-and-first-party-providers.md), and
[ADR-0063](0063-actor-fileserver-multi-writer-and-event-supervision.md).
Planning and architectural direction only; no GUI or new runtime behavior is claimed shipped
until its owning OpenSpec change lands, passes review, and syncs into canonical specs.

## Context

The 2026 Agentic Development Environment (ADE) landscape reflects a shared shift: developers
are moving from supervising a single synchronous coding chat to orchestrating concurrent,
long-running Agent Processes. Existing ADE products cluster around four architectural
patterns, each carrying distinct structural trade-offs:

1. **Parallel worktree orchestrators (e.g., Conductor)**: provide strong diff and execution
   observability across isolated `git worktree` checkouts, but remain single-player
   Human-in-the-Loop thread babysitters with weak cross-task continuity.
2. **Issue-board multi-agent platforms (e.g., Multica)**: establish durable ticket lifecycles
   and reusable team skills, but introduce heavy server, database, and kanban ceremony for
   individual daily workflows.
3. **Chat-first multi-agent workspaces (e.g., Raft)**: reduce management friction through
   channels and threads, but suffer always-on group-chat token amplification and weak
   structured observability at code boundaries.
4. **Hybrid group-chat + isolated worktree task board + global supervisor models (e.g., Eta)**:
   combine chat alignment with worktree task boards and a cross-project supervisor, but expose
   structural split-brain friction across the chat/task boundary (missing conversation context
   inside isolated tasks, ambiguous `@mention` and wake semantics, ad-hoc state machines) and
   rely on an ambient cross-project supervisor.

Alan already possesses a cleaner substrate on `main`: `alan9` namespaces, `/proc` and
`/agent/<pid>` process views, M8 Project Workstreams and read-only Project Ledger
([ADR-0062](0062-project-workstreams-ledger-and-stateful-cron-streams.md)), `routefs`
cross-stream handoffs, single-writer `memfs`, Tier-3 native OS Copy-on-Write (`clonefile(2)` /
`FICLONE` with `git worktree` fallback) and `branchfs` isolation, plus Wave 2
([ADR-0063](0063-actor-fileserver-multi-writer-and-event-supervision.md)) mount-time
`AgentActorView::Client` write boundaries, `OpenMode::WriteExclusive` leases, and event-driven
`/proc/<pid>/wait` supervision.

Equally important, Alan's interaction model is **Human-in-the-End (HITE)**:
`Human Defines -> Agent Executes -> Human Owns`. Normal reversible work runs silently inside
confined OS sandboxes, explicit Host Mount grants, `ConfinedSmart` posture, and `task_budget`
limits; human attention is reserved for commit boundaries, exceptions (`Yield`), and evidence
verification. One remaining daily-use friction in `alan9` is explicit boundary ergonomics
(authorizing Host Mounts and inspecting active namespace boundaries without loosening security).
A well-scoped GUI solves that friction directly without reviving a terminal emulator or an
anthropomorphic chat room.

## Decisions

### 1. Calm HITE Control Plane on `nsh`, not a Terminal Shell or Group-Chat Room

Future `Alan.app` is a pure-Rust `nsh` desktop workbench centered on exception-driven human
oversight, boundary ergonomics, and evidence verification.

- **Not a terminal host**: `Alan.app` does not embed a PTY emulator, tab manager, or split-pane
  host (preserving [ADR-0054](0054-retire-desktop-client-prefer-terminal-hosts.md)). Ordinary
  POSIX/VT terminals remain first-class hosts for terminal-native work.
- **Not a simulated human company**: `Alan.app` does not model agents as persona avatars in
  group-chat rooms exchanging `@mention` messages, nor does it introduce a cross-project
  supervisor.

### 2. File-Tree-Native Iron Law and Symmetric TUI/GUI Coexistence

Per [ADR-0027](0027-north-star-capability-map.md) and
[ADR-0062](0062-project-workstreams-ledger-and-stateful-cron-streams.md) §8, `Alan.app` holds
zero private business database and uses zero side-channel RPC APIs:

- **Pure `aP` file projection**: every panel projects strictly from `aP` file surfaces
  (`/proc`, `/agent/root`, `/agent/<pid>`, `requests/*`, `/mnt/mem`, `/mnt/route/log`,
  `inbox/*`, and Project Ledger) under `AgentActorView::Client` (write-open restricted to
  `io/input`, `machine/ctl`, and `requests/<id>/response` per
  [ADR-0063](0063-actor-fileserver-multi-writer-and-event-supervision.md)) and event-driven
  waits (`/proc/<pid>/wait`, `/agent/<pid>/events`).
- **App-Hosted Multi-Root without a background daemon**: `Alan.app` hosts isolated `alan9`
  Project instances in-process via `alan-os-host` (`EmbeddedAlan`) on demand when a bound
  project is opened or triggered. Closing a GUI window only detaches the view
  ([ADR-0047](0047-closing-an-agent-view-only-detaches.md)); quitting `Alan.app` flushes
  checkpoints and cleanly stops all hosted instances. No always-on `aland` background daemon
  is introduced.
- **Symmetric terminal attach**: each hosted `alan9` instance exposes its local `aP` Unix
  socket ([ADR-0045](0045-local-hosts-attach-over-ap-unix-sockets.md)) so any ordinary
  terminal can `alan attach` into the exact same live Root or child Agent Process as a
  non-owning client.

### 3. Attention-First Information Architecture (3-Zone Layout + `⌘K` Command Palette)

The workbench adopts a calm three-zone layout (with the right inspector collapsed by default)
plus a `⌘K` command palette:

- **Left Rail (Attention Priority)**:
  - `Needs You`: unified HITE commit-boundary and exception queue aggregating pending
    `requests/*` across active projects (action approvals, Host Mount requests, agent
    Yields, and Tier-3 CoW/worktree sync-back reviews).
  - `Inbox`: quiet digests emitted by Stateful Cron Streams along with budget and
    auto-pause alerts.
  - `Projects`: explicitly bound domain-isolated Multi-Root list displaying read-only Project
    Ledger badges (active/idle stream counts, unread `inbox/` count) with on-demand instance
    launch, plus a clean `--ephemeral` scratchpad entry.
- **Center Canvas (Unified Root + Workstreams + Commit-Boundary Review)**:
  - Compact Project Ledger strip showing `persistent` and `one_off` workstreams, current
    Approval Posture, and budget gauge, with single-click Focus switching (`/root` <->
    `/agent/<child_pid>`).
  - Single authoritative Project Root conversation (`/agent/root`) emitting calm inline
    receipts for `routefs`-mediated child workstreams (isolation tier, status, elapsed time,
    token usage). Selecting a receipt inspects or steers `/agent/<child_pid>` directly; because
    Root and child workstreams share `/proc`, Project Ledger, `routefs/log`, and `memfs`,
    there is no chat-vs-task split brain.
  - Commit-Boundary Evidence & Diff Review view for Tier-3 CoW / `branchfs` runs: structured
    file diffs, Checkpointed Reasoning provenance (policy decisions, executed commands,
    rollout evidence), and direct actions (`Sync-back / Merge`, `Reject & Steer`, `Rollback`,
    or content-addressed `Checkpoint Fork`).
- **Right Inspector (Collapsed by Default — `alan9` X-Ray)**:
  - `Boundaries & Mounts`: live namespace mounts, bound `q` packages, Execution Isolation
    posture, and `task_budget` caps, including drag-and-drop Host Mount authorization cards
    (scoped one-off vs project-bound, read-only vs read-write) to eliminate explicit-mount
    friction without ambient host access.
  - `Memory`: `memfs` `Project Continuity Summary` search, human-sovereign edit/rollback, and
    review of child `memory_proposals`.
  - `Live Process & Trace`: `/proc` process tree, `/mnt/route/log`, `/agent/<pid>/machine/tape`,
    and retained tool evidence.
  - `Terminal Handoff`: one-click `alan attach` socket handoff to any standard terminal.

### 4. Coexistence with Vertical Alan Apps

Purpose-built vertical Alan Apps (such as `M3` and `SuperReader`) keep their domain-native
UX built over `EmbeddedAlan` and app-owned file-server adapters. `Alan.app` serves as the
personal computing workbench for cross-project workstreams, explicit boundaries, Memory
Stores, Quartermaster `q` packages, LLM Connections, and HITE exception triage—never forcing
vertical domain workflows into generic chat.

### 5. Phased Delivery and Sequencing Guardrails

1. **Phase 0 (Prerequisite)**: Complete terminal-native `Qualification` (`>=50` real daily
   tasks in [the personal daily-use roadmap](../../openspec/changes/qualify-personal-daily-development/roadmap.md))
   and dogfood M8 workstreams in CLI/TUI before starting GUI implementation.
2. **Phase 1 (`Alan.app` v0.1 — Thin HITE Companion)**: App-Hosted Multi-Root + `aP` Unix
   socket export; `Needs You` (Yield/approval + drag-and-drop Host Mount) + `Inbox`; Root &
   Workstream inline-receipt canvas; Tier-3 CoW Diff & Rollout Evidence review.
3. **Phase 2 (`Alan.app` v0.2 — `alan9` Differentiators)**: visual `memfs` proposal review and
   rollback, Checkpoint branching/forking, Quartermaster `q` & Connection management, and
   lightweight federation with vertical Alan Apps.

## Explicit Non-Goals

- No embedded PTY terminal emulator, tab bar, or window/pane manager.
- No always-on background `aland` daemon.
- No anthropomorphic multi-agent group chat rooms or `@mention` coordination theater.
- No machine-wide God Root or cross-project "Captain" supervisor.
- No private GUI business database or non-file side-channel RPC API.
- No mandatory GUI dependency; standard-terminal CLI/TUI remains a first-class surface.

## Alternatives and Consequences

- **Multi-agent group-chat ADE (Raft / Eta style)**: rejected; introduces chat-vs-task
  split-brain, persona coordination noise, and background token amplification that conflict
  with HITE and single-Root `routefs` clarity.
- **Team kanban / ticket-board ADE (Multica style)**: rejected; adds unnecessary server and
  ticket-workflow overhead for a local-first personal computing environment.
- **Rebuilding a terminal-host shell (retired Alan for macOS)**: rejected per
  [ADR-0054](0054-retire-desktop-client-prefer-terminal-hosts.md); mixes PTY presentation
  lifecycle with agent semantics.
- **Terminal-only forever**: rejected as an artificial ceiling; keeping the CLI/TUI primary
  while adding a file-tree-native `nsh` workbench later lets visual diffs, drag-and-drop Host
  Mounts, and cross-project HITE triage complement terminal work without architectural drift.
  The trade-off is maintaining strict `aP`-only discipline in `Alan.app` so GUI features never
  leak private state outside `alan9`.

## References

- [ADR-0027: North Star Capability Map](0027-north-star-capability-map.md)
- [ADR-0054: Retire the desktop client and prefer existing terminal hosts](0054-retire-desktop-client-prefer-terminal-hosts.md)
- [ADR-0061: Terminal-native Alan, OS-boundary security, Quartermaster and first-party providers](0061-terminal-native-os-boundary-quartermaster-and-first-party-providers.md)
- [ADR-0062: Project workstreams, read-only ledger auto-load, routefs handoff and stateful cron streams](0062-project-workstreams-ledger-and-stateful-cron-streams.md)
- [ADR-0063: Actor-isolated FileServers, multi-writer boundaries, and event-driven process supervision](0063-actor-fileserver-multi-writer-and-event-supervision.md)
- [Personal daily-use roadmap](../../openspec/changes/qualify-personal-daily-development/roadmap.md)
