# Proposal

## Why

Alan's supervision and lifecycle monitoring currently rely on fixed-interval blind polling (`25ms` `SupervisorRuntime` ticks, `10ms` `wait_unit_ready` loops, `10–250ms` ~10-file polling in `DelegatedChildRunSupervisor`, and `10ms` quiescence polling in `async_workstreams`). Meanwhile, `SwitchableFileServer` exists only as a `package`-service special case, and `SupervisorRuntime::stop()` tears down Boot Units in alphabetical `BTreeMap` order rather than reverse `after` dependency order (`#1190` Slice B + Slice C polling removal).

## What Changes

- Add `/proc/<pid>/wait` (`FileKind::Stream`, backed by `alan_ap::Stream`) to `alan-kernel` so callers block without polling and receive a single race-free terminal JSON exit record.
- Add `ExecSpec.link_parent: bool` (default `false`) to `alan-kernel` so parent-linked children undergo deterministic kernel cascade cancellation on **any** parent exit (normal, cancelled, or failed).
- Generalize `SwitchableFileServer` into `SupervisedHandle` with monotonic `generation: u64` validation across all `/srv/*` service handles; upgrade `ServiceManager` to event-driven supervision, strict reverse-topological `stop()`, default `OneForOne` recovery with opt-in `RestForOne` per `BootUnit`, and `/mnt/service-manager/units/<name>/` telemetry (`generation`, `restarts`, `backoff_until`, `last_exit_reason`).
- Replace polling loops in `DelegatedChildRunSupervisor` and `async_workstreams` (`crates/agent-engine`) with event-driven waits on `/proc/<pid>/wait` and `/agent/<pid>/events`, enabling `link_parent: true` on parent-owned child launches.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `plan9-kernel-substrate`: Race-free `/proc/<pid>/wait` exit stream and declarative `ExecSpec.link_parent` cascade cancellation.
- `service-manager`: Generation-tracked `SupervisedHandle` for `/srv/*`, event-driven supervision, strict reverse-topological `stop()`, opt-in `RestForOne`, and unit telemetry projections.
- `child-run-lifecycle`: Event-driven child run and workstream supervision without blind polling loops.

## Impact

- Delivers `#1190` Slice B and the polling-removal half of Slice C per `ADR-0063` while preserving `AgentMachine` `gen_statem` refactoring as a separate future evaluation.
- Eliminates idle/waiting CPU wakeups and race windows across `service-manager` and `agent-engine` without introducing a second RPC mechanism or global `Pid` registry.
