# Spec Delta

## MODIFIED Requirements

### Requirement: Write authority carries an actor dimension; extension is by interpose
alan9 SHALL enforce mount-time actor-scoped write authority over `/agent` via
`AgentActorView` (`Engine`, `Client`, `Child`) rather than a static global node
table or extra `aP` wire-identity fields. Extensions SHALL interpose via
mounts governed by who-may-{read, write, mount, interpose}.

#### Scenario: An extension changes agent behavior
- **WHEN** an extension must alter how an agent reads or writes a file
- **THEN** it interposes a file server between the writer and the agent files
- **AND** it does so through a mount governed by who-may-{write,mount,interpose},
  not a private API

#### Scenario: The same node is writable to one actor and not another
- **WHEN** two actors hold fids on the same node with different mounted authority
- **THEN** the write decision depends on the actor and its capabilities
- **AND** node-writability is not a single global property independent of actor

#### Scenario: Client mounts reject writes to engine-owned surfaces
- **WHEN** a caller attached via `LocalEntryService::namespace_for_local_client`, `namespace.ap.sock`, or `EmbeddedAlan` calls `open` with write intent on `io/output`, `machine/tape`, `machine/ui/*`, `actions/*`, or `requests/clone`
- **THEN** `open` fails at dial time with `ErrorCode::NoAccess` and `stat.writable` reports `false`
- **AND** writes to `io/input`, `machine/ctl`, and `requests/<id>/response` remain permitted

#### Scenario: Child agents cannot mutate parent or sibling /agent state
- **WHEN** a child Agent Process attempts a write-intent `open` under `/agent/<parent_pid>` or a sibling `/agent/<other_pid>`
- **THEN** `open` fails at dial time with `ErrorCode::NoAccess` while own `<self_pid>` engine surfaces remain governed by its child view

### Requirement: External writers require a protocol-layer tape lease
alan9 SHALL enforce `OpenMode::WriteExclusive` at the `aP`/kernel layer for
`machine/tape`, `io/output`, and `machine/ui/events` before permitting any
external-writer or interposing file server surface. Because `AgentActorView`
blocks non-Engine opens on those files, unprivileged clients cannot hold
leases across idle or yielded windows to starve the Engine.

#### Scenario: The lease is still only an internal check
- **WHEN** the `machine/tape` lease is enforced solely inside the agent file server
- **THEN** no external-writer surface (human edit-on-yield or interpose) is enabled
- **AND** promoting the lease to the aP layer is a prerequisite for any such surface

#### Scenario: An interposer is mounted with the lease at the aP layer
- **WHEN** the lease is enforced at the aP layer and an interposing server is
  mounted between a writer and the agent files
- **THEN** a write attempted while the engine holds the lease is refused regardless
  of which file server receives it
- **AND** the interposer cannot grant a write the aP layer forbids

#### Scenario: Concurrent unframed writes to io/output or ui/events are rejected
- **WHEN** the Agent Engine holds `OpenMode::WriteExclusive` on `io/output` or `machine/ui/events`
- **THEN** any second write open against that surface fails at dial time with `ErrorCode::NoAccess` regardless of interposing mounts

#### Scenario: Unprivileged client cannot starve Engine tape lease during yield
- **WHEN** an Agent Engine yields and releases its `machine/tape` fid
- **THEN** a `Client` or non-owning `Child` mount attempting `open(..., Write | WriteExclusive | ReadWrite)` on `machine/tape` fails with `ErrorCode::NoAccess`
