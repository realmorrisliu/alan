# Tasks

## 1. `alan-ap`: Exclusive Open Mode & `ActorFileServer` (`crates/ap`)
- [x] 1.1 Add `OpenMode::WriteExclusive` to `crates/ap/src/types.rs` (including serde round-trip coverage in `crates/ap/tests/wire.rs`).
- [x] 1.2 Implement `FileServerActor`, `ActorOutcome`, `ActorExitHandle`, and `spawn_file_actor(actor, mailbox_cap)` in `crates/ap` with async backpressure (`send().await`), non-blocking `ReadStream` offload outside the actor loop, and panic-to-`ErrorCode::Io` isolation.
- [x] 1.3 Add `crates/ap` unit tests verifying zero mailbox starvation under blocked `Stream` readers, bounded backpressure, and clean `ErrorCode::Io` + `ActorExitHandle` notification on actor panic.

## 2. `alan-kernel`: Protocol Lease Enforcement & `ProcFs` Ordering (`crates/kernel`)
- [x] 2.1 Update `Access::allows` and `MountFs::is_mutating` for `OpenMode::WriteExclusive`, and enforce a shared kernel exclusive-write lease table in `MountFs`/`Namespace` keyed by canonical target path so interposing `FileServer`s cannot bypass an active exclusive lease.
- [x] 2.2 Serialize per-PID `ProcFs` IO commits across `io/{input,output}`, `io/events`, and bridged process events (`publish_process_event`) so concurrent writers preserve exact cross-stream order.
- [x] 2.3 Enforce dial-time `ErrorCode::NoAccess` in `ProcFs::open` for `Write | WriteExclusive | ReadWrite` opens on read-only nodes (including `io/events`).
- [x] 2.4 Add `crates/kernel` unit tests covering interposer lease enforcement, lease release on `clunk` and transport drop, concurrent `ProcFs` cross-stream ordering, and dial-time `NoAccess` on `/proc/<pid>/io/events`.

## 3. Actor-Scoped `/agent` Views & Engine Lease Adoption (`crates/agentfs`, `crates/service-manager`, `crates/agent-engine`)
- [x] 3.1 Implement `AgentActorView` (`Engine { self_pid }`, `Client`, `Child { self_pid }`) enforcing actor-scoped writability at `open` (`NoAccess`) and `stat` (`stat.writable == false`).
- [x] 3.2 Update `LocalEntryService::namespace_for_local_client`, `namespace.ap.sock`, `EmbeddedAlan`, and `build_child_namespace_plan` (`crates/agent-engine/src/runtime/child_agents.rs`) to mount `/agent` with the appropriate `AgentActorView` (`Client` or `Child`).
- [x] 3.3 Require `OpenMode::WriteExclusive` for `machine/tape`, `io/output`, and `machine/ui/events` writers across `agent-engine` and `agentfs`, preventing interleaved writes or JSONL corruption.

## 4. `RouteFs` Canary Actor Migration (`crates/routefs`)
- [x] 4.1 Refactor `RouteFs` (`crates/routefs/src/lib.rs`) to implement `FileServerActor` via `spawn_file_actor`, removing internal `tokio::sync::Mutex` locks while offloading port/log `Stream` reads cleanly.
- [x] 4.2 Add crash-isolation and concurrent-reader/writer tests in `crates/routefs` and confirm existing `workstream_pty_test` / M8 handoff suites remain green.

## 5. End-to-End Verification & Quality Gate
- [x] 5.1 Add end-to-end regression tests across `AgentRootFs` + `ProcFs` + `MountFs` for concurrent `io/input` clunks + cross-stream ordering, client/child write denial on engine-owned surfaces, and interposing-server lease rejection.
- [x] 5.2 Run `just quality` and strict OpenSpec validation (`openspec validate --strict harden-ap-multi-writer-and-actor-substrate`).
