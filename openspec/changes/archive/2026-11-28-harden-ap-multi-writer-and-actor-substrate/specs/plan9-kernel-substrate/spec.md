# Spec Delta

## ADDED Requirements

### Requirement: Protocol-layer exclusive write lease survives namespace interposition
`alan-ap` SHALL provide `OpenMode::WriteExclusive` bound to fid open/clunk and
connection lifetime, and `alan-kernel` SHALL enforce active exclusive leases
at the `Namespace`/`MountFs` boundary so interposing file servers cannot grant
conflicting writes to `machine/tape`, `io/output`, or `machine/ui/events`.

#### Scenario: An interposing file server cannot bypass an active exclusive lease
- **WHEN** the owning Engine holds an `OpenMode::WriteExclusive` fid on `machine/tape` and an interposing `FileServer` is mounted ahead of `/agent`
- **THEN** any second `Write`, `WriteExclusive`, or `ReadWrite` open targeting that path fails at dial time with `ErrorCode::NoAccess`
- **AND** once the holder clunks the fid or drops its connection, a new exclusive open succeeds

#### Scenario: Readers continue tailing during an active exclusive write lease
- **WHEN** an Engine holds `OpenMode::WriteExclusive` on `io/output` or `machine/tape`
- **THEN** concurrent `OpenMode::Read` callers succeed and tail newly appended bytes without blocking

### Requirement: In-process FileServers MAY execute through a single-owner ActorFileServer
`crates/ap` SHALL expose a `FileServerActor` abstraction and `spawn_file_actor`
adapter wrapping a bounded `mpsc` mailbox with async backpressure, non-blocking
`ReadStream` offload outside the actor loop, panic-to-`ErrorCode::Io` isolation,
and an `ActorExitHandle`.

#### Scenario: Blocked stream readers do not stall the actor mailbox
- **WHEN** a reader opens a `FileKind::Stream` on an `ActorFileServer` and blocks waiting for future bytes
- **THEN** the actor returns `ActorOutcome::ReadStream` and awaits completion outside the mailbox loop
- **AND** concurrent `walk`, `stat`, and `write` calls continue completing without starvation

#### Scenario: Actor panic is isolated to the failing server lifetime
- **WHEN** a `FileServerActor` panics inside its isolated Tokio task
- **THEN** owned actor state drops cleanly without poisoning any shared mutex
- **AND** in-flight and subsequent calls receive `ErrorCode::Io` while `ActorExitHandle` reports the failure

### Requirement: ProcFs enforces dial-time NoAccess and serialized cross-stream IO ordering
`ProcFs::open` SHALL reject `Write`, `WriteExclusive`, or `ReadWrite` opens on
read-only `/proc` nodes (including `io/events`) at dial time with
`ErrorCode::NoAccess`, and `ProcFs` SHALL serialize per-PID IO commits across
`io/{input,output}`, `io/events`, and bridged process events.

#### Scenario: Dial-time write open on io/events fails immediately
- **WHEN** a caller opens `/proc/<pid>/io/events` with `OpenMode::Write`, `WriteExclusive`, or `ReadWrite`
- **THEN** `open` returns `ErrorCode::NoAccess` before any `write` occurs

#### Scenario: Concurrent io/input clunks preserve cross-stream ordering
- **WHEN** two writers concurrently commit framed payloads to `/proc/<pid>/io/input`
- **THEN** the byte appends in `io/input`, records in `io/events`, and bridged events in `/agent/<pid>/events` appear in identical commit order
