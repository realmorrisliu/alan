# Actor-isolated FileServers, multi-writer boundaries, and event-driven process supervision

Status: implemented 2026-11-28 (PR #1197 and PR #1200 merged on `main`; canonical specs synced and changes archived).
Refines [ADR-0024](0024-plan9-kernel-substrate.md),
[ADR-0026](0026-wire-transport-replaces-in-process-shortcuts.md),
[ADR-0043](0043-boot-unit-and-restart-policy.md), and
[ADR-0062](0062-project-workstreams-ledger-and-stateful-cron-streams.md), while
preserving [ADR-0055](0055-agent-machine-composes-typed-capabilities.md) and
[ADR-0061](0061-terminal-native-os-boundary-quartermaster-and-first-party-providers.md).
Records the shipped architecture for GitHub Issues `#1187` and `#1190`, implemented by
PR `#1197` (`harden-ap-multi-writer-and-actor-substrate`) and PR `#1200`
(`add-event-driven-process-supervision`), synced into canonical specs and archived on
`2026-11-28`.

## Context

Alan (`alan9`) borrows its spatial capability and naming architecture from Plan 9:
per-process namespaces (`Namespace`/`MountFs`), a wire-shaped file protocol (`aP`),
`/proc`, `/srv`, and union/interpose mounts ([ADR-0024](0024-plan9-kernel-substrate.md)).
However, recent multi-writer audits (`#1187`) and concurrency/lifecycle reviews
(`#1190`) exposed complementary temporal, ownership, and lifecycle gaps:

1. **Coarse `/agent` mounts**: `LocalEntryService::namespace_for_local_client`,
   `namespace.ap.sock` attachment, `EmbeddedAlan`, and child agents inherit
   `ReadWrite` `/agent` mounts while `AgentFs` writability is a static per-node
   table, exposing engine-owned files (`io/output`, `machine/tape`, `machine/ui/*`,
   `actions/*`, `requests/clone`) to unintended writers.
2. **In-server-only `machine/tape` lease**: `AgentFs` checks `state.tape_writer`
   internally, which an interposing `FileServer` mounted ahead of `/agent` can
   bypass and which leaves idle/yielded windows exposed to conflicting opens.
3. **Cross-stream race and mutex hazards**: `ProcFs` drops `state.lock()` between
   stream appends and event publication, inverting `io/{input,output}` vs
   `io/events`/bridged `events`, and skips dial-time `NoAccess` on read-only
   nodes such as `io/events`. Meanwhile, shared `tokio::sync::Mutex<State>` across
   `.await` boundaries risks `select!` waiter starvation (`lock_discipline_tests.rs`)
   and cross-caller blast radius on panic.
4. **Polling-based lifecycle observation and flat supervision**: `ServiceManager`
   polls `ProcFs` every `25ms` and unit readiness every `10ms`,
   `DelegatedChildRunSupervisor` polls ~10 files every `10–250ms`, and
   `async_workstreams` polls quiescence every `10ms`. Meanwhile `SwitchableFileServer`
   is a `package`-only special case, and `stop()` iterates units in `BTreeMap`
   name order instead of reverse dependency order.

## Decisions

### 1. Mount-time `AgentActorView` without wire-identity bloat

Write authority over `/agent` is enforced at mount time by scoped projections
wrapping `AgentRootFs`/`AgentFs` (`AgentActorView::{Engine { self_pid }, Client,
Child { self_pid }}`) rather than adding actor identity fields to `aP` wire frames:

- **`Client` view** (`local-entry` local clients, `namespace.ap.sock`, `EmbeddedAlan`):
  write-open is allowed only on `io/input`, `machine/ctl`, and
  `requests/<id>/response`. Engine-owned surfaces fail at `open` with
  `ErrorCode::NoAccess` and report `stat.writable == false`.
- **`Engine` / `Child` view**: engine-owned write surfaces are restricted to the
  actor's own `<self_pid>`; child agents cannot write parent or sibling
  `/agent/<other_pid>` trees.

This preserves [ADR-0024](0024-plan9-kernel-substrate.md) D6 ("capability = mount")
without breaking wire transparency.

### 2. Protocol-layer exclusive-write lease (`OpenMode::WriteExclusive`)

`alan-ap::OpenMode` gains `WriteExclusive`, bound strictly to fid `open`/`clunk`
and transport drop lifetime (no TTL heartbeats). `alan-kernel` maintains a shared
per-namespace lease table at the `MountFs`/`Namespace` boundary so any
interposing `FileServer` mounted ahead of `/agent` cannot grant a conflicting
write open while an exclusive lease is active. Single-producer engine surfaces
(`machine/tape`, `io/output`, `machine/ui/events`) require `WriteExclusive`.

### 3. Single-owner `ActorFileServer` with async backpressure (`crates/ap`)

In-process file servers may adopt a zero-framework `FileServerActor` trait (pure
synchronous `&mut self` request handling with `ActorOutcome::ReadStream` offloading
blocking `FileKind::Stream` waits outside the actor loop) wrapped by
`spawn_file_actor(actor, mailbox_cap) -> (Arc<dyn FileServer>, ActorExitHandle)`:

- Bounded `mpsc::channel(cap)` enforces async backpressure (`send().await`)
  rather than speculative `RateLimited` rejections.
- Panics inside the actor loop (`panic = "unwind"`) are caught by its dedicated
  Tokio task, drop owned state without lock poisoning, fail in-flight/new calls
  with `ErrorCode::Io`, and notify `ActorExitHandle`.
- `RouteFs` serves as the canary migration in Change 1; `ProcFs` retains its
  direct kernel/service-manager API while serializing per-PID IO commits and
  rejecting read-only `Write`/`ReadWrite` opens with `ErrorCode::NoAccess`.

### 4. Two-layer fault-isolation pyramid

- **Inner layer (μs recovery)**: pure-memory `ActorFileServer` instances recover
  in-process with state held cleanly or reconstructed from durable System Store /
  rollout state.
- **Outer layer (ms hard isolation)**: untrusted tools, Quartermaster `q`
  packages, MCP servers, and external `fileserver` extensions remain confined in
  OS-sandboxed child processes over wire `aP` ([ADR-0061](0061-terminal-native-os-boundary-quartermaster-and-first-party-providers.md)).

Both layers export plain mountable file trees; actor isolation complements
confinement and never substitutes for OS sandboxing or Host Mount grants.

### 5. Event-driven process supervision (`/proc/<pid>/wait`, `link_parent`, `SupervisedHandle`)

- **`/proc/<pid>/wait`**: read-only `FileKind::Stream` backed by `alan_ap::Stream`;
  blocks while the process runs and immediately yields a single terminal JSON line
  (`{"pid":...,"status":"exited","exit_code":...,"reason":"..."}`) on or after exit.
- **`ExecSpec.link_parent: bool` (default `false`)**: when enabled, parent exit
  of **any** kind (normal, cancelled, or failed) deterministically cascades
  cancellation to linked child processes.
- **`SupervisedHandle` and `SupervisionTree`**: generalize `SwitchableFileServer`
  across all `/srv/*` handles with monotonic `generation: u64` validation so
  stale pre-restart fids fail cleanly with `ErrorCode::Io`; replace `25ms`/`10ms`
  polling loops with `/proc/<pid>/wait` and readiness events; enforce strict
  reverse-topological `stop()`; default runtime restarts to `OneForOne` behind
  `SupervisedHandle` (avoiding live-session disruption when leaf services such as
  `package` or `route` recover), with explicit opt-in `RestForOne` per `BootUnit`
  and unit supervision telemetry under `/mnt/service-manager/units/<name>/`.
- Replace `DelegatedChildRunSupervisor` and `async_workstreams` polling loops
  with blocking waits on `/proc/<pid>/wait` and `/agent/<pid>/events`.

### 6. Delivery split & explicit Non-Goals

Wave 2 splits cleanly into two independent OpenSpec changes:

1. `harden-ap-multi-writer-and-actor-substrate` (`#1187` Gaps 1–5 + `#1190` Slice A)
2. `add-event-driven-process-supervision` (`#1190` Slice B + Slice C polling removal)

**Explicit Non-Goals**:
- No global `Pid ! Msg` or ambient process registry ([ADR-0024](0024-plan9-kernel-substrate.md) D6).
- No Erlang hot code reload or heavyweight actor framework (`Actix`, `Ractor`).
- No `AgentMachine` `gen_statem` rewrite in Wave 2 (deferred for separate evaluation).

## Owning changes

- [Multi-writer & Actor substrate archived OpenSpec proposal](../../openspec/changes/archive/2026-11-28-harden-ap-multi-writer-and-actor-substrate/proposal.md)
- [Event-driven process supervision archived OpenSpec proposal](../../openspec/changes/archive/2026-11-28-add-event-driven-process-supervision/proposal.md)
- [Personal daily-use roadmap](../../openspec/changes/qualify-personal-daily-development/roadmap.md)
