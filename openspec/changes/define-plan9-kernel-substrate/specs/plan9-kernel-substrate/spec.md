## ADDED Requirements

### Requirement: Kernel owns only namespace, process table, and synthetic devices
Alan Kernel SHALL consist of exactly the namespace engine, the file-server
contract, the process table, and the synthetic devices `/proc` and `/srv`. Alan
Kernel SHALL NOT model agents, LLM providers, tape, memory, tools, skills,
policy, or any higher-level product concept.

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

### Requirement: aP fids, clone-via-open, and a two-fold error model
aP SHALL model a fid as a handle to one interaction: `walk`/`open` allocate it
and `clunk` releases it. Each `open` SHALL yield an independent fid so concurrent
callers do not interfere. `open` MAY have allocation side effects: opening a
designated `clone` file SHALL allocate a new resource (such as a connection
directory) and return its name or handle, as an open-with-allocation convention
rather than a new operation. Errors SHALL split two ways: a dial-time failure
(no access, rate limited, not found) SHALL return an operation error code, while
a mid-interaction failure SHALL surface as a terminal error record in the
relevant stream.

#### Scenario: A clone file is opened
- **WHEN** a caller opens a `clone` file (for example under an `llmfs` connection)
- **THEN** aP allocates a new resource such as a connection directory and returns
  its name or handle
- **AND** two callers opening the same `clone` get independent resources

#### Scenario: A dial-time failure occurs
- **WHEN** an `open` is denied (no access, rate limited, or not found)
- **THEN** aP returns an operation error code
- **AND** no partial interaction stream is created

#### Scenario: A mid-interaction failure occurs
- **WHEN** an interaction fails after it has begun streaming
- **THEN** the failure appears as a terminal error record in that interaction's
  stream
- **AND** readers observe it by reading the stream, not by a side channel

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
  namespace
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
namespace, status, exit state, and its standard IO streams.

#### Scenario: A process is inspected
- **WHEN** a consumer opens `/proc/<pid>`
- **THEN** it finds files describing identity, parent, credentials, namespace,
  status, exit state, and IO streams subject to access rights
- **AND** `/proc/<pid>` is the single source of truth for that process; any
  `/agent`-style view is derived from it

### Requirement: `/srv` is the bootstrap rendezvous device
Alan Kernel SHALL provide `/srv` as a synthetic device where file servers post
mountable handles. `/srv` SHALL exist before any user-space file server so that
servers have a rendezvous point to publish to and clients have a place to mount
from.

#### Scenario: A file server publishes itself
- **WHEN** a user-space file server starts
- **THEN** it posts a mountable handle under `/srv`
- **AND** another process can mount that handle into its own namespace

#### Scenario: Boot assembles the root namespace
- **WHEN** Alan OS boots
- **THEN** the kernel comes up with only `/proc`, `/srv`, and the namespace
  engine present
- **AND** init / Service Manager mounts every other tree (such as `/agent`,
  `/bin`, `/lib`, `/man`, `/mnt`) by starting file servers and mounting their
  posted handles

### Requirement: The kernel crate is dependency-isolated
The `alan-kernel` crate SHALL depend on no agent, LLM provider, tape, memory,
sandbox, runtime, protocol, renderer, or transport implementation. Those
concerns SHALL live in user-space file-server crates and adapters above the
kernel.

#### Scenario: Kernel crate dependencies are audited
- **WHEN** `alan-kernel` dependencies are reviewed
- **THEN** they exclude `alan-runtime`, `alan-protocol`, provider clients,
  memory stores, sandbox backends, renderer libraries, and async task handles
- **AND** the agent runtime, LLM providers, memory, and tools appear to the
  kernel only as user-space file servers
