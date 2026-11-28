# Design: Multi-Writer `/agent` Boundaries, Protocol Leases, and `ActorFileServer`

## Context

This change implements **Wave 2 / Change 1** under
[ADR-0063](../../../docs/adr/0063-actor-fileserver-multi-writer-and-event-supervision.md),
addressing all five verified multi-writer gaps from GitHub Issue `#1187` alongside
the lightweight `ActorFileServer` substrate from `#1190` (Slice A).

```text
  TUI / EmbeddedAlan / Sock Client        AgentEngine / Child Agent (pid P)
                 │                                       │
       [AgentActorView::Client]               [AgentActorView::Engine/Child(P)]
                 │  (writable: io/input,                 │ (writable on own P only;
                 │   machine/ctl, req/response)          │  WriteExclusive on tape/output/ui_events)
                 └───────────────────┬───────────────────┘
                                     ▼
                    alan-kernel (MountFs + Lease Table)
                       (interpose-proof WriteExclusive)
                                     │
                 ┌───────────────────┴───────────────────┐
                 ▼                                       ▼
         AgentRootFs / AgentFs                 ProcFs (serialized per-PID IO)
```

## Goals / Non-Goals

### Goals
- Separate client-writable `/agent` surfaces from engine-owned surfaces via mount-time projection (`AgentActorView`), rejecting unauthorized opens at dial time with `ErrorCode::NoAccess`.
- Enforce `OpenMode::WriteExclusive` at the `alan-kernel` (`MountFs`/`Namespace`) boundary so interposing `FileServer`s cannot bypass `machine/tape`, `io/output`, or `machine/ui/events` single-writer exclusivity.
- Eliminate `ProcFs` cross-stream inversion between `io/{input,output}` and `io/events`/bridged `events`, and align `ProcFs::open` with dial-time `NoAccess`.
- Provide `FileServerActor` / `spawn_file_actor` in `crates/ap` and validate it by migrating `RouteFs` (`crates/routefs`).

### Non-Goals
- No wire-frame actor identity headers in `alan-ap` (mount topology remains the capability boundary per ADR-0024 D6).
- No TTL lease timers or keepalive heartbeats (leases bind strictly to fid `open`/`clunk`/transport drop).
- No full rewrite of `MountFs` or `ProcFs` internal APIs into `FileServerActor` in this change.

## Design Decisions

### 1. Mount-time `AgentActorView` (`crates/agentfs`, `crates/service-manager`, `crates/agent-engine`)
Introduce `AgentActorView`:
- `Client`: grants write intent ONLY on `io/input`, `machine/ctl`, and `requests/<id>/response`. All engine-owned surfaces (`io/output`, `machine/tape`, `machine/ui/*`, `machine/ui/events`, `actions/*`, `requests/clone`, `requests/<id>/{kind,prompt,options,ctl}`) reject `Write | WriteExclusive | ReadWrite` opens with `ErrorCode::NoAccess` and mask `stat.writable = false`.
- `Engine { self_pid }`: grants engine writes only under `<self_pid>` (and `root` when resolved to `<self_pid>`); client-surface writes remain permitted for child orchestration under `<child_pid>` if mounted.
- `Child { self_pid }`: mirrors `Engine { self_pid }` for own `<self_pid>` and scopes parent/sibling `/agent/<other_pid>` mounts to read-only (or unmounted where child does not supervise siblings).

Wiring points:
- `LocalEntryService::namespace_for_local_client` wraps `/agent` with `AgentActorView::Client`.
- Local `namespace.ap.sock` and `EmbeddedAlan` client handles attach through `AgentActorView::Client`.
- `build_child_namespace_plan` (`crates/agent-engine/src/runtime/child_agents.rs`) projects child `/agent` mounts through `AgentActorView::Child { self_pid }`.

### 2. Protocol-layer `OpenMode::WriteExclusive` & Kernel Lease Table (`crates/ap`, `crates/kernel`)
- Extend `alan_ap::OpenMode` with `WriteExclusive` (serde `"write_exclusive"`), treated as write intent by `Access::allows` and `MountFs::is_mutating`.
- Add a shared per-namespace exclusive-lease table in `alan-kernel` (`MountFs`), keyed by canonical backing target path (relative to the underlying mount/tree identity rather than fid) so interposing servers stacked via `Namespace::mount` share the exact lock domain.
- Opening with `WriteExclusive` (or attempting a non-exclusive `Write`/`ReadWrite` open while an exclusive lease is active on an exclusively-managed path) fails immediately with `ErrorCode::NoAccess`. Releasing happens deterministically on `clunk` or `MountFs` fid cleanup/drop.
- Engine writers (`NamespaceTapeWriter`, assistant `io/output` streaming, `machine/ui/events` JSONL emitter) open with `OpenMode::WriteExclusive`; `AgentFs` rejects non-exclusive writes on those three nodes even without an interposer.
- **Anti-DoS guarantee**: because `Client` and `Child` views reject any write-intent open on `machine/tape`, `io/output`, and `machine/ui/events` for non-owning pids, unprivileged callers can no longer hold open fids across idle/yield windows to starve the engine.

### 3. `ProcFs` Per-PID IO Serialization & Dial-Time `NoAccess` (`crates/kernel`)
- Replace split lock-drop-append ordering in `ProcFs::write(Output)` and `ProcFs::clunk(Input)` with an ordered per-PID IO commit queue/serializer (or async serialization gate) that executes `stream.append`, `append_io_event`, and `publish_process_event` in deterministic FIFO commit order without holding a global cross-PID lock across unrelated readers.
- Extend `ProcFs::open` to check node writability for ALL nodes (not only `Clone`), failing `Write`/`WriteExclusive`/`ReadWrite` opens on `io/events`, `status`, `exit`, `parent`, `credentials`, `namespace`, and `descriptors` at dial time with `ErrorCode::NoAccess`.

### 4. `ActorFileServer` (`crates/ap`) & `RouteFs` Canary (`crates/routefs`)
- Define `FileServerActor`: synchronous `&mut self` handlers for `walk`, `open`, `write`, `stat`, `create`, `remove`, `clunk`, and non-streaming `read`; streaming `read` returns `ActorOutcome::ReadStream { stream, offset, count }` (or `Ready(Vec<u8>)`).
- `spawn_file_actor<A: FileServerActor + Send + 'static>(actor: A, mailbox_cap: usize) -> (Arc<dyn FileServer>, ActorExitHandle)`:
  - Uses `tokio::sync::mpsc::channel(mailbox_cap)` with async backpressure (`send().await`).
  - Runs the actor inside a dedicated `tokio::spawn` loop so `panic = "unwind"` destroys only the actor's owned state without poisoning any shared mutex; closed response/mailbox channels fail in-flight and future `FileServer` calls with `ErrorCode::Io` and resolve `ActorExitHandle` (`Exited` vs `Panicked(reason)`).
  - Proxy awaits `ActorOutcome::ReadStream` outside the actor loop so slow stream readers never block other callers or `stat`/`write` operations.
- Migrate `RouteFs` (`crates/routefs`) onto `FileServerActor`, removing `tokio::sync::Mutex` while keeping all routing rules, `DEAD_LETTER_PORT`, `MAX_HANDOFF_HOPS`, and `RouteHandoffMessage` behavior identical.

## Risks / Rollback

- **Wire compatibility**: adding `OpenMode::WriteExclusive` extends an internal crate enum (`alan-ap`) serialized via `serde`; all shipped wire peers build from workspace `alan-ap`. External plugins using `Read | Write | ReadWrite` remain untouched unless opening engine-exclusive agent surfaces.
- **Canary isolation**: keeping `RouteFs` as the sole Change-1 actor migration allows immediate verification of `select!`-safety and panic behavior without destabilizing `MountFs` or `AgentFs` concurrently.
