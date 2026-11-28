# Tasks

## 1. Kernel Wait Stream & Declarative Parent Link (`crates/kernel`)
- [x] 1.1 Add `/proc/<pid>/wait` (and `/proc/self/wait`) as a read-only `FileKind::Stream` backed by `alan_ap::Stream`, publishing single-line terminal JSON (`pid`, `status`, `exit_code`, `reason`) race-free for both live and already-exited processes.
- [x] 1.2 Add `#[serde(default)] pub link_parent: bool` to `ExecSpec` and wire deterministic cascade cancellation in `ProcessTable`/`ProcFs` on any parent exit (normal, failed, or cancelled).
- [x] 1.3 Add `crates/kernel` unit tests covering pre-exit blocking read, post-exit immediate read, write-open `ErrorCode::NoAccess`, and multi-level `link_parent` cascade cancellation.

## 2. `SupervisedHandle`, Reverse Shutdown & Event-Driven `ServiceManager` (`crates/service-manager`)
- [x] 2.1 Generalize `SwitchableFileServer` into `SupervisedHandle` with monotonic `generation: u64` tracking across all `/srv/*` service handles so stale pre-restart fids fail cleanly with `ErrorCode::Io`.
- [x] 2.2 Add `SupervisionStrategy::{OneForOne, RestForOne}` (default `OneForOne`) to `BootUnit` TOML schema and validation (`crates/service-manager/src/boot_unit.rs`).
- [x] 2.3 Refactor `SupervisorRuntime` (`crates/service-manager/src/runtime/supervisor.rs`) to drive unit supervision and readiness from `/proc/<pid>/wait` instead of `25ms` ticks and `10ms` `wait_unit_ready` polling; implement strict reverse-topological `stop()` and opt-in `RestForOne` cascade restarts.
- [x] 2.4 Expose `generation`, `restarts`, `backoff_until`, and `last_exit_reason` under `/mnt/service-manager/units/<name>/`.
- [x] 2.5 Add `crates/service-manager` tests for reverse-order shutdown, `OneForOne` leaf self-healing without dropping `root-agent`, opt-in `RestForOne` cascade, and stale-generation fid rejection.

## 3. Event-Driven Child & Workstream Supervision (`crates/agent-engine`)
- [ ] 3.1 Replace `DelegatedChildRunSupervisor`'s `10–250ms` polling loop (`crates/agent-engine/src/runtime/delegated_child_run/supervisor.rs`) with blocking waits over `/proc/<pid>/wait` and `/agent/<pid>/events` (preserving pause/timeout/cancellation semantics).
- [ ] 3.2 Set `link_parent: true` on parent-owned child agent launches while keeping independent/detached launches `false`.
- [ ] 3.3 Replace `async_workstreams` `10ms` quiescence polling (`crates/agent-engine/src/runtime/async_workstreams.rs`) with stream-driven completion signaling.

## 4. Verification & Quality Gate
- [ ] 4.1 Run end-to-end tests (`workstream_pty_test`, lifecycle/chaos suites, and PTY E2E tests touching `/mnt/service-manager/units/*`).
- [ ] 4.2 Run `just quality` and strict OpenSpec validation (`openspec validate --strict add-event-driven-process-supervision`).
