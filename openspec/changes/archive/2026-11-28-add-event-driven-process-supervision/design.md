# Design: Event-Driven Process Supervision (`/proc/<pid>/wait`, `link_parent`, `SupervisedHandle`)

## Context

This change implements **Wave 2 / Change 2** under
[ADR-0063](../../../docs/adr/0063-actor-fileserver-multi-writer-and-event-supervision.md),
replacing blind timer polling with aP-native event observation and dependency-aware
supervision (`#1190` Slice B + Slice C polling removal).

```text
  SupervisorRuntime / ChildRunSupervisor / async_workstreams
         │                             ▲
         │ blocking read (offset=0)    │ terminal JSON line on exit
         ▼                             │
   /proc/<pid>/wait  ──(ProcessTable exit / parent exit cascade)──▶ linked children
         │
         ▼
   /srv/<name> via SupervisedHandle (generation: u64; stale fids -> ErrorCode::Io)
```

## Goals / Non-Goals

### Goals
- Expose `/proc/<pid>/wait` as a race-free read-only `FileKind::Stream` inside `alan-kernel`.
- Add opt-in `ExecSpec.link_parent: bool` (`false` by default) to deterministically cancel linked child processes on **any** parent exit.
- Generalize `SwitchableFileServer` -> `SupervisedHandle` across all `/srv/*` handles with monotonic `generation: u64` fid checks.
- Drive `ServiceManager` readiness, restarts, strict reverse-topological `stop()`, default `OneForOne` recovery, and opt-in `RestForOne` via `/proc/<pid>/wait` + `/srv` readiness notifications instead of `25ms`/`10ms` ticks.
- Project `generation`, `restarts`, `backoff_until`, and `last_exit_reason` under `/mnt/service-manager/units/<name>/`.
- Replace `10–250ms` polling in `DelegatedChildRunSupervisor` and `10ms` quiesce polling in `async_workstreams` with `/proc/<pid>/wait` and `/agent/<pid>/events`.

### Non-Goals
- No unconditional global `RestForOne` across all `after` edges (prevent leaf restarts such as `package` or `route` from dropping live `root-agent`/`local-entry` sessions when `SupervisedHandle` isolates the crash).
- No `AgentMachine` `gen_statem` transition/queue rewrite (`#1190` Slice C part 2 remains deferred).
- No global process registry or out-of-band lifecycle RPC outside aP/kernel.

## Design Decisions

### 1. `/proc/<pid>/wait` Stream (`crates/kernel`)
- Add `Node::Wait(Pid)` under `/proc/<pid>/wait` and `/proc/self/wait` as read-only `FileKind::Stream` backed by `alan_ap::Stream`.
- Reading while alive blocks until process termination; reading after termination immediately returns retained bytes from offset `0` (zero race window between exit and subscriber attach).
- Emits a single newline-delimited JSON record:
  `{"pid":<u64>,"status":"exited","exit_code":<i32>,"reason":"<normal|failed|cancelled>"}\n`.
- Read-only enforcement at dial time returns `ErrorCode::NoAccess` for write opens (consistent with Change 1 `ProcFs::open` alignment).

### 2. Declarative `ExecSpec.link_parent: bool` (`crates/kernel`)
- Add `#[serde(default)] pub link_parent: bool` to `ExecSpec` (default `false` preserves standalone tools, scheduled workflows, and legacy spawns).
- When `link_parent == true` (and `parent_pid` exists), exiting the parent process — whether `normal`, `failed`, or `cancelled` — triggers kernel-level cancellation of live linked descendants and closes their `/proc/<child>/wait` streams deterministically.
- Works both via `ProcFs` spawn APIs and `/proc/clone` commits.

### 3. `SupervisedHandle` & `SupervisionTree` (`crates/service-manager`)
- **Generation-isolated `/srv/*` proxy (`SupervisedHandle`)**: wraps active backing `FileServer` plus `generation: u64`. Every `walk`/`open` records the handle generation on the fid (or maps fids against generation); `deactivate()` clears the active target and future rebind bumps `generation += 1`. Any operation using a pre-restart fid fails cleanly with `ErrorCode::Io`, while a fresh `walk` binds transparently to the restarted server.
- **BootUnit strategy field**: add `#[serde(default)] pub supervision: SupervisionStrategy` (`OneForOne` default | `RestForOne`) to `BootUnit`.
  - Default `OneForOne`: `package`, `route`, `connection`, `host-mount` recover behind `SupervisedHandle` without tearing down `root-agent` or `local-entry`.
  - Opt-in `RestForOne` (e.g., configurable on `agent-runtime`): when that unit fails and restarts, downstream dependents in `BootManifest` are stopped cleanly in reverse topological order and brought back in dependency order.
- **Event-driven loop & reverse `stop()`**: `SupervisorRuntime` spawns/tracks per-unit wait tasks reading `/proc/<pid>/wait` and sends exit notifications into `run_supervisor`'s `select!`, removing the `25ms` periodic tick and `10ms` `wait_unit_ready` spin loop. `stop()` computes reverse topological order from `BootManifest.after` so dependents always terminate before their prerequisites.
- **Telemetry projection**: expose `generation`, `restarts`, `backoff_until`, and `last_exit_reason` files under `/mnt/service-manager/units/<name>/`.

### 4. Event-Driven Child & Workstream Supervision (`crates/agent-engine`)
- Replace `DelegatedChildRunSupervisor`'s `tokio::time::sleep(file_poll_interval)` loop over ~10 files with multiplexed blocking reads on `/proc/<pid>/wait` and `/agent/<pid>/events` (`requests`/`actions`/`output` readiness), bounded by child timeout cancellation.
- Set `link_parent: true` on parent-owned delegated child spawns (`ChildProcessLaunch`) while leaving decoupled workflow launches `false`.
- Replace `async_workstreams` `QUIESCE_POLL_INTERVAL` (`10ms`) spin loop with stream event notification on workstream turn/process completion.

## Risks / Rollback

- **Restart blast radius**: keeping `OneForOne` as the `BootUnit` default prevents leaf-service hiccups from killing interactive `alan` sessions; `RestForOne` activates only when explicitly opted into a `BootUnit`.
- **Stream EOF / reattach**: callers must handle already-exited `/proc/<pid>/wait` reads cleanly from offset `0`; unit tests cover both pre-exit attach and post-exit attach races.
