# Proposal

## Why

Multi-writer audits (`#1187`) show that local clients, attached `namespace.ap.sock` handles, `EmbeddedAlan` callers, and child agents currently mount `/agent` `ReadWrite` against a static `AgentFs` writability table, while `machine/tape` leases stay internal to `AgentFs` and `ProcFs` drops its state lock between IO appends and event publication. Simultaneously, shared `tokio::sync::Mutex` state across `.await` points exposes in-process file servers to `select!` waiter starvation and cross-caller panic blast radius (`#1190` Slice A).

## What Changes

- Enforce mount-time actor-scoped write projections (`AgentActorView::{Engine, Client, Child}`) on `/agent` across `LocalEntryService`, `namespace.ap.sock`, `EmbeddedAlan`, and `build_child_namespace_plan`, without adding actor identity headers to `aP` wire frames.
- Add `OpenMode::WriteExclusive` to `alan-ap` (tied strictly to fid open/clunk/disconnect lifetime) and enforce exclusive write leases at the `alan-kernel` (`Namespace`/`MountFs`) boundary across `machine/tape`, `io/output`, and `machine/ui/events`, preventing bypass via interposing `FileServer`s.
- Serialize per-PID `ProcFs` IO commits across `io/{input,output}`, `io/events`, and bridged process events, and reject `Write`/`ReadWrite` `open` on read-only `/proc` nodes (such as `io/events`) at dial time with `ErrorCode::NoAccess`.
- Add the lightweight `FileServerActor` + `spawn_file_actor` primitive (`ActorFileServer`) in `crates/ap` with non-blocking `ActorOutcome::ReadStream`, async backpressure (`send().await`), and isolated panic-to-`ErrorCode::Io` handling + `ActorExitHandle`, and migrate `RouteFs` (`crates/routefs`) as the canary server.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `plan9-kernel-substrate`: Protocol-layer `WriteExclusive` lease across interposition, `ActorFileServer` primitive, and `ProcFs` serialized IO ordering + dial-time `NoAccess`.
- `agent-file-layout-contract`: Mount-time actor-scoped `/agent` write authority (`AgentActorView`) and protocol-layer exclusive-write leases for `machine/tape`, `io/output`, and `machine/ui/events`.
- `local-entry-service`: Client-scoped `/agent` mount on `LocalEntryService::namespace_for_local_client`.
- `message-routing`: Crash-isolated `ActorFileServer` execution for `RouteFs`.

## Impact

- Closes all verified gaps in GitHub Issue `#1187` and delivers Slice A of `#1190` per `ADR-0063`.
- Preserves existing `EmbeddedAlan`, TUI, and CLI client contracts (`io/input`, `machine/ctl`, `requests/<id>/response`) while blocking unauthorized engine-surface writes at dial time.
- Unblocks future external-writer and interposing `FileServer` surfaces safely without altering `aP` wire transport framing.
