# Change: Project Workstreams, Root Coordination, and Stateful Cron Streams

## Why

Alan (`alan9`) already ships the underlying file-native primitives for multi-agent work—`/proc` and `/agent/<pid>` child lifecycles, concurrent `spawn_subagent` batches, `alan-memfs` BM25+CJK memory, `alan-routefs` typed message routing, `alan workflow` (`launchd`/`crontab`) scheduling, and explicit project continuity (`/summary`, `/resume`). However, these primitives still operate as disconnected tools:

1. Child runs are short-lived, turn-blocking calls rather than named `persistent` workstreams (reused across turns and sessions) or `one_off` workstreams (auto-collapsed on completion).
2. Starting `alan` in an ongoing project directory requires manual recovery selection rather than presenting a lightweight, read-only overview of active workstreams and recent scheduled results.
3. `routefs` is not connected to Root-mediated cross-workstream handoffs when a user or child stream raises work belonging to another stream.
4. Scheduled `alan workflow` runs execute out-of-band without surfacing digests into the project conversation or supporting guarded stateful continuity across runs.

This change brings cohesive single-conversation project orchestration to Alan while preserving foreground instance ownership, OS-boundary security, explicit Host Mount grants, and calm POSIX/VT terminal ergonomics (`ADR-0062`).

## What Changes

- **Directory-Bound Read-Only Project Ledger (`alan-os-host-lifecycle`)**: Launching `alan` inside an explicitly bound project directory auto-loads a lightweight read-only Project Ledger (active workstream summaries and unread `inbox/` workflow digests) without replaying side effects or bypassing live grant/budget validation. `alan --ephemeral` and `/new` continue to start completely clean sessions.
- **Named Workstreams & In-Instance Async Concurrency (`child-run-lifecycle`)**: Root can asynchronously spawn or resume named `persistent` (checkpointed to `idle` between turns) and `one_off` (auto-archived and collapsed on completion) child Agent Processes inside the foreground instance, flushing all checkpoints cleanly on exit without a background daemon.
- **Tiered Write Isolation & Lease Locks (`child-run-lifecycle`)**: Read-only workstreams execute concurrently; routine writes to the same Host Mount serialize via an in-instance single-writer queue; concurrent long-running write workstreams allocate isolated `git worktree` directories with narrowed Host Mount grants; and per-stream file lease locks prevent interactive vs scheduled checkpoint corruption.
- **Root-Mediated Cross-Stream Handoff via `routefs` (`message-routing`)**: Child workstreams emit typed reroute/handoff documents to `/mnt/route/send` when work belongs to another stream; Root consumes, logs to `/mnt/route/log`, and dispatches to the target stream with a `max_handoff_hops = 2` loop bound and calm inline receipts.
- **Single-Writer `memfs` Delta Governance & Project Inbox (`runtime-memory-surfaces`)**: Child workstreams mount project `memfs` read-only and return structured `memory_proposals` with provenance for atomic Root deduplication and commit; scheduled workflows append bounded redacted digests to the project `inbox/` surfaced on Root startup.
- **Conversational Workflows & Guarded Stateful Cron Streams (`alan-os-host-lifecycle`)**: Root can draft scheduled `WorkflowManifest` definitions conversationally (installed to `launchd`/`crontab` only after explicit confirmation) and optionally bind a recurring task to a dedicated `persistent` Stateful Cron Stream guarded by per-stream lease locks, automatic compaction, strict per-run `task_budget` caps, live grant/posture revalidation, and `max_consecutive_failures` auto-pause alerts.
- **Calm Single-Viewport TUI Controls (`rust-inline-tui`)**: Add compact status-bar stream/inbox badges, inline routing receipts, `/streams` overview, and `/focus <name>` / `/root` non-owning client switching across `/agent/<child_pid>` and `/agent/root`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `alan-os-host-lifecycle`
- `child-run-lifecycle`
- `message-routing`
- `runtime-memory-surfaces`
- `rust-inline-tui`

## Impact

- **Affected specs**: `alan-os-host-lifecycle`, `child-run-lifecycle`, `message-routing`, `runtime-memory-surfaces`, `rust-inline-tui`.
- **Affected crates**: `alan-agent-engine`, `alan-agentfs`, `alan-routefs`, `alan-memfs`, `alan-os-host`, `alan-tui`, `alan`.
- **Backwards compatibility & security posture**: Preserves `alan --ephemeral`, `/new`, explicit `/resume`, foreground-only instance lifecycle (no background Alan daemon), OS sandbox confinement, and Host Mount Service grant ownership. Designed as M8 queued after the active `qualify-personal-daily-development` Qualification milestone (`ADR-0062`).
