# Design: Project Workstreams, Root Coordination, and Stateful Cron Streams

## Context

`alan9` already owns the kernel/process substrate (`/proc`, `/srv`, `/agent/<pid>`), `alan-memfs`, `alan-routefs`, `alan workflow` (`launchd`/`crontab`), and explicit project continuity (`/summary`, `/resume`). What is missing is the coordination layer that turns a project directory into a calm, single-conversation control plane—routing user requests to named persistent or one-off workstreams, letting workstreams hand off misdirected or cross-cutting requests back through Root, surfacing scheduled workflow results, and preventing the failure modes seen in cloud multi-thread products (unbounded background token burn, concurrent `MEMORY.md` corruption, silent code collisions, and loss of clean one-off sessions).

## Goals / Non-Goals

### Goals
- Provide a single-entry Root coordinator experience for explicitly bound projects without introducing a background Alan daemon or global Thread/Conversation runtime object.
- Support both durable thematic workstreams (`persistent`) and self-cleaning ephemeral tasks (`one_off`) with async in-instance execution.
- Connect `alan-routefs` into real Root ↔ Child handoff flows with auditable logs and loop protection.
- Keep shared memory (`alan-memfs`) race-free via read-only child access and Root-owned atomic commits.
- Support conversational workflow drafting and guarded Stateful Cron Streams alongside stateless `inbox/` digests.
- Keep the terminal UI standard-terminal-native (`openpty` + `vt100` verifiable), calm, and single-viewport.

### Non-Goals
- No always-on background Alan host daemon (`ADR-0056`, `ADR-0061`).
- No desktop sidebar, workspace manager, or multi-pane terminal emulator (`ADR-0054`, `ADR-0061`).
- No implicit project binding from Git remotes, repo names, or ungranted foreign checkouts.
- No automatic replay of side effects or restoration of revoked Host Mount grants from saved Ledger metadata.

## Architecture Overview

```text
Foreground `alan` instance (standard POSIX/VT terminal, no background daemon)
├── TUI (non-owning file client) ──[/focus <name> | /root]──▶ /agent/root OR /agent/<child_pid>
│
├── /agent/root (Coordinator)
│   ├── Reads on boot: System Store Project Ledger + unread inbox/ (read-only)
│   ├── Dispatches: inline answer | resume persistent stream | spawn one_off/persistent stream
│   ├── Consumes: /mnt/route/ports/root (cross-stream handoffs from children)
│   ├── Commits: deduplicated memory_proposals ──▶ project memfs (single-writer)
│   └── Manages: per-grant single-writer queue OR on-demand git worktree isolation
│
├── Child Workstreams (/agent/<child_pid>, async in-instance, flushed on exit)
│   ├── Mounts: project memfs (RO), /mnt/route (send-only to Root), narrowed Host Mount
│   ├── Acquires: System Store stream lease lock (streams/<name>.lock)
│   └── Returns: handoff result + optional memory_proposals + optional worktree diff ref
│
└── Scheduled `alan workflow` (`launchd` / `crontab` -> `alan -p ...`)
    ├── Stateless project run ──▶ appends redacted digest to System Store inbox/
    └── Stateful Cron Stream (opt-in) ──▶ acquires stream lease lock -> resumes stream checkpoint
                                        (compaction + budget cap + grant revalidation + fail pause)
```

## System Store Layout & Data Contracts

Project state lives under the System Store continuity tree indexed by an opaque project key (`<opaque_project_key>`), never inside a Host project directory:

```text
System Store/services/projects/<opaque_project_key>/
├── binding.json              # Explicit directory binding (canonical root path + grant ref + created_at)
├── ledger.json               # Bounded summary index of streams + latest state
├── streams/
│   ├── <stream_name>.json    # WorkstreamRecord metadata & checkpoint pointer
│   └── <stream_name>.lock    # Advisory/flock lease lock for interactive vs cron arbitration
└── inbox/
    └── <run_id>.json         # Bounded, secret-scrubbed scheduled workflow digest
```

### Core Record Contracts

- **`WorkstreamRecord` (`streams/<stream_name>.json`)**:
  - `version: u16`
  - `name: String` (slug, unique within project)
  - `scope: String` (short human-readable responsibility summary for Root routing)
  - `persistence: "persistent" | "one_off"`
  - `state: "running" | "idle" | "archived" | "paused_error"`
  - `last_checkpoint_ref: Option<String>` (durable rollout/checkpoint reference)
  - `rollout_ref: Option<String>` (latest evidence path)
  - `write_mode: "read_only" | "shared_queue" | "worktree"`
  - `worktree_ref: Option<String>` (worktree branch/path metadata when `write_mode == "worktree"`)
  - `budget_profile: Option<String>` / `model_override: Option<String>`
  - `cron_binding: Option<CronStreamPolicy>` (`workflow_id`, `max_consecutive_failures`, `consecutive_failures`, `compaction_threshold_tokens`, `per_run_budget_usd`)
  - `updated_at: String` (RFC 3339 timestamp)
- **`RouteHandoffMessage` (written to `/mnt/route/send`)**:
  - `version: u16`
  - `kind: "reroute_request" | "cross_stream_dependency" | "handoff_result"`
  - `source_stream: String`
  - `suggested_target: Option<String>`
  - `hop_count: u8` (must satisfy `hop_count <= 2`; exceeded hops route to `dead-letter` and escalate to Root)
  - `correlation_id: String`
  - `payload: String`
- **`MemoryProposal` (attached to child handoff)**:
  - `kind: "decision" | "convention" | "pitfall" | "file_index"`
  - `summary: String`
  - `provenance_rollout: String`
  - `supersedes: Option<String>`
- **`InboxDigestEntry` (`inbox/<run_id>.json`)**:
  - `run_id: String`, `workflow_id: String`, `stream_name: Option<String>`, `status: "ok" | "warning" | "paused_error"`, `summary: String`, `evidence_ref: String`, `created_at: String`, `digested_at: Option<String>`.

## Key Decisions & Guardrails

1. **Explicit Binding & Clean Session Escape Hatches**:
   - A project directory is bound explicitly (e.g., via project init/bind action or explicit prompt confirmation on first grant). Foreign worktrees or clones with matching Git remotes never auto-bind (`restore-project-continuity` invariant).
   - Bare `alan` in a bound directory loads `ledger.json` + unread `inbox/` as read-only Root context. `alan --ephemeral` and `/new` skip Ledger loading and never mutate `ledger.json`.
   - Every stream resumption revalidates live Host Mount grants, OS confinement, and remaining `task_budget`.

2. **In-Instance Async Concurrency & Clean Exit**:
   - Root dispatches child workstreams onto background tokio tasks backed by child Agent Processes (`/proc/<pid>`, `/agent/<pid>`). Root remains interactive while children execute.
   - When a `persistent` stream finishes a turn, its checkpoint updates and `state` becomes `idle`. When a `one_off` stream finishes, `state` becomes `archived`, collapsing it from the active Ledger view while keeping `rollout_ref` inspectable.
   - Actual `alan` process exit cancels/quiesces active children, flushes durable checkpoints, and tears down the foreground instance cleanly.

3. **Visible Routing & `routefs` Cross-Stream Handoff**:
   - Root evaluates each unprefixed user message against `ledger.json` scopes and routes it inline, to an existing stream, or to a newly spawned stream.
   - If the user is focused on stream A (`/focus A`) and sends a prompt belonging to stream B, stream A emits a `reroute_request` to `/mnt/route/send` instead of answering out-of-scope or silently calling sibling B. Root receives the message from `/mnt/route/ports/root`, records the decision in `/mnt/route/log`, updates focus or dispatches to B, and emits an inline receipt (`↳ rerouted from [A] → [B]`).
   - `hop_count > 2` routes to `dead-letter` and surfaces to Root for human clarification.

4. **Tiered Write Isolation**:
   - `read_only`: unlimited concurrency inside the foreground instance.
   - `shared_queue`: default for small/everyday edits targeting the same Host Mount; Root maintains an in-instance FIFO queue per grant so only one writer edits the primary working tree at a time.
   - `worktree`: when a second long-running write task is requested while a `shared_queue` writer is active (or when requested explicitly), Alan allocates a native OS CoW workspace (`openspec/changes/add-cow-workspaces-and-executable-arena/`, with `git worktree` fallback when CoW cloning is unavailable), binds the child Process to a narrowed Host Mount for that workspace path only, and returns a diff/sync-back summary to Root on completion.

5. **Single-Writer `memfs` Governance**:
   - Child workstreams receive read-only `memfs` descriptors (`recall/<query>`).
   - Root validates, secret-scrubs, deduplicates, and commits `memory_proposals` to `memfs` with provenance pointing to the child `rollout_ref`, preserving `/summary` user inspect/edit/delete and content-addressed revision rollback.

6. **Conversational Workflows & Guarded Stateful Cron Streams**:
   - Root drafts a `WorkflowManifest` from natural language and displays schedule, target project, trust posture, budget cap, and grant scope for explicit user confirmation before calling `alan workflow` installation (`launchd`/`crontab`).
   - Stateful Cron Streams acquire `streams/<stream_name>.lock` non-blockingly (or with short bounded wait). If an interactive `alan` session currently holds the stream lock, the cron invocation aborts cleanly without mutating the checkpoint and logs a contention notice to `inbox/`.
   - On every cron wake: (a) revalidate live Host Mount grants and trust posture; (b) run automatic tape compaction if above `compaction_threshold_tokens`; (c) enforce strict `per_run_budget_usd`; (d) increment `consecutive_failures` on error and transition stream to `paused_error` + write an `inbox/` alert once `max_consecutive_failures` is reached.

7. **Calm Single-Viewport TUI**:
   - No permanent sidebar. Default view stays the inline transcript + contextual prompt (`alan: ` / `alan! `).
   - Footer status bar adds a concise indicator (`streams: <active> active · <idle> idle · <inbox> inbox`).
   - `/streams` prints a compact inline table/list (active, idle, archived count, unread inbox items) with quick actions (focus, archive, clear inbox, resume paused cron stream).
   - `/focus <name>` switches the TUI's mounted `/agent/<pid>` path to an active or resumed child stream (and `/root` returns to `/agent/root`) while keeping scrollback coherent across switches and narrow terminal resizes.

## Future Multi-Root and `nsh` App-Hosted GUI Alignment (Post-M8 / M9 Horizon)

While M8 targets the CLI and standard POSIX/VT terminal TUI exclusively (no background daemon and no desktop client in M8), its System Store contracts and file boundaries are aligned with a potential post-M8 `nsh` `Alan.app` host (M9, unimplemented and outside M8 scope):

- **Multi-Root by Project Domain**: `System Store/services/projects/<opaque_project_key>/`, per-project `memfs`, independent Host Mount grants, and per-stream `streams/<name>.lock` leases keep every Project Root self-contained so multiple Project Roots can coexist without a machine-wide God Root.
- **App-Hosted Runtime over aP Unix Sockets**: A future pure-Rust `nsh` `Alan.app` may host multiple isolated `alan9` Project instances in-process via `alan-os-host` (`EmbeddedAlan V2`) and expose per-instance aP Unix sockets (`ADR-0045`), allowing terminal `alan` to attach as a non-owning file client when the app is running while staying fully standalone when it is not. Window close detaches views (`ADR-0047`), and application quit flushes checkpoints and terminates hosted instances cleanly instead of leaving a separate `aland` daemon behind.
- **File-Tree-Native Projection (`ADR-0027`)**: Any future `nsh` GUI projects directly from `/agent/root`, `/agent/<pid>`, `requests/*`, `/mnt/mem`, `/mnt/route/log`, and `inbox/*` with symmetric human edits and no private business state or side-channel RPC APIs; the terminal TUI remains a first-class client.

## Phased Delivery Plan

- **Slice 1 (M8.1 — Ledger, Named Workstreams, Inbox)**:
  - System Store project binding + read-only `ledger.json` auto-load on `alan` startup (`--ephemeral` / `/new` bypass).
  - Named `persistent` (checkpointed `idle`) and `one_off` (auto-`archived`) workstreams via extended `spawn_subagent` / `dispatch_stream`.
  - Project `inbox/` directory + `alan workflow` digest emission + Root startup summary.
- **Slice 2 (M8.2 — Async Concurrency, `routefs` Handoff, TUI Focus)**:
  - In-instance async child workstream execution + clean shutdown checkpoint flush.
  - Wire `alan-routefs` between child streams and Root for `reroute_request` / `cross_stream_dependency` with `max_handoff_hops = 2` and `/mnt/route/log` auditability.
  - TUI inline routing receipts, status-bar badge, `/streams`, and `/focus <name>` / `/root`.
- **Slice 3 (M8.3 — Tiered Worktrees/CoW, `memfs` Delta Commits, Stateful Cron Streams)**:
  - Per-grant single-writer queue + on-demand native OS CoW workspace (with `git worktree` fallback) narrowed Host Mount isolation + 3-way conflict-checked sync-back / merge handoff (coordinated with `openspec/changes/add-cow-workspaces-and-executable-arena/`).
  - Child read-only `memfs` + structured `memory_proposals` atomic Root commit.
  - Conversational `WorkflowManifest` drafting + explicit confirmation + Stateful Cron Streams with per-stream lease locks, compaction, budget cap, and failure auto-pause.

## Risks / Trade-offs

| Risk | Mitigation |
| :--- | :--- |
| Root misroutes an unprefixed prompt to the wrong stream | Always emit calm inline routing receipt + allow instant Esc interrupt and `/focus` reassignment; keep `one_off` cheap and auto-collapsed. |
| Interactive `alan` and cron run race on the same persistent checkpoint | Mandatory per-stream `flock`/lease lock (`streams/<name>.lock`) + live grant revalidation; cron defers to interactive session and logs to `inbox/`. |
| Stateful Cron Stream context bloats or silently burns budget across days | Enforce `compaction_threshold_tokens`, strict `per_run_budget_usd`, and `max_consecutive_failures` auto-pause. |
| Parallel write workstreams collide on working tree | Default single-writer FIFO queue per Host Mount; escalate to narrowed `git worktree` mount only when concurrent writes are needed. |
| Users feel trapped in a single monolithic project chat | Preserve `alan --ephemeral`, `/new`, `/focus`, and `/streams` one-command reset/isolation at all times. |
