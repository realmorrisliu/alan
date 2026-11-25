# Tasks

## 1. Slice 1 — Project Binding, Read-Only Ledger, Named Workstreams, and Inbox (M8.1)

- [x] 1.1 Add System Store project binding (`binding.json`), opaque project key resolution without Git-remote inference, and read-only `ledger.json` auto-load on `alan` startup; test `--ephemeral`, `/new`, unbound directories, and live grant/budget revalidation.
- [x] 1.2 Extend workstream dispatch (`persistent` vs `one_off`) with named checkpoint persistence (`idle`) and automatic `one_off` archival (`archived`) while keeping durable rollout evidence inspectable.
- [x] 1.3 Add project `inbox/` digest persistence for `alan workflow` runs and surface unread digests on Root startup with explicit mark-digested state transitions; test redaction and bounded digest rendering.

## 2. Slice 2 — In-Instance Async Concurrency, `routefs` Handoff, and TUI Focus (M8.2)

- [x] 2.1 Support in-instance asynchronous child workstream execution while Root stays interactive, plus orderly cancellation and checkpoint flush on foreground `alan` exit (preserving `tmux` view-detach behavior without a background daemon).
- [x] 2.2 Wire `alan-routefs` (`/mnt/route/send`, `/mnt/route/ports/root`, `/mnt/route/log`) for cross-stream `reroute_request` and `cross_stream_dependency` handoffs with `max_handoff_hops = 2` dead-letter escalation and zero authority widening.
- [x] 2.3 Implement TUI inline routing receipts, compact status-bar stream/inbox counts, `/streams` overview, and `/focus <name>` / `/root` non-owning client switching over `/agent/<pid>`; verify scrollback, resize reflow, and narrow panes via real Unix PTY (`openpty` + `vt100`) tests.

## 3. Slice 3 — Tiered Write Isolation, `memfs` Delta Governance, and Stateful Cron Streams (M8.3)

- [ ] 3.1 Implement tiered write isolation: concurrent `read_only` streams, per-grant FIFO `shared_queue` serialization, and on-demand native OS CoW workspace (`add-cow-workspaces-and-executable-arena`, falling back to `git worktree`) allocation with narrowed Host Mount grants and explicit conflict-checked sync-back/merge handoff.
- [ ] 3.2 Enforce read-only child `memfs` descriptors and atomic Root deduplication/commit of structured `memory_proposals` with rollout provenance, secret scrubbing, and `/summary` revision rollback compatibility.
- [ ] 3.3 Add conversational `WorkflowManifest` drafting with explicit user confirmation before `launchd`/`crontab` installation, plus guarded Stateful Cron Streams with per-stream lease lock (`streams/<name>.lock`) contention tests, automatic compaction, per-run `task_budget` cap, live grant revalidation, and `max_consecutive_failures` auto-pause alerts to `inbox/`.

## 4. Verification, PR Review, and Archive Readiness

- [ ] 4.1 Run workspace tests (`just test`), `just quality`, real Unix PTY E2E tests (`pty_e2e`), and `just openspec-check` (`openspec validate --all --strict`).
- [ ] 4.2 Complete independent code/architecture review against `ADR-0062` and `ADR-0061` and resolve review findings.
- [ ] 4.3 After implementation PRs merge on `main` and CI passes, sync the five delta specs under `openspec/changes/add-project-workstreams/specs/` into `openspec/specs/` and archive the change under `openspec/changes/archive/YYYY-MM-DD-add-project-workstreams/`.
