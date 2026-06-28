## ADDED Requirements

### Requirement: An agent is a conforming process, not a kernel type
Alan OS SHALL treat an agent as a `Process` whose `/agent/<pid>` overlay conforms
to the agent file-layout convention. Agent-ness SHALL be discoverable by
inspecting the `/agent` overlay, not by any kernel flag or kernel category, while
`/proc/<pid>` stays generic.

#### Scenario: A process is tested for agent-ness
- **WHEN** a tool needs to know whether a process is an agent
- **THEN** it inspects `/agent/<pid>` for the agent overlay (such as `machine/`
  and `requests/`), while `/proc/<pid>` exposes only the generic layout
- **AND** no kernel agent type is consulted, and no agent files are sought in
  `/proc`

#### Scenario: A third-party runtime exposes agents
- **WHEN** a third-party runtime file server exports process directories that
  conform to this contract
- **THEN** existing shells and tools operate its agents unchanged
- **AND** the runtime needs no kernel changes to be operable

### Requirement: Every process exposes the generic process layout
Alan OS SHALL define a generic process layout that every process exposes: the
full `/proc/<pid>` substrate layout (identity, parentage, credentials, namespace,
status, and exit state per `define-plan9-kernel-substrate`) plus the common
IO/control subset — an `io/` directory with `input`, `output`, and `events`
streams, a `status` file, and a `ctl` control file. The agent overlay is unioned
on top of this full layout.

#### Scenario: A non-agent process is operated
- **WHEN** a consumer opens a non-agent process directory
- **THEN** it finds the substrate metadata (identity, parentage, credentials,
  namespace, exit state) plus the `io/`, `status`, and `ctl` IO/control subset
- **AND** `cat io/output`, reading `status`, and writing `ctl` work the same as
  for an agent; `children/` can be derived from `/proc` parentage

#### Scenario: Output is complete and tail-reachable
- **WHEN** a consumer reads `io/output`
- **THEN** the stream is append-only with monotonic offsets and exposes its full
  produced content (subject to retention), so a reader can always resume to the
  newest bytes
- **AND** clipping of the newest output is a renderer concern, never a gap in the
  stream's data

### Requirement: An agent overlays agent files on the generic process layout
Alan OS SHALL define the agent layout as the generic process layout plus an agent
overlay. The generic layout — the full `/proc/<pid>` substrate layout (identity,
parentage, credentials, namespace, exit state) plus the `io/`/`status`/`ctl`
IO/control subset — is kernel-rendered. The agent-specific superset — `requests/`,
`actions/`,
`machine/`, `context/`, `children/`, and a top-level `events` stream — SHALL be
served by the agent runtime file server and presented at `/agent/<pid>` as an
overlay (a union of the kernel's `/proc/<pid>` with the agent surfaces). The
kernel SHALL NOT render agent-specific files in `/proc`; agent surfaces come from
the agent runtime, unioned under `/agent/<pid>`. The `machine/` directory SHALL
contain the tape, machine state, and checkpoints. `children/` SHALL be a view of
the agent's child processes derived from `/proc` parentage (not a second source
of truth). The top-level `events` stream SHALL be an aggregate, watchable by
blocking read, over the agent's lifecycle, IO, request, action, and child
changes, so a watcher can follow the whole agent from one stream.

#### Scenario: An agent is inspected
- **WHEN** a consumer opens `/agent/<pid>`
- **THEN** it finds the generic `io/`, `status`, `ctl` (from `/proc/<pid>`) plus
  the agent overlay `requests/`, `actions/`, `machine/`, `context/`, `children/`,
  and `events` (from the agent runtime)
- **AND** `/proc/<pid>` alone exposes only the generic layout; the kernel renders
  no agent-specific files

#### Scenario: Generic tools still work on the bare process
- **WHEN** a consumer reads `/proc/<pid>/io/output`
- **THEN** it works whether or not the process is an agent
- **AND** agent surfaces are reached through `/agent/<pid>`, not synthesized into
  `/proc`

#### Scenario: A watcher follows the whole agent
- **WHEN** a consumer wants to follow everything an agent does
- **THEN** it tails the top-level `events` stream and learns of new output,
  requests, actions, status changes, and spawned children
- **AND** the per-container streams (`io/events`, `requests/` events,
  `actions/` events) remain available for finer-grained watching

#### Scenario: Child agents are listed
- **WHEN** a consumer lists `children/` on an agent
- **THEN** it sees the agent's child processes resolved from `/proc` parentage
- **AND** `children/` does not duplicate process state owned by `/proc`

#### Scenario: A non-agent is read for agent files
- **WHEN** a consumer lists `requests/` on a non-agent process
- **THEN** the directory is absent or empty
- **AND** the same tool code degrades gracefully across process kinds

### Requirement: Control is expressed by writing to a `ctl`, split by ownership
Alan OS SHALL express control as text commands written to a `ctl` file, split by
who owns the semantics. Generic process control (interrupt, cancel, signal) SHALL
be the kernel-owned `/proc/<pid>/ctl`. Agent-runtime control whose meaning is the
Agent Execution Engine's (such as `compact` and `rollback`, which operate on the
tape/checkpoints) SHALL be written to an agent-runtime-owned `machine/ctl` in the
`/agent/<pid>` overlay — NOT the kernel `/proc/<pid>/ctl`, so the kernel never
interprets runtime semantics. New control actions SHALL be added as new `ctl`
commands on the owning surface, not as new files or APIs.

#### Scenario: An agent is interrupted (generic control)
- **WHEN** an operator interrupts an agent
- **THEN** the operator writes an interrupt command to the kernel
  `/proc/<pid>/ctl`
- **AND** no dedicated interrupt file or out-of-band API is required

#### Scenario: Tape is compacted (runtime control)
- **WHEN** an operator compacts or rolls back an agent's tape
- **THEN** the command is written to the agent-runtime-owned `machine/ctl` in the
  `/agent/<pid>` overlay, not to the kernel `/proc/<pid>/ctl`
- **AND** the kernel does not interpret tape/checkpoint semantics

#### Scenario: A request is answered
- **WHEN** a user answers an agent request
- **THEN** the answer is written to the request's response file under
  `requests/<id>/` and committed on clunk (the aP commit-on-clunk document
  convention), so a large structured-input/credential answer is delivered whole,
  never truncated
- **AND** the agent runtime delivers it on commit without a private resume API,
  and never resumes on a partial write

### Requirement: `/agent` is an overlay over `/proc`
Alan OS SHALL present `/agent` as an overlay over `/proc`: for each
agent-conforming process it unions the kernel's `/proc/<pid>` generic layout with
the agent runtime's agent surfaces, with stable aliases such as `/agent/root`
resolving to whichever pid currently embodies the root agent's home. `/proc`
remains the source of truth for generic process state; the agent runtime is the
source of truth for the agent surfaces. `/agent` SHALL NOT be a second,
independent process table.

#### Scenario: The root agent is addressed
- **WHEN** a consumer opens `/agent/root`
- **THEN** it resolves to the current root agent process in `/proc`
- **AND** the same process is visible at `/proc/<pid>`

#### Scenario: The root agent restarts
- **WHEN** the root agent process restarts with a new pid
- **THEN** `/agent/root` resolves to the new pid
- **AND** durable identity remains the root agent's home path, not the pid

### Requirement: The LLM is a typed stream the process consumes
Alan OS SHALL model the LLM as a typed stream a process reads. The LLM SHALL have
no inherent authority; tool-call intents in the stream SHALL become real effects
only when the consuming process spawns them under its own namespace and policy.

#### Scenario: A tool-call intent appears in the stream
- **WHEN** the LLM stream contains an intent to run a tool
- **THEN** the consuming process decides whether to spawn the corresponding
  `/bin` executable under its namespace
- **AND** the effect is governed by the process, not granted by the provider

#### Scenario: A sub-agent is denied model access
- **WHEN** a parent spawns a child without binding the provider file server
- **THEN** the child cannot open an LLM stream
- **AND** the denial needs no separate policy check beyond the absent mount

### Requirement: The request is assembled from the namespace
Alan OS SHALL assemble the logical model request as a view over namespace files —
`machine/tape`, `context/`, and the visible `/bin` Tools. Tape compaction SHALL be
a view over `machine/tape` (tape is truth; the context-window view is what is
sent), not a hidden runtime step. An agent's model-callable tools SHALL be exactly
the visible `/bin` entries that are Tools (those carrying a tool manifest under
`/lib/exec/<tool>`); Agent Executables in the `/bin` union (e.g. `review`,
`delegate`) are spawn targets, not model-callable Tools, and SHALL NOT appear in
the request's tool list.

#### Scenario: Context is changed
- **WHEN** a file is bound into or removed from an agent's `context/` or `/bin`
- **THEN** the next assembled request reflects the change
- **AND** no separate prompt-assembly configuration is edited

#### Scenario: The provider is changed
- **WHEN** the provider mount is rebound to a different LLM file server
- **THEN** the agent's request assembly is unchanged
- **AND** only the provider-local wire translation differs

#### Scenario: The model's tool list is computed
- **WHEN** the request's available tools are computed
- **THEN** they are the visible `/bin` Tool entries (those with a manifest under
  `/lib/exec/<tool>`), excluding Agent Executables, which are spawn targets not
  model-callable tools
- **AND** there is no separate tool registry granting tools outside the namespace

### Requirement: Requests and actions are files with events
Alan OS SHALL represent agent yield, confirmation, approval, credential,
selection, and structured-input requests as file trees under `requests/<id>/`,
and agent-proposed or running effects under `actions/<id>/`. Each dynamic
container SHALL expose an events stream that consumers watch by blocking read.

#### Scenario: A new request appears
- **WHEN** an agent raises a request
- **THEN** `requests/<id>/` is created with kind, prompt, options, status, and a
  response file
- **AND** a watcher learns of it by reading the `requests/` events stream, not by
  polling

#### Scenario: An action runs a tool
- **WHEN** an agent spawns a `/bin` tool as an effect
- **THEN** the tool process appears in `/proc`, and `actions/<id>/` records its
  status, process reference, output, result, and approval state
- **AND** the action references the tool process rather than duplicating it

### Requirement: Request and action status integrity
Alan OS SHALL keep request and action status truthful. A response written to a
request whose status is already terminal (answered, closed, or cancelled) SHALL
be rejected. An action's recorded terminal status SHALL accurately reflect the
underlying effect: a failed effect SHALL be recorded as failed, not partial, and
an incomplete result SHALL be recorded as partial, not satisfied.

#### Scenario: A response targets a closed request
- **WHEN** a client writes `requests/<id>/response` for a request that is already
  answered, closed, or cancelled
- **THEN** the write is rejected with an error and does not re-open or re-run the
  request
- **AND** the originating process is not resumed for an already-settled request
  (the invariant the legacy "resume non-yielded run" bug violated)

#### Scenario: A failed effect is recorded
- **WHEN** an effect fails while also producing incomplete or unsupported result
  fields
- **THEN** `actions/<id>/status` records the effect as failed
- **AND** it is not downgraded to partial because of the result fields

#### Scenario: An incomplete result is recorded
- **WHEN** an effect satisfies only some requested result fields
- **THEN** the action records a partial result rather than satisfied
- **AND** downstream completion logic cannot treat it as fully satisfied

### Requirement: Root Agent has broad awareness but narrow authority
Alan OS SHALL keep awareness and authority separate for the Root Agent. Because a
namespace tends to couple visibility with reachability, the separating dimension
SHALL be access rights: awareness is granted by binding trees read-only, and
authority is granted by binding trees read-write. The Root Agent's default
namespace SHALL be broad read-only (system indexes, notifications, process and
service status, public app indexes, continuity memory) and narrow read-write, and
it SHALL gain authority over private content only through explicitly granted
read-write mounts.

#### Scenario: Root Agent observes useful work
- **WHEN** the Root Agent sees, through its broad read-only mounts, an event that
  suggests useful work
- **THEN** it may raise a request, propose an action, or spawn a child with an
  explicitly constructed namespace
- **AND** it cannot mutate private content it can only see read-only

#### Scenario: Root Agent is granted authority
- **WHEN** the Root Agent must act on a resource
- **THEN** authority is granted by binding that resource read-write into its (or a
  child's) namespace
- **AND** broad read-only awareness never implies write authority

### Requirement: Durable agent identity is a home tree
Alan OS SHALL make an agent's durable identity a home file tree (config, memory,
`machine/` state) owned by a storage-backed file server and bound into the
agent's namespace. Running the agent SHALL be an ephemeral process bound to that
home; restart continuity SHALL be a new process re-binding the same home.
Whether an agent is durable or ephemeral SHALL be decided by where its home is
mounted.

#### Scenario: An agent resumes after restart
- **WHEN** a durable agent's process restarts
- **THEN** a new process binds the same home tree and resumes its `machine/tape`
- **AND** continuity comes from the durable home, not from kernel persistence

#### Scenario: An ephemeral agent is created
- **WHEN** an agent is given a non-durable (for example tmpfs) home
- **THEN** its tape and state do not survive restart
- **AND** durability differs only by the home's mount, not by an agent type

### Requirement: Metering lives in the provider file server
Alan OS SHALL place model cost, metering, and rate-limiting in the LLM provider
file server, which an agent is subject to only when that server is bound into its
namespace. There SHALL be no global model-quota policy engine outside the
namespace.

#### Scenario: An agent's model spend is capped
- **WHEN** an agent must be limited in model usage
- **THEN** the binding provider file server enforces the limit on the streams it
  serves
- **AND** the limit is reached through the namespace mount, not an ambient global
  quota service

### Requirement: A `ctl` is scoped to one lifecycle-bearing object
Alan OS SHALL place a `ctl` in the directory that represents a single
lifecycle-bearing object, alongside that object's data/status — the Plan 9 idiom
(`/proc/<pid>/ctl`, `/net/tcp/<n>/ctl`). There SHALL NOT be one global
`/agent/<pid>/ctl` that re-encodes object addressing as a verb argument, nor a
`ctl` on a leaf that is pure state (e.g. `machine/tape/ctl`). The control surfaces
this yields are `/proc/<pid>/ctl` (generic process) and `machine/ctl`
(agent-runtime tape/checkpoint); leaf state files (`machine/status`,
`requests/<id>/status`) are read-only and carry no control.

#### Scenario: A controllable object gains a new verb
- **WHEN** a new control action is added for an object that already has a `ctl`
- **THEN** it is a new command on that object's existing `ctl`
- **AND** no new global control file and no per-leaf `ctl` is introduced

#### Scenario: A leaf state file is not a control surface
- **WHEN** a consumer wants to change an agent's run-state
- **THEN** it writes a command to the owning object's `ctl` (`/proc/<pid>/ctl` for
  generic lifecycle, `machine/ctl` for runtime), never free text to a status leaf
- **AND** `machine/status` and `requests/<id>/status` remain read-only state

### Requirement: Tape and event streams are append-only and leased during generation
Alan OS SHALL keep `machine/tape` and every `events` stream append-only. While an
agent is generating, `machine/tape` SHALL be held under an exclusive-write lease —
exactly one writer (the generating engine), while readers may still tail it — so
no second writer can interleave records into the tape mid-stream. The safe window
for an external actor to amend `machine/tape` or `context/` is the agent's
yielded/paused state.

#### Scenario: A second writer attempts the tape during generation
- **WHEN** an agent is generating and another writer attempts to write
  `machine/tape`
- **THEN** the write is refused because the generating engine holds the
  exclusive-write lease
- **AND** the tape cannot be spliced mid-stream

#### Scenario: A reader tails the tape during generation
- **WHEN** a consumer tails `machine/tape` while the agent is generating
- **THEN** the read succeeds and resumes from the caller's offset
- **AND** the exclusive-write lease bars writers, not readers

#### Scenario: An external actor amends during a yield
- **WHEN** the agent is yielded/paused and an authorized actor amends
  `machine/tape` or `context/`
- **THEN** the amendment is accepted because no generation lease is held
- **AND** on resume the engine continues from the amended state

### Requirement: Write authority carries an actor dimension; extension is by interpose
Alan OS SHALL make write authority to an agent's files a function of the acting
actor (the agent's own engine, a parent, a human operator, an interposing file
server) and that actor's mounted capabilities — not a static property of the node
alone. The unit of behavior extension SHALL be interposing a file server on the
namespace, governed by who-may-{read, write, mount, interpose}, never a private
out-of-band API (the iron law). Until the kernel's amplification check
(ADR-0024 R1) lands, this boundary is convention-enforced, not claimed as hard
isolation.

#### Scenario: An extension changes agent behavior
- **WHEN** an extension must alter how an agent reads or writes a file
- **THEN** it interposes a file server between the writer and the agent files
- **AND** it does so through a mount governed by who-may-{write,mount,interpose},
  not a private API

#### Scenario: The same node is writable to one actor and not another
- **WHEN** two actors hold fids on the same node with different mounted authority
- **THEN** the write decision depends on the actor and its capabilities
- **AND** node-writability is not a single global property independent of actor

### Requirement: External writers require a protocol-layer tape lease
Alan OS SHALL NOT permit any actor other than the agent's own engine (a human
amending `machine/tape`/`context/`, or an interposing file server) to write the
agent namespace until the `machine/tape` exclusive-write lease is enforced at the
aP protocol layer rather than solely as a check inside the agent file server.
Because an interposing server bypasses the agent file server's own `write` path, an
agent-file-server-internal check MUST NOT remain the sole enforcement of the lease
once a second writer can exist; the protocol-layer guarantee is a prerequisite that
gates enabling any external-writer surface.

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

### Requirement: The agent namespace is self-describing
Alan OS SHALL let an agent's files describe their own byte contract in-band, so the
agent — itself a consumer that reads and writes these files to think — can read the
contract as prose rather than depending only on out-of-band documentation. The
minimal form SHALL be a documented record vocabulary per stream and a readable
`ctl`-help for each control surface, expanded incrementally.

#### Scenario: A consumer learns a surface's contract from the namespace
- **WHEN** an agent or tool needs to know a stream's record vocabulary or a
  `ctl`'s accepted commands
- **THEN** it reads an in-band description for that node
- **AND** it does not depend solely on external specs or Rust doc-comments
