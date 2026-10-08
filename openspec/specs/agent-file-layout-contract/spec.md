# agent-file-layout-contract Specification

## Purpose
Defines the generic Process layout and the `/agent` overlay convention,
including namespace-derived capabilities, file-backed IO with explicit input
routing, machine state, request/action trees, stream authority, access rights,
and durability ownership.
## Requirements
### Requirement: An agent is a conforming process, not a kernel type
alan9 SHALL treat an agent as a `Process` whose `/agent/<pid>` overlay conforms
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
alan9 SHALL define a generic process layout that every process exposes: the
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

### Requirement: Agent input records declare routing explicitly
An input operation written to an Agent Process SHALL use the canonical
`type: "input"` record and SHALL include an explicit `mode`. Alan SHALL NOT
accept the retired `type: "steer"` alias or infer a mode when the field is
absent.

#### Scenario: Explicit input is submitted
- **WHEN** a caller submits `type: "input"` with a supported explicit mode and
  valid parts
- **THEN** the Agent Process routes the input according to that mode

#### Scenario: Input mode is absent
- **WHEN** a caller submits `type: "input"` without `mode`
- **THEN** the record is rejected as malformed
- **AND** Alan does not infer `steer` or any other routing behavior

#### Scenario: Retired steer operation is submitted
- **WHEN** a caller submits an operation with `type: "steer"`
- **THEN** the record is rejected as an unsupported operation shape
- **AND** the caller must resubmit canonical explicit input

### Requirement: An agent overlays agent files on the generic process layout
alan9 SHALL define the agent layout as the generic process layout plus an agent
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
alan9 SHALL express control as text commands written to a `ctl` file, split by
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
alan9 SHALL present `/agent` as an overlay over `/proc`: for each
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
alan9 SHALL model the LLM as a typed stream a process reads. The LLM SHALL have
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
alan9 SHALL assemble the logical model request as a view over namespace files:
`machine/tape`, `context/`, visible Tool packages, and the Agent Runtime
Service's defined interaction/governance control operations. Tape compaction
SHALL be a view over `machine/tape` (tape is truth; the context-window view is
what is sent), not a hidden runtime step. An agent's executable model-callable
Tools SHALL be exactly the visible `/bin` entries that carry a valid Tool
manifest at `/lib/exec/<tool>/manifest`. Agent Executables and ordinary commands
in the `/bin` union are spawn targets, not model-callable Tools, and SHALL NOT
appear in the request's Tool list. Tool definition, capability, locality, and execution
metadata SHALL come from the mounted package files, with no separate catalog or
registry authority. Runtime-owned request, mount, plan, and delegation controls
MAY expose model-callable operation schemas without pretending to be executable
Tool packages; their handlers SHALL write the corresponding AgentFS, namespace,
or machine-control surfaces and SHALL NOT grant `/bin` execution authority.

#### Scenario: Context is changed
- **WHEN** a file is bound into or removed from an agent's `context/` or `/bin`
- **THEN** the next assembled request reflects the change
- **AND** no separate prompt-assembly configuration is edited

#### Scenario: The provider is changed
- **WHEN** the provider mount is rebound to a different LLM file server
- **THEN** the agent's request assembly is unchanged
- **AND** only the provider-local wire translation differs

#### Scenario: The model's Tool list is computed
- **WHEN** the request's available Tools are computed
- **THEN** they are the visible `/bin` entries with valid manifests under
  `/lib/exec/<tool>`, excluding Agent Executables and ordinary commands
- **AND** there is no separate Tool catalog or registry granting Tools outside
  the namespace

#### Scenario: A mounted Tool package is incomplete
- **WHEN** the executable or manifest half of a Tool package is absent or the
  manifest cannot be validated
- **THEN** request assembly does not expose that entry as a model-callable Tool
- **AND** the failure identifies the incomplete mounted package rather than
  consulting process-global defaults

#### Scenario: Runtime exposes an interaction control
- **WHEN** the Agent Runtime Service exposes a request, mount, plan, or
  delegation control to the model
- **THEN** request assembly identifies it as a runtime-owned control operation,
  not a mounted executable Tool package
- **AND** invoking it writes its defined file/control surface without spawning
  an arbitrary `/bin` executable

### Requirement: Referenced capability file servers have explicit mount boundaries
alan9 SHALL treat the LLM provider, Memory Store, Tool, and Skill capabilities
referenced by this contract as external file-server surfaces, not fields on the
agent runtime. The referenced surfaces are:

- LLM provider access: `alan-llmfs` posts `/srv/llm` and serves `/mnt/llm`;
  Providers are introspection surfaces under `/mnt/llm/providers/<provider>`,
  while callable Connections live under `/mnt/llm/connections/<connection>`.
- Memory Stores: storage-backed Memory Store file servers post handles such as
  `/srv/mem` and are mounted for process use under `/mnt/mem` or passed as
  narrower descriptors/binds. They own memory authority; memory kinds describe
  usage, not ownership.
- Tools: Tool file servers contribute executable command files unioned into
  `/bin`, with machine-readable manifests at `/lib/exec/<tool>/manifest` and
  manuals under `/man/1`.
- Skills: package file servers expose manual-like Skill packages under
  `/lib/skill/<name>` and documentation under `/man/skill/<name>`. Skills are
  passed by descriptor and do not execute or grant authority by themselves.

The agent file layout MAY project references to these surfaces, but it SHALL NOT
own their global registries, provider wire formats, memory authority, tool
catalog implementation, or skill package resolution. Those detailed protocols
belong to their own OpenSpec capabilities.

#### Scenario: A spawner assembles an agent's capabilities
- **WHEN** an agent process is spawned
- **THEN** the spawner binds only the selected LLM Connection, Memory Store
  descriptors, Tool command files, and Skill package descriptors into the
  process namespace
- **AND** any unbound provider, store, tool, or skill is structurally absent from
  the agent's world rather than denied by a side-channel API

#### Scenario: Memory is read or written
- **WHEN** an agent recalls, flushes, or promotes memory
- **THEN** it does so through a Memory Store path or descriptor in its namespace
- **AND** the agent runtime does not use a global memory registry outside the
  namespace to grant authority

#### Scenario: Tool and Skill packages are both present
- **WHEN** a Skill package includes instructions and package-local executables
- **THEN** the Skill contributes knowledge through its descriptor, while any
  executable effect is available to the agent only if a Tool command is bound
  into `/bin` or another explicit execution path
- **AND** a Skill package does not become an executable Tool merely by being
  discovered

### Requirement: Requests and actions are files with events
alan9 SHALL represent agent yield, confirmation, approval, credential,
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
alan9 SHALL keep request and action status truthful. A response written to a
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
alan9 SHALL keep awareness and authority separate for the Root Agent. Because a
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
alan9 SHALL make an agent's durable identity a home file tree (config, memory,
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
alan9 SHALL place model cost, metering, and rate-limiting in the LLM provider
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
alan9 SHALL place a `ctl` in the directory that represents a single
lifecycle-bearing object, alongside that object's data/status — the Plan 9 idiom
(`/proc/<pid>/ctl`, `/net/tcp/<n>/ctl`). The only `ctl` files in an agent's
overlay SHALL be the kernel-owned `/proc/<pid>/ctl` (aliased into `/agent/<pid>`
by the overlay, per "An agent overlays agent files on the generic process
layout") for generic process control, and the agent-runtime-owned `machine/ctl`
for tape/checkpoint control. There SHALL NOT be a second, agent-runtime-owned
catch-all control file placed at `/agent/<pid>/ctl` that re-encodes object
addressing as a verb argument or duplicates/shadows the kernel's generic `ctl`,
nor a `ctl` on a leaf that is pure state (e.g. `machine/tape/ctl`). Leaf state
files (`machine/status`, `requests/<id>/status`) are read-only and carry no
control.

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
alan9 SHALL keep `machine/tape` and every `events` stream append-only. While an
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
alan9 SHALL make write authority to an agent's files a function of the acting
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
alan9 SHALL NOT permit any actor other than the agent's own engine (a human
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
alan9 SHALL let an agent's files describe their own byte contract in-band, so the
agent — itself a consumer that reads and writes these files to think — can read the
contract as prose rather than depending only on out-of-band documentation. The
minimal form SHALL be a documented record vocabulary per stream and a readable
`ctl`-help for each control surface, expanded incrementally.

#### Scenario: A consumer learns a surface's contract from the namespace
- **WHEN** an agent or tool needs to know a stream's record vocabulary or a
  `ctl`'s accepted commands
- **THEN** it reads an in-band description for that node
- **AND** it does not depend solely on external specs or Rust doc-comments

### Requirement: Agent Machine persistence is Process-owned
Agent Runtime Service SHALL persist and restore Agent Machine tape, transition state, checkpoints,
requests, actions, and renderer projections through files owned by the Agent Process and its
durable backing stores. The Process path and durable record identifiers SHALL be sufficient to
locate and interpret that state.

#### Scenario: An Agent Process resumes durable machine state
- **WHEN** Agent Runtime Service restores an Agent Machine from durable rollout or checkpoint files
- **THEN** the restored state is associated with the concrete Agent Process and AgentFS layout
- **AND** the rollout or checkpoint retains the durable provenance needed to interpret the state

### Requirement: Agent Machine confirmation state is file-backed
Runtime confirmation requests and decisions SHALL be recorded through Agent Machine checkpoint,
request, action, and tape files with Process-visible provenance.

#### Scenario: A confirmation decision resumes execution
- **WHEN** an authorized client writes a confirmation decision through the owning request or control
  file
- **THEN** Agent Runtime Service records the decision against the current tape/checkpoint state
- **AND** execution resumes from the recorded decision and Agent Machine state

### Requirement: AgentFS is the complete observable runtime-state boundary
AgentFS SHALL own Agent Process output, tape, requests, actions, machine
snapshots, renderer-safe UI state, and ordered update streams under `/agent`,
while `/proc` SHALL own generic Process state. Hosts and supervisors SHALL NOT
require an engine handle, callback, or broadcast receiver to observe equivalent
live state.

#### Scenario: Host attaches after a turn has started
- **WHEN** a host opens an already-running Agent Process
- **THEN** it hydrates snapshots and resumes streams from `/agent/<pid>` and
  reads generic lifecycle from `/proc/<pid>`
- **AND** attachment does not depend on having received earlier in-memory events

#### Scenario: Parallel live state API is added
- **WHEN** current engine code exposes output, request, action, machine, or UI
  state through a callback or publish/subscribe channel
- **THEN** repository verification fails because the owning file surface is the
  complete observable boundary

### Requirement: Request cancellation belongs to the request owner
AgentFS SHALL expose a writable `ctl` on each request object at
`/agent/<pid>/requests/<request-id>/ctl`. An authorized descriptor write of
`cancel` SHALL atomically transition only that pending request to `cancelled`
under the same owner lock used to settle responses. Cancellation SHALL leave
its response unchanged and SHALL publish request/status invalidation through
the existing event surfaces. Request `status` SHALL remain read-only.

Cancellation of an already cancelled request SHALL be idempotent. An answered
or closed request SHALL retain its actual terminal state and response when
cancellation races with settlement; cancellation SHALL NOT overwrite either.
Malformed control, unavailable requests, and missing write rights SHALL fail
without mutating any request or granting execution authority.

#### Scenario: A pending request is cancelled
- **WHEN** the owning Agent Process cancels its exact pending request through its request control descriptor
- **THEN** AgentFS publishes `cancelled` for that request with its response unchanged
- **AND** request watchers invalidate the outstanding question or confirmation
- **AND** unrelated requests and their responses remain unchanged

#### Scenario: A response descriptor predates cancellation
- **WHEN** a response descriptor opened while a request was pending is closed after cancellation wins
- **THEN** response settlement is rejected against the current terminal status
- **AND** the response and cancelled state are not overwritten and the Process is not resumed

#### Scenario: An actual answer wins a cancellation race
- **WHEN** an authorized response has already settled the request before its cancel operation
- **THEN** the existing answered state and actual response remain authoritative
- **AND** cancellation does not manufacture a rejection, approval, or second terminal event

### Requirement: Machine cancellation settles its owned requests before reset
The Agent Execution Engine SHALL settle only the confirmation and structured
input requests associated with the current Machine logical waits before
clearing those waits at the shared turn reset or cancellation boundary.
It SHALL address the service-assigned request IDs within the pinned actual
Agent Process path through ordinary namespace descriptors. A checkpoint ID
SHALL NOT be substituted for a request directory ID. Host Mount requests SHALL
continue to settle through their owning service and existing durable evidence.

Request cancellation or terminal-evidence failure SHALL preserve any unsettled
logical association and SHALL NOT be treated as successful reset. Engine
cancellation SHALL NOT synthesize a user response, scan and cancel unrelated
pending requests, make status writable, or introduce a direct Engine dependency
on AgentFS. Existing Process lifecycle and input cancellation evidence SHALL
remain authoritative.

#### Scenario: A confirmation is interrupted
- **WHEN** cancellation interrupts a Tool while its confirmation is pending
- **THEN** the Machine-associated AgentFS request reaches truthful terminal state before its logical wait is cleared
- **AND** existing watchers remove the approval prompt so ordinary shell commands and natural exit are usable
- **AND** a cancellation that wins before approval does not spawn the Tool, grant authority, or replay queued work

#### Scenario: Cancellation cannot settle its request
- **WHEN** the owner operation or required terminal evidence fails
- **THEN** the corresponding Machine wait association remains recoverable
- **AND** the Engine does not claim successful cancellation or reset for that unsettled association

### Requirement: Evaluation snapshots cannot become a second write authority
The `machine/evaluation` file SHALL project the latest acknowledged Machine observation as a bounded versioned snapshot. Agent Runtime Service SHALL publish it through the owning AgentFS in-process update path. The public aP node SHALL reject write-intent opens and direct writes. Historical evidence SHALL remain in rollout/checkpoint storage.

#### Scenario: No observation exists
- **WHEN** a fresh Machine has no acknowledged evaluation evidence
- **THEN** its projection reports unknown or no observation rather than successful selection

#### Scenario: Advice is recovered into another Process
- **WHEN** explicit recovery reconstructs evaluation evidence under a new PID
- **THEN** the projection retains the source observation and captured Connection identity
- **AND** publication neither dispatches an evaluator nor settles ordinary queued input

### Requirement: Machine control transport admits bounded complete envelopes
AgentFS `machine/ctl` SHALL accept one nonempty UTF-8 control record per write up
to and including 64 KiB, including any prefix and serialized envelope. It SHALL
reject larger records and CR, LF or NUL without publishing a control event.
AgentFS SHALL record the complete accepted record once; semantic validation and
program selection SHALL remain owned by Agent Runtime Service.

#### Scenario: A valid owner request exceeds the former 8 KiB transport limit
- **WHEN** `agent_work select_owner` encodes a valid request with an 8 KiB question
  into a control record within 64 KiB
- **THEN** the writable AgentFS transport accepts the complete record once
- **AND** the submission receipt retains the encoded work UUID
- **AND** the request does not become ordinary text in `io/input`

### Requirement: Evaluation observations project existing Machine evidence
AgentFS SHALL expose evaluation identity, schema, captured Connection/model and terminal or waiting outcome as read-only Machine state. Agent Machine SHALL own transition updates and rollout/checkpoint SHALL retain durable evidence. Evaluation SHALL NOT introduce a second Tape, mutable result authority or Process kind.

#### Scenario: An evaluation completes without prose
- **WHEN** the Machine receives a validated structured evaluation result
- **THEN** the result and provenance are observable through its Machine projection and durable execution evidence
- **AND** io/output need not manufacture an assistant response
- **AND** Process exit remains distinct from evaluation completion

#### Scenario: External writer attempts to change advice
- **WHEN** a client writes to the evaluation observation
- **THEN** the write is rejected
- **AND** only Machine-owned controls can advance the current transition

### Requirement: Work results project acknowledged Machine completion
AgentFS SHALL expose `machine/work` as a read-only version-1 document with
`work` null before any observation and the latest acknowledged work snapshot
otherwise. A snapshot SHALL contain `version`, `work_id`, `source_rollout_id`,
`request_sha256`, `state` and spent model attempt counts. The full validated
request and candidate evidence SHALL remain in the owning rollout snapshot. State SHALL distinguish started, completed, waiting, failed,
cancelled and interrupted. Completed work SHALL include `owner` and citations
with namespace path, inclusive line range and SHA-256 of the captured range bytes;
waiting work SHALL include an owned request reference and boundary reason.
Completed result SHALL be at most 8 KiB; the bounded nonterminal recovery snapshot
MAY retain the validated request and candidate evidence within the existing 1 MiB
document ceiling. Machine SHALL acknowledge rollout evidence before publication;
AgentFS SHALL neither validate work history nor acquire a second write authority.
`machine/evaluation` SHALL retain separate model advice provenance; Action results
SHALL remain Tool completion metadata. Work completion SHALL NOT imply Process exit
or manufacture an assistant message in `io/output`.

#### Scenario: A client attempts to alter a work result
- **WHEN** a client opens `machine/work` with write intent or directly writes it
- **THEN** AgentFS rejects the write and preserves the Machine-owned snapshot

#### Scenario: Completed work is recovered
- **WHEN** explicit recovery reconstructs an acknowledged completed work under a new PID
- **THEN** its work UUID, source rollout identity, owner and citations remain unchanged
- **AND** recovery publishes no synthetic answer and dispatches no model call
