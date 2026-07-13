# plan9-kernel-substrate Specification

## Purpose
Defines Alan Kernel as the ephemeral namespace, Process-table, `/proc`, and
`/srv` substrate over aP, including file/stream semantics, Process creation,
access rights, network transparency, and dependency isolation.
## Requirements
### Requirement: Kernel owns only namespace, process table, and synthetic devices
Alan Kernel SHALL consist of exactly the namespace engine, the process table, and
the synthetic devices `/proc` and `/srv`. The aP file-server contract
(`FileServer` trait, fid, byte/offset stream types) SHALL live in the standalone
`alan-ap` crate (ADR-0025 D2); Alan Kernel SHALL depend on `alan-ap` and host it,
not own or duplicate the contract. Alan Kernel SHALL NOT model agents, LLM
providers, tape, memory, tools, skills, policy, or any higher-level product
concept.

#### Scenario: Kernel surface is reviewed
- **WHEN** the Alan Kernel surface is reviewed
- **THEN** every kernel concept is one of: namespace, mount, bind, union, path,
  file, directory, byte/offset stream, fid, file-server operation, process,
  process-table entry, `/proc`, or `/srv`
- **AND** it does not introduce Object, Buffer, View, Command, Query,
  Subscription, Task, Artifact, Evidence, Journal, ViewModel, or any
  agent-specific type

#### Scenario: A higher-level concept is proposed for the kernel
- **WHEN** a feature wants to add a non-substrate concept to the kernel
- **THEN** it is instead expressed as a user-space file server, a file-layout
  convention, or a descriptor above the kernel
- **AND** the kernel contract is not widened to host it

### Requirement: Kernel models a single process category
Alan Kernel SHALL model one process category, `Process`. It SHALL NOT define an
`Agent Process` category. Whether a process is an agent, a service, a tool, or a
root agent SHALL be observable only at the file and namespace layer, not as a
kernel type.

#### Scenario: Process table is reviewed
- **WHEN** the kernel process ontology is reviewed
- **THEN** it contains only `Process`
- **AND** "agent", "subagent", "root agent", "service", "command", "task", and
  "run" are roles or conventions, not kernel categories

#### Scenario: A process runs the agent loop
- **WHEN** a process assembles a request, reads an LLM stream, and spawns effects
- **THEN** the kernel still sees an ordinary `Process` with identity, parentage,
  credentials, descriptors, namespace, lifecycle, status, and exit state
- **AND** its agent-ness is determined by conformance to the agent file-layout
  convention, not by a kernel flag

### Requirement: The file-service protocol (aP) is wire-shaped
Alan OS SHALL define its file-service protocol — aP, the 9P analog, owned by the
`alan-ap` crate — so that every operation can be carried unchanged across a
process boundary by a dumb byte transport. Operations SHALL be expressed over
fids, paths, byte buffers, offsets, and error codes. The protocol SHALL NOT
require in-memory pointers, borrowed references, or return values that are
meaningful only within one address space. aP is Alan's own minimal protocol, not
literal 9P; a 9P gateway MAY be added later.

#### Scenario: Contract operation is defined
- **WHEN** a file-server operation such as walk, open, read, write, stat,
  create, remove, or clunk is defined
- **THEN** its inputs and outputs are paths/fids, byte buffers, offsets, and
  error codes
- **AND** none of them is a rich in-process type that cannot be serialized

#### Scenario: v1 transport is chosen
- **WHEN** the first implementation runs built-in file servers
- **THEN** they may use an in-process fast path with no serialization so that
  high-rate streams pay no protocol cost
- **AND** a later wire transport for out-of-process or networked file servers
  reuses the same contract without changing it

### Requirement: aP fids, clone-via-open, and a three-phase error model
aP SHALL model a fid as a handle to one interaction: `walk`/`open` allocate it
and `clunk` releases it. Each `open` SHALL yield an independent fid so concurrent
callers do not interfere. `open` MAY have allocation side effects: opening a
designated `clone` file SHALL allocate a new resource (such as a connection
directory) and return its name or handle, as an open-with-allocation convention
rather than a new operation. A multi-write request committed on `clunk` (the
commit-on-clunk framing used by `llmfs` `data` and `routefs` `send`) MAY be
rejected at commit. Errors SHALL therefore split by phase: a dial-time failure
(no access, rate limited, not found) SHALL return an `open` operation error code;
a commit-time failure (malformed/truncated request at `clunk`) SHALL return a
`write`/`clunk` operation error and start no interaction; and a mid-interaction
failure SHALL surface as a terminal error record in the relevant stream.

#### Scenario: A clone file is opened
- **WHEN** a caller opens a `clone` file (for example under an `llmfs` connection)
- **THEN** aP allocates a new resource such as a connection directory and returns
  its name or handle
- **AND** two callers opening the same `clone` get independent resources

#### Scenario: A dial-time failure occurs
- **WHEN** an `open` is denied (no access, rate limited, or not found)
- **THEN** aP returns an operation error code
- **AND** no partial interaction stream is created

#### Scenario: A commit-time failure occurs
- **WHEN** a multi-write request is malformed or truncated at `clunk`
- **THEN** the `write`/`clunk` returns an operation error and no interaction starts
- **AND** this is distinct from a dial-time `open` error and from a mid-interaction
  stream record

#### Scenario: A mid-interaction failure occurs
- **WHEN** an interaction fails after it has begun streaming
- **THEN** the failure appears as a terminal error record in that interaction's
  stream
- **AND** readers observe it by reading the stream, not by a side channel

### Requirement: Document write entry points commit on clunk
aP SHALL define one framing convention for any write entry point that consumes a
complete document: the document MAY span multiple `write`s and the entry point
SHALL commit only on `clunk` of the writing fid — never on a partial write — and
SHALL reject a malformed/truncated document at commit (a commit-time error, per
the three-phase model). This single convention SHALL govern every such surface —
`llmfs` `data`, `routefs` `send`, the `/proc/clone` exec spec, an agent's
`requests/<id>/response`, and an agent's `io/input` message (each user message is
one framed unit, so a turn never starts on truncated input) — which reference it
rather than redefining framing.

#### Scenario: A multi-write document is committed
- **WHEN** a caller writes a document to any such entry point across several
  writes and then clunks the fid
- **THEN** the surface acts on the complete document at clunk, never on a partial
  write
- **AND** a truncated/malformed document is rejected at commit, not acted on

### Requirement: Streams are byte/offset file kinds
Alan Kernel SHALL model streams as named files carrying bytes with offsets.
Typed records, such as LLM events, SHALL be a byte-stream record convention
(for example one JSON record per line) above the kernel, not a kernel type.
Streams SHALL support read, tail, and resume from an offset, and SHALL retain
history up to an owning-server policy so a reader that opens or reconnects after
records were produced can still read them from offset 0 (no missed or
mis-replayed records).

#### Scenario: A process emits output
- **WHEN** a process emits text, reasoning, or lifecycle records
- **THEN** the output is a named stream file readable by offset
- **AND** any record typing is a convention a consumer interprets, not a kernel
  schema

### Requirement: Observation is a blocking read with no second event system
Alan Kernel SHALL provide observation only as a read on a stream file that
blocks until new bytes are available. It SHALL NOT introduce a separate
subscription, notification, or event-bus primitive.

#### Scenario: A consumer watches a resource
- **WHEN** a consumer wants live updates from a file or directory
- **THEN** it reads an events/log stream file and blocks until new records arrive
- **AND** there is no subscription object, registry, or parallel event transport

#### Scenario: Many consumers watch the same stream
- **WHEN** multiple consumers watch one stream
- **THEN** each holds its own offset and reads independently
- **AND** the owning file server multiplexes readers without a kernel-level
  notification system

### Requirement: The per-process namespace is the sole capability boundary
Alan Kernel SHALL make the per-process namespace the only capability boundary.
A resource SHALL be reachable by a process if and only if it is present in that
process's namespace or dialable through a file server already in that namespace.
There SHALL be no global ambient addressing that bypasses the namespace.

#### Scenario: A child namespace is constructed
- **WHEN** a process spawns a child
- **THEN** the child receives a namespace constructed by the spawner
- **AND** the child may further restrict its own namespace but cannot acquire a
  channel to a file server it was not granted

#### Scenario: An opaque id is used
- **WHEN** an opaque id (fid, projection key, runtime reference) is used
- **THEN** it resolves only within a namespace
- **AND** it is never treated as a global capability that reaches a resource
  outside the namespace

#### Scenario: A resource is withheld from a process
- **WHEN** a process must be denied access to a resource such as an LLM provider
- **THEN** the resource's file server is simply not bound into that process's
  namespace, and its `/srv` handle is filtered out so it cannot be remounted
- **AND** no separate global policy check is required to enforce the denial

### Requirement: Access rights separate awareness from authority
Alan Kernel SHALL use access rights as the dimension that separates awareness
from authority, because a namespace tends to couple visibility with reachability.
A tree bound read-only SHALL grant awareness (walk, read, watch) without granting
mutation; a tree bound read-write SHALL grant authority. A process SHALL NOT
escalate a read-only mount to read-write from within its own namespace.

#### Scenario: A tree is bound for awareness only
- **WHEN** a spawner binds a tree into a child's namespace read-only
- **THEN** the child can walk, read, and watch that tree
- **AND** it cannot mutate the tree or re-bind it read-write

#### Scenario: Authority is granted explicitly
- **WHEN** a process must mutate a resource
- **THEN** that resource is bound read-write into its namespace
- **AND** broad read-only visibility never implies write authority

### Requirement: The namespace is assembled by mount, bind, and union
Alan Kernel SHALL assemble a namespace from mount, bind, and union operations
over file servers. Union directories SHALL allow several sources to contribute
entries at a single path. A child SHALL inherit a namespace from its spawner and
SHALL be able to modify only its own namespace.

#### Scenario: A standard root is assembled
- **WHEN** a directory such as `/bin` is contributed to by several file servers
- **THEN** the kernel presents a union view of their entries at that path
- **AND** the contributing servers remain independent

#### Scenario: A process modifies its namespace
- **WHEN** a process mounts, binds, or unmounts in its own namespace
- **THEN** only that process's view changes
- **AND** other processes' namespaces are unaffected

### Requirement: The kernel is ephemeral; persistence belongs to file servers
Alan Kernel SHALL keep the process table, namespaces, and fids as runtime state
that does not survive restart. Durability SHALL be a property of storage-backed
file servers, never of the kernel.

#### Scenario: The kernel restarts
- **WHEN** the kernel restarts
- **THEN** the process table, namespaces, and fids start empty
- **AND** any continuity is provided by storage-backed file servers re-presenting
  their durable trees, not by kernel persistence

#### Scenario: A durable identity is referenced
- **WHEN** something must refer to a long-lived entity across restarts
- **THEN** it refers to a path in a durable file server tree
- **AND** it does not rely on a pid, which is ephemeral

### Requirement: `/proc` renders the process table as files
Alan Kernel SHALL render the process table as files under `/proc`. Each process
SHALL appear as `/proc/<pid>` with files for identity, parentage, credentials,
namespace, status, exit state, its standard IO streams (`io/`), and a `ctl`
control file (the generic process layout every process exposes, so control writes
such as interrupt/cancel route through `/proc/<pid>/ctl`).

#### Scenario: A process is inspected
- **WHEN** a consumer opens `/proc/<pid>`
- **THEN** it finds files describing identity, parent, credentials, namespace,
  status, exit state, `io/` streams, and a `ctl` control file subject to access
  rights
- **AND** `/proc/<pid>` is the single source of truth for that process; any
  `/agent`-style view is derived from it

### Requirement: Process creation (spawn) is an aP write via clone-via-open
Alan Kernel SHALL expose process creation through aP, not a side API, so an
aP-only client (such as Alan Shell) can launch processes with no non-file
operation. Opening `/proc/clone` SHALL allocate a new process slot and return its
pid at open time (clone-via-open, like `/net`'s clone returning a connection
name). The pending slot SHALL be private to the clone fid — visible to the
spawner via the open result but NOT yet listed in the public `/proc` — until
commit. The caller SHALL then write the exec spec — executable path, arguments,
and the child's namespace/descriptors — into the new slot and `clunk` to commit;
the exec spec follows the commit-on-clunk document convention (it MAY span
multiple writes; the process starts only at clunk, never from a truncated spec).
On a successful `clunk` the process starts and `/proc/<pid>` becomes visible in
the public `/proc`; `clunk` returns success or a commit-time error and carries no
special payload. The child's namespace SHALL be
the one the spawner specifies, but spawn SHALL be capability-preserving, not
capability-amplifying:
the kernel SHALL reject any exec-spec namespace entry or descriptor the spawner
could not itself open or delegate from its own namespace and access rights. A
spawner therefore cannot bind a withheld llmfs Connection, `/srv` handle, or any
other resource it cannot reach into a child (the basis of the capability
boundary, D6). If the commit fails (malformed exec spec or capability rejection),
the kernel SHALL discard the fid-private pending slot — which was never listed in
public `/proc`, so it neither leaks nor is observed by a `/proc` watcher; the
failing `clunk` returns
the commit-time error.

#### Scenario: A client spawns a process
- **WHEN** Alan Shell launches an executable
- **THEN** opening `/proc/clone` returns the new pid (a fid-private pending slot,
  not yet in public `/proc`); it writes the exec spec into the slot and clunks to
  start the process, at which point `/proc/<pid>` becomes publicly visible
- **AND** the pid came from the clone-open result and status is read from
  `/proc/<pid>/status`; no operation outside aP open/write/clunk was needed

#### Scenario: The spawned child's namespace is constructed
- **WHEN** the exec spec includes the child's namespace/descriptors
- **THEN** the child starts with exactly that namespace (it cannot reach servers
  not granted, per D6)
- **AND** spawn is the point where the capability boundary is set

#### Scenario: A spawner tries to amplify a child's capabilities
- **WHEN** a restricted process writes an exec spec that binds a resource it
  cannot itself open (such as a withheld llmfs Connection or a `/srv` handle it
  may not mount)
- **THEN** the kernel rejects the spawn
- **AND** a child can never receive a capability the spawner did not hold or could
  not delegate

#### Scenario: A spawn fails at commit
- **WHEN** the exec spec is malformed or capability-rejected at `clunk`, after the
  fid-private pending slot was allocated at clone-open
- **THEN** the failing `clunk` returns the commit-time error and the kernel
  discards the fid-private pending slot
- **AND** because the slot was never listed in public `/proc`, it neither leaks
  nor is observed by a `/proc` watcher

### Requirement: `/srv` is the bootstrap rendezvous device, access-filtered
Alan Kernel SHALL provide `/srv` as a synthetic device where file servers post
mountable handles. `/srv` SHALL exist before any user-space file server so that
servers have a rendezvous point to publish to and clients have a place to mount
from. `/srv` SHALL NOT be an ambient backdoor: a posted handle SHALL carry access
rights, and a process SHALL see and mount only the handles permitted by its
namespace and access rights. A service withheld from a process (by not binding
its tree) SHALL NOT be remountable by that process via `/srv` — otherwise the
denial-by-absent-mount guarantee of the capability model would not hold. A
restricted child MAY be given a filtered or absent `/srv`.

#### Scenario: A file server publishes itself
- **WHEN** a user-space file server starts
- **THEN** it posts a mountable handle under `/srv` with access rights
- **AND** another process can mount that handle only if its namespace and access
  rights permit

#### Scenario: A withheld service cannot be remounted via `/srv`
- **WHEN** a sub-agent is denied model access by not binding the llmfs Connection
  tree into its namespace
- **THEN** it cannot regain that service by mounting a `/srv` handle (its `/srv`
  view is filtered to exclude handles it may not mount)
- **AND** the denial-by-absent-mount guarantee (D6) holds

#### Scenario: Boot assembles the root namespace
- **WHEN** Alan OS boots
- **THEN** the kernel comes up with only `/proc`, `/srv`, and the namespace
  engine present
- **AND** init / Service Manager mounts every other tree (such as `/agent`,
  `/bin`, `/lib`, `/man`, `/mnt`) by starting file servers and mounting their
  posted handles

### Requirement: aP supports network transparency (import/export)
The aP protocol SHALL be designed so the same operations work across a network:
a process SHALL be able to import a remote file tree into its namespace and a
server SHALL be able to export its tree to another host, with no change to the
protocol or to clients. This is the basis for distributed agents — importing a
remote tool tree or model Connection into a namespace rather than calling an RPC
mesh. The wire transport that realizes it is a later slice (ADR-0024 D5); v1 is
in-process, but the contract MUST not preclude it.

#### Scenario: A remote tree is imported
- **WHEN** the wire transport exists and a process imports a remote host's tree
- **THEN** it mounts into the local namespace and is used with the same
  walk/open/read/write/clunk operations as a local tree
- **AND** clients do not distinguish local from imported trees

#### Scenario: The contract is checked for network readiness
- **WHEN** the aP contract is reviewed
- **THEN** every operation is expressible over a byte transport (no in-process-
  only assumptions), so import/export can be added without changing it
- **AND** in-process v1 remains the fast path

### Requirement: The kernel crate is dependency-isolated

The `alan-kernel` crate SHALL depend only on `alan-ap`, the aP protocol contract. Agent execution, LLM providers, tape, memory, policy, sandboxing, renderer concerns, service implementations, and byte transports SHALL live in user-space file-server crates and adapters above Alan Kernel.

#### Scenario: Kernel crate dependencies are audited

- **WHEN** `alan-kernel` dependencies are reviewed
- **THEN** they include `alan-ap` and exclude Agent Execution Engine, AgentFS service, provider, Memory Store, sandbox backend, renderer, and transport implementation crates
- **AND** agents, providers, memory, and Tools appear to Alan Kernel only through Processes, descriptors, namespaces, mounts, and file-server trees

### Requirement: Kernel boot creates the Service Manager Process
Kernel bootstrap SHALL provide the Process and namespace primitives needed for
Alan OS Host to create Service Manager as the first system Process. Kernel MUST
remain ignorant of Boot Unit, service policy, and renderer transport details.

#### Scenario: Host starts Service Manager
- **WHEN** a fresh Kernel has no committed Processes
- **THEN** Host creates Service Manager through normal Process creation
- **AND** later services appear as ordinary Process table entries
