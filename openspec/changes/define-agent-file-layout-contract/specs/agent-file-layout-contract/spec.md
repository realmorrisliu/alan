## ADDED Requirements

### Requirement: An agent is a conforming process, not a kernel type
Alan OS SHALL treat an agent as a `Process` whose directory conforms to the
agent file-layout convention. Agent-ness SHALL be discoverable by walking the
process directory, not by any kernel flag or kernel category.

#### Scenario: A process is tested for agent-ness
- **WHEN** a tool needs to know whether a process is an agent
- **THEN** it walks `/proc/<pid>` and checks for the agent layout (such as
  `machine/` and `requests/`)
- **AND** no kernel agent type is consulted

#### Scenario: A third-party runtime exposes agents
- **WHEN** a third-party runtime file server exports process directories that
  conform to this contract
- **THEN** existing shells and tools operate its agents unchanged
- **AND** the runtime needs no kernel changes to be operable

### Requirement: Every process exposes the generic process layout
Alan OS SHALL define a generic process layout that every process exposes: an
`io/` directory with `input`, `output`, and `events` streams, a `status` file,
and a `ctl` control file.

#### Scenario: A non-agent process is operated
- **WHEN** a consumer opens a non-agent process directory
- **THEN** it finds `io/`, `status`, and `ctl`
- **AND** `cat io/output`, reading `status`, and writing `ctl` work the same as
  for an agent

#### Scenario: Output is complete and tail-reachable
- **WHEN** a consumer reads `io/output`
- **THEN** the stream is append-only with monotonic offsets and exposes its full
  produced content (subject to retention), so a reader can always resume to the
  newest bytes
- **AND** clipping of the newest output is a renderer concern, never a gap in the
  stream's data

### Requirement: An agent extends the process layout
Alan OS SHALL define the agent layout as a strict superset of the generic
process layout, adding `requests/`, `actions/`, `machine/`, `context/`,
`children/`, and a top-level `events` stream. The `machine/` directory SHALL
contain the tape, machine state, and checkpoints. `children/` SHALL be a view of
the agent's child processes derived from `/proc` parentage (not a second source
of truth). The top-level `events` stream SHALL be an aggregate, watchable by
blocking read, over the agent's lifecycle, IO, request, action, and child
changes, so a watcher can follow the whole agent from one stream.

#### Scenario: An agent directory is inspected
- **WHEN** a consumer opens an agent's `/proc/<pid>`
- **THEN** it finds the generic `io/`, `status`, `ctl` plus `requests/`,
  `actions/`, `machine/`, `context/`, `children/`, and `events`
- **AND** reading `io/output` works whether or not the process is an agent

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

### Requirement: Control is expressed by writing to `ctl`
Alan OS SHALL express control of processes and agents as text commands written
to the `ctl` file. New control actions SHALL be added as new `ctl` commands, not
as new files or APIs.

#### Scenario: An agent is interrupted
- **WHEN** an operator interrupts an agent
- **THEN** the operator writes an interrupt command to `/proc/<pid>/ctl`
- **AND** no dedicated interrupt file or out-of-band API is required

#### Scenario: A request is answered
- **WHEN** a user answers an agent request
- **THEN** the answer is written to the request's response file under
  `requests/<id>/`
- **AND** the agent runtime delivers it without a private resume API

### Requirement: `/agent` is a view over `/proc`
Alan OS SHALL present `/agent` as a derived view over `/proc`: a union or bind of
agent-conforming process directories, with stable aliases such as `/agent/root`
resolving to whichever pid currently embodies the root agent's home. `/agent`
SHALL NOT be a second, independent process table.

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
`machine/tape`, `context/`, and the `/bin` tools visible in the namespace. Tape
compaction SHALL be a view over `machine/tape` (tape is truth; the context-window
view is what is sent), not a hidden runtime step. An agent's available tools
SHALL be exactly the `/bin` entries visible in its namespace.

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
- **THEN** they are the `/bin` entries visible in the agent's namespace with
  their manifests
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
