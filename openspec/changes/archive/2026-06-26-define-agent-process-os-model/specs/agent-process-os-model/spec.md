## ADDED Requirements

### Requirement: Agent Process is a convention over a single Process category
Alan Kernel SHALL model execution with a single `Process` category (per ADR-0024
and `define-plan9-kernel-substrate`); there SHALL NOT be a separate
`Agent Process` kernel category. An "Agent Process" is an ordinary `Process`
recognized by conforming to the agent file layout (its AgentFS surfaces under
`/agent/<pid>`), discovered by walking the process directory rather than by a
kernel flag. It has ordinary process identity, parentage, credentials,
descriptors, lifecycle, input/output streams, status, and exit state.

#### Scenario: Process table is reviewed
- **WHEN** Alan Kernel process ontology is reviewed
- **THEN** it contains a single `Process` category
- **AND** agent-ness is a file-layout/AgentFS convention, not a kernel process
  kind, and there are no separate kinds for app, command, service, task, run, or
  subagent

#### Scenario: Child agent is spawned
- **WHEN** an Agent Process spawns another agent executable
- **THEN** the child is another Agent Process with normal process identity
- **AND** "subagent" is treated as a product/runtime phrase for a child Agent
  Process, not a Kernel primitive

### Requirement: Agent Processes are created by system call
Alan OS SHALL create Agent Processes by spawning or executing Agent
Executables through system-call semantics. Creating agent work SHALL NOT require
an app-facing RPC agent API or session creation API.

#### Scenario: App starts agent work
- **WHEN** an Alan App needs AI-mediated work
- **THEN** it opens the needed files, Skills, Memory Stores, and policy files as
  Descriptors and spawns an Agent Executable
- **AND** Alan OS returns a process identity visible in `/proc` and `/agent`

#### Scenario: Compatibility API is reviewed
- **WHEN** a current HTTP/WS session creation path is retained
- **THEN** it is documented as compatibility transport over spawn/open/watch
  behavior
- **AND** it does not become the canonical Alan OS model

### Requirement: Service Manager replaces daemon as the lifecycle concept
Alan OS SHALL define Service Manager as the system Process responsible for
starting, stopping, restarting, and supervising system services and boot units.
The former daemon concept SHALL be treated as a legacy implementation detail or
transport compatibility layer, not as Alan OS architecture.

#### Scenario: Alan OS starts
- **WHEN** Alan OS boots its system services
- **THEN** Service Manager starts required file-server services and the Root
  Agent Process boot unit
- **AND** Service Manager exposes its own management view as files under
  `/mnt/service`

#### Scenario: New service is proposed
- **WHEN** a future change proposes a new OS service
- **THEN** it is modeled as a Process exporting a file tree and optionally
  posting a handle under `/srv`
- **AND** it is not introduced primarily as a REST or HTTP service

### Requirement: Alan OS services are file servers
Alan OS services SHALL be long-running Processes that export file trees.
Processes SHALL use mount, bind, open, read, write, watch, and spawn semantics
to interact with services. Service handles SHALL be posted under `/srv`, while
canonical service trees SHALL be mounted at conventional namespace paths.

#### Scenario: Service tree is mounted
- **WHEN** Agent Runtime Service is available
- **THEN** it posts a handle under `/srv/agent-runtime`
- **AND** its AgentFS tree is mounted at `/agent`

#### Scenario: Service handle is inspected
- **WHEN** a process lists `/srv`
- **THEN** it sees posted mountable handles, not all service state
- **AND** service state remains in the mounted service tree

### Requirement: AgentFS exposes Agent Process surfaces
Agent Runtime Service SHALL serve AgentFS at `/agent`. `/agent/root` SHALL be a
stable alias to the Root Agent Process, and `/agent/<pid>` SHALL expose
agent-specific files for each Agent Process.

#### Scenario: Agent Process is inspected
- **WHEN** a process opens `/agent/<pid>`
- **THEN** it can discover standard files for status, control, children,
  context, requests, actions, IO, and machine state according to access rights
  (results are conveyed via IO output and per-action `actions/<id>/result`, not a
  top-level `result` file)
- **AND** the same Agent Process also appears in `/proc/<pid>`

#### Scenario: Root Agent is inspected
- **WHEN** a process opens `/agent/root`
- **THEN** it resolves to the current Root Agent Process
- **AND** Root Agent remains a real Agent Process, not the owner of AgentFS

### Requirement: AgentFS separates IO from machine state
AgentFS SHALL separate the external IO surface of an Agent Process from the
Turing-machine surface used by Agent Runtime Service. Agent IO SHALL be the
default surface for shells, apps, and users. Agent Machine SHALL contain tape,
machine state, transition events, and checkpoints.

#### Scenario: User follows an agent response
- **WHEN** Alan Shell or Alan Agent reads an Agent Process by default
- **THEN** it reads `/agent/<pid>/io/output` and watches
  `/agent/<pid>/io/events`
- **AND** it does not need to parse machine tape

#### Scenario: Runtime state is inspected
- **WHEN** a permitted debug or audit view opens `/agent/<pid>/machine`
- **THEN** it can inspect tape, state, machine events, and checkpoints
- **AND** those files remain Agent Runtime Service schema, not Kernel ontology

### Requirement: Requests and approvals are files
Agent Runtime Service SHALL represent Agent Process yield, confirmation,
approval, credential, selection, and structured-input requests as file trees
under `/agent/<pid>/requests`. Responses SHALL be written to files, not
submitted through a private resume API.

#### Scenario: Agent asks for confirmation
- **WHEN** an Agent Process needs confirmation
- **THEN** Agent Runtime Service creates `/agent/<pid>/requests/<request-id>`
  with kind, prompt, options, status, and response files
- **AND** Alan Shell, Alan Agent, or another permitted host answers by writing
  the response file

### Requirement: Agent actions are file-backed records
Agent-proposed or agent-initiated external effects SHALL be represented under
`/agent/<pid>/actions`. Tools SHALL remain executables; an action is one
specific proposed or running effect, optionally linked to a `/proc/<tool-pid>`
Process.

#### Scenario: Agent spawns a Tool
- **WHEN** an Agent Process spawns `/bin/apply-patch`
- **THEN** the Tool process appears in `/proc`
- **AND** `/agent/<agent-pid>/actions/<action-id>` records status, process link,
  stdout, stderr, result, risk, and approval state

### Requirement: Tools are executables
Alan OS SHALL model Tools as executable files installed into the command
namespace, usually by bind/union into `/bin`. Every Tool SHALL provide a quick
help message, a manual page, and a machine-readable manifest under
`/lib/exec/<tool>`.

#### Scenario: Agent discovers a Tool
- **WHEN** an Agent Process considers using a Tool
- **THEN** it can inspect `/bin/<tool> --help`, `/man/1/<tool>`, and
  `/lib/exec/<tool>/manifest`
- **AND** permission to execute the Tool comes from descriptors, access rights,
  namespace visibility, and policy, not from Skill text

### Requirement: Skills are manual-like packages
Alan OS SHALL model Skills as installable knowledge packages, not executables.
Agent Processes SHALL receive Skills canonically by descriptor passing; argv or
environment skill names are shell sugar for opening and passing descriptors.

#### Scenario: Shell starts a skilled agent
- **WHEN** Alan Shell runs `review --skill repo-coding src/lib.rs`
- **THEN** the shell resolves the Skill package, opens descriptors for
  `/lib/skill/repo-coding` and relevant manuals, and passes those descriptors
  to the spawned Agent Process
- **AND** the Skill does not grant permission to execute Tools or read files

### Requirement: Memory and policy are descriptor-passed file trees
Memory Stores and agent policies SHALL be file trees opened and passed to Agent
Processes as Descriptors. AgentFS MAY project effective memory and policy
descriptors for inspection, but it SHALL NOT own a global memory registry or
global agent policy database.

#### Scenario: Agent receives memory
- **WHEN** an Agent Process needs workspace and personal memory
- **THEN** the parent process opens the permitted Memory Store paths and passes
  descriptors at spawn time
- **AND** writes follow the store owner's access rights, consent, and policy

### Requirement: Root Agent Process has bounded default descriptors
Root Agent Process SHALL default to system-level index, notification,
continuity, process status, service event, and public app index descriptors. It
SHALL NOT default to app-private content, workspace-private files, user memory,
or other agents' machine tape.

#### Scenario: Root Agent sees useful cross-app work
- **WHEN** Root Agent Process observes public or permitted events that suggest
  useful work
- **THEN** it may raise a request, propose an action, or spawn a child Agent
  Process with explicitly opened descriptors
- **AND** it does not read private content without consent or access rights

### Requirement: Alan Shell is the primary OS interaction surface
Alan Shell SHALL be the primary user interaction surface for Alan OS. It SHALL
operate by reading and writing namespace files, spawning Processes and Agent
Processes, mounting or binding service trees, and inspecting `/proc`, `/agent`,
`/bin`, `/lib`, `/man`, and `/mnt` file trees.

#### Scenario: User operates agents without Alan Agent
- **WHEN** Alan Agent is not open
- **THEN** the user can still list Agent Processes, inspect Root Agent status,
  spawn agent executables, answer requests, and observe events through Alan
  Shell

### Requirement: Alan Agent is built-in but optional
Alan Agent SHALL be a built-in optional Agent Workspace app over Agent Process
files. It SHALL NOT be required for apps or users to spawn, inspect, steer, or
complete Agent Processes.

#### Scenario: User opens Alan Agent
- **WHEN** the user opens Alan Agent
- **THEN** it provides a richer workspace for `/agent`, `/proc`, requests,
  actions, memory, evidence, and cross-app work
- **AND** it does not become Root Agent Process, Agent Runtime Service, or
  Service Manager

### Requirement: Existing session concepts decompose into Agent Process files
Existing Alan Agent sessions SHALL migrate toward Agent Process file surfaces.
Session metadata maps to status, conversation IO maps to Agent IO, rollout and
runtime events map to IO or machine event streams, tape maps to Agent Machine
tape, and recovery maps to checkpoints.

#### Scenario: Existing session is attached
- **WHEN** a compatibility client attaches to an existing session
- **THEN** the compatibility layer can project that session as an Agent Process
  file surface
- **AND** future clients can attach by opening or watching AgentFS files instead
  of calling a session API
