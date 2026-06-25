## ADDED Requirements

### Requirement: Alan is the repo-level product constitution
Alan SHALL be the long-term programmable personal computing environment product
model for the alan repository, including Alan Agent, Alan Shell, Alan for
macOS, Alan Apps such as Groove Master, host surfaces, adapters, and future
products.

It SHALL NOT be treated merely as a feature of the existing macOS shell, HTTP
server, daemon-shaped compatibility code, terminal runtime, or agent session UI.
It also SHALL NOT be treated as an unrelated side product whose direction does
not apply to the rest of the repo.

#### Scenario: Future work scopes implementation
- **WHEN** a future OpenSpec change implements product, runtime, agent, host,
  app, adapter, extension, or surface behavior
- **THEN** the change identifies how the scope relates to the Alan OS model, or
  explicitly marks itself as legacy/compatibility work
- **AND** it does not assume that existing shell containers, terminal workflows,
  HTTP sessions, or agent session UI are the required implementation boundary

### Requirement: Alan OS is file and process oriented
Alan OS SHALL center its operating-system model on namespace, mounts, paths,
files, descriptors, access rights, credentials, ordinary Processes, Agent
Processes, process table, standard namespace roots, file-server services, and
service mount anchors.

Objects, buffers, views, queries, tasks, evidence, artifacts, plans, and
semantic snapshots SHALL be modeled as app, agent, service, or host
interpretations over files and processes unless a future spec explicitly proves
that a new Kernel primitive is necessary.

#### Scenario: Kernel scope is reviewed
- **WHEN** a future change adds Alan Kernel behavior
- **THEN** it explains the behavior in terms of file, descriptor, process,
  namespace, mount, credential, access, or service-handle semantics
- **AND** it does not introduce app/agent concepts as Kernel primitives without
  explicit justification

### Requirement: Standard namespace is layered
Alan OS SHALL keep its default top-level namespace roots small and stable:
`/proc`, `/agent`, `/srv`, `/bin`, `/lib`, `/man`, and `/mnt`.

`/proc`, `/agent`, and `/srv` SHALL be live process or service views. `/bin`,
`/lib`, and `/man` SHALL be command, package, and documentation roots. `/mnt`
SHALL be the conventional parent for mounted service, app, memory, policy, and
data trees. Alan-specific package trees such as Skills, Tool manifests, Memory
Stores, and policy packages SHALL NOT become new default top-level roots unless
a future spec explicitly justifies the exception.

#### Scenario: Alan-specific package is installed
- **WHEN** Alan installs Tool metadata, a Skill package, or a reusable policy
  package
- **THEN** the package lives under `/lib/exec`, `/lib/skill`, or `/lib/policy`
- **AND** its documentation lives under `/man`
- **AND** Alan does not create `/tool`, `/skill`, or `/policy` as default
  top-level roots

#### Scenario: Service or data tree is mounted
- **WHEN** Alan mounts a Service Manager view, Memory Store, policy service
  view, app tree, or external data tree
- **THEN** the mounted tree lives under `/mnt`
- **AND** Alan does not create `/service`, `/mem`, or `/policy` as default
  top-level roots

### Requirement: Alan owns the OS runtime environment
Alan SHALL own the OS runtime environment, including namespace, mounts, files,
descriptors, access rights, credentials, ordinary Processes, Agent Processes,
file-server services, Service Manager, AgentFS, Tools, Skills, Memory Stores,
policy descriptors, app registration, host integration conventions,
compatibility projections, evidence, and audit.

The current `alan-runtime` crate SHALL be treated as the current Agent Execution
Engine that can back Agent Runtime Service work until it is renamed or
replaced. The current HTTP/WS server SHALL be treated as compatibility
transport and legacy service implementation, not as Alan OS architecture. The
CLI SHALL be treated as the operator/client entrypoint. Neither surface SHALL
be described as Alan OS, Alan Kernel, Service Manager, Agent Runtime Service,
or an independent "agent backend" product.

#### Scenario: Current agent execution is described
- **WHEN** product or architecture docs describe `alan-runtime`
- **THEN** they describe it as the current Agent Execution Engine that can back
  Agent Runtime Service work
- **AND** they do not describe it as Alan OS or Alan Kernel

#### Scenario: Current compatibility transport is described
- **WHEN** product or architecture docs describe the current HTTP/WS server or
  CLI server
- **THEN** they describe that server as compatibility transport and legacy
  service implementation
- **AND** they describe the CLI as the operator/client entrypoint
- **AND** they do not describe either as Alan OS, Alan Kernel, Service Manager,
  Agent Runtime Service, or a separate Agent Backend product surface

### Requirement: Alan OS roles are explicit
Future and aligned existing specs SHALL identify their role in the Alan OS
family as one or more of:

- Alan Kernel;
- Service Manager;
- file-server service;
- built-in Alan App;
- Alan App;
- host or frame surface;
- native adapter;
- compatibility transport;
- legacy or compatibility surface.

#### Scenario: A runtime substrate is proposed
- **WHEN** a future change proposes a runtime substrate such as Alan Kernel
- **THEN** it identifies whether it owns Kernel primitives, file/process
  surfaces, namespace/mount behavior, descriptors, access rights, or extension
  descriptors
- **AND** it does not claim to be the whole Alan product
- **AND** it does not describe itself as middleware between a current backend
  such as Agent Execution Engine and a host such as Alan Shell

#### Scenario: A service is proposed
- **WHEN** a future change proposes a system service
- **THEN** it identifies the Process that serves it, the `/srv` handle or mount
  point, the file tree it exposes, and the descriptors/access rights clients
  use
- **AND** it does not define a separate app-facing RPC API as the canonical OS
  boundary unless the file surface is explicitly compatibility-only

#### Scenario: A host-surface change is proposed
- **WHEN** a future change modifies Alan for macOS, Alan Shell, or another host
- **THEN** it identifies host-owned concerns such as layout, windowing, native
  chrome, renderer state, input translation, and platform-specific presentation
- **AND** it does not redefine Alan Kernel files, processes, descriptors, or
  app-domain truth as host-owned state

#### Scenario: An Alan App is proposed
- **WHEN** a future change proposes a domain product such as Groove Master
- **THEN** it states the real user-facing product boundary first
- **AND** it identifies the Alan app adapter that maps domain files, commands,
  process work, views, and agent participation into the shared environment

### Requirement: Service Manager is the lifecycle manager
Alan OS SHALL use Service Manager as the canonical lifecycle concept for boot
units and system file-server services. The former daemon concept SHALL be
treated as compatibility implementation language, not the target architecture.

#### Scenario: A system service starts
- **WHEN** Alan OS boots or enables a system service
- **THEN** Service Manager starts or supervises the service Process
- **AND** the service publishes a handle under `/srv` or a documented service
  mount anchor
- **AND** clients interact with the mounted file tree rather than depending on
  daemon-specific lifecycle semantics

### Requirement: An agent is the standard OS execution form, as an ordinary Process
Alan OS SHALL define an agent as the standard execution form for agent work,
realized as an ordinary `Process` recognized by the agent file layout — not a
separate kernel category (ADR-0024 D3). Alan Apps and Alan Shell SHALL create
agent work by spawning Agent Executables with descriptors. Alan Kernel SHALL own
only a single `Process` category plus files, descriptors, access rights,
credentials, namespace, mounts, `/proc`, `/srv`, and service mount anchors; agent-
ness is a file-layout/AgentFS convention. Agent Runtime Service SHALL execute
agents and serve AgentFS outside Alan Kernel.

#### Scenario: App requests AI-mediated help
- **WHEN** an Alan App such as UPDF or Groove Master needs reading assistance,
  practice help, task planning, transformation, or command proposals
- **THEN** it opens bounded file, memory, skill, and policy descriptors and
  spawns an Agent Executable
- **AND** it does not need to embed an app-local chatbot or route ordinary app
  assistance through the Alan Agent UI
- **AND** it does not call an RPC-style agent API as the canonical OS
  mechanism

#### Scenario: Agent Process internals are reviewed
- **WHEN** Agent Process implementation details such as model calls, tape,
  requests, actions, policy, tool execution, skills, memory, or compaction are
  discussed
- **THEN** they are scoped to Agent Runtime Service, Agent Execution Engine,
  AgentFS, app adapters, or compatibility transport
- **AND** they are not added to Alan Kernel ontology

### Requirement: Agent Runtime Service serves AgentFS
Alan OS SHALL model Agent Runtime Service as a file-server service managed by
Service Manager. It SHALL execute Agent Processes and serve AgentFS at
`/agent`. `/agent/root` SHALL identify Root Agent Process. `/agent/<pid>` SHALL
expose Agent Process files such as status, control, children, request, action,
io, policy, context, and machine surfaces.

#### Scenario: AgentFS is mounted
- **WHEN** a host, Alan App, or Alan Shell needs to inspect agent work
- **THEN** it mounts or opens AgentFS files
- **AND** it reads and writes Agent Process state through file operations and
  descriptors
- **AND** it does not require Alan Agent to be running

### Requirement: Alan Agent is an optional Agent Workspace
Alan Agent SHALL be modeled as a built-in optional Agent Workspace app for
inspecting, steering, comparing, and organizing Agent Processes. Alan Agent
SHALL NOT be treated as Root Agent Process, Agent Runtime Service, Service
Manager, Alan Kernel, or the only way apps can run agents.

#### Scenario: Alan Agent is aligned
- **WHEN** Alan Agent execution engine, session, tool, plan, memory, child-agent,
  or governance work is aligned with this constitution
- **THEN** Alan Agent is treated as a built-in optional workspace over Agent
  Processes
- **AND** current Agent Execution Engine, provider, protocol, and compatibility
  transport paths are treated as internal implementation details or adapters
  during migration

### Requirement: Tools are executable command files
Alan OS SHALL model Tools as executable commands in `/bin` or bound into
`/bin`. Each Tool SHOULD expose human help through `--help`, a manual page at
`/man/1/<tool>`, and machine-readable metadata at
`/lib/exec/<tool>/manifest`.

#### Scenario: Agent uses a Tool
- **WHEN** an Agent Process needs external action
- **THEN** it executes a Tool command with descriptors and argv/env inputs
- **AND** policy/governance evaluates the proposed action at the Agent Runtime
  Service layer
- **AND** the Tool is not modeled as a JSON callback or RPC agent method

### Requirement: Skills are manual-like knowledge packages
Alan OS SHALL model Skills as knowledge and instruction packages installed
under `/lib/skill/<name>` and surfaced through `/man/skill/<name>`. Skills
SHALL be passed to Agent Processes by descriptor. Shell names, argv, or
environment variables MAY be ergonomic sugar over descriptor passing.

#### Scenario: Agent uses a Skill
- **WHEN** an Agent Process is launched with a Skill
- **THEN** Agent Runtime Service receives a descriptor to the Skill package or
  its mounted file tree
- **AND** the Skill contributes instructions, examples, or reference material
- **AND** the Skill is not treated as the executable action surface

### Requirement: Existing specs align through environment metadata
Selected active and future OpenSpec changes SHALL include enough environment
alignment metadata for reviewers to understand how the change fits the
constitution.

At minimum, aligned specs SHOULD state:

- Alan OS role;
- file/process/service mapping;
- native source-of-truth boundary;
- host/rendering/layout boundary where relevant;
- deferred migration or compatibility boundary.

#### Scenario: Existing macOS shell component work is reviewed
- **WHEN** `add-macos-shell-component-system` or equivalent host presentation
  work is aligned with this constitution
- **THEN** it is classified as a macOS host surface/design-system capability
- **AND** it may own presentational primitives, tokens, preview galleries, and
  host accessibility rules
- **AND** it does not claim ownership of Alan Kernel primitives

### Requirement: Roadmap sequencing is OS spine before app and host migration
Alan OS roadmap planning SHALL sequence work in this order unless a future spec
explicitly justifies a narrower compatibility exception:

1. Alan OS spine;
2. Alan Agent on Agent Processes;
3. Alan Shell and Alan for macOS as hosts;
4. Groove Master as the first domain Alan App;
5. UPDF as a complex content Alan App.

The Alan OS spine SHALL mean the minimal Kernel, Service Manager contract,
file-server service model, process/file/descriptor/access path, `/srv`, `/proc`,
app registration, and compatibility projection needed to host real apps. It
SHALL NOT mean that every future OS capability must be complete before app
migration begins.

#### Scenario: Existing Alan products are migrated
- **WHEN** existing Alan products are migrated onto Alan OS
- **THEN** Alan Agent is migrated first as a built-in optional Agent Workspace
  over Agent Processes
- **AND** Alan Shell and Alan for macOS are migrated as hosts that consume Alan
  file/process/service surfaces rather than owning app or Kernel truth

### Requirement: Product is local-first and filesystem-first
The product SHALL treat local-first operation, filesystem-backed inspection,
and UNIX-like composition as core product principles.

#### Scenario: Product stores or represents user work
- **WHEN** the product stores state, discovers work, creates outputs, or
  represents resources
- **THEN** it prefers ordinary files, directories, manifests, sidecars, caches,
  imports, exports, mounts, service file trees, or projections where practical
- **AND** it does not require a proprietary object store or universal URI layer
  as the first product principle

### Requirement: Data ownership and activity organization are separate
The product SHALL allow source data to remain in filesystems, projects,
repositories, external services, databases, or OS resources while organizing
user activity through Alan files, mounted service trees, app surfaces, host
views, and process work.

#### Scenario: External or existing data is used
- **WHEN** a file, project, terminal, Git state, note, task, mail item, calendar
  item, recording, loop, or remote record is brought into the environment
- **THEN** the product can represent it through a mounted file tree or app file
  surface without requiring Alan to become the source of truth for the
  underlying data

### Requirement: Agents are first-class actors through files and processes
Agents SHALL be first-class actors alongside humans by operating through Alan
OS files, descriptors, processes, mounted services, Tools, Skills, permissions,
and audit surfaces.

#### Scenario: Agent performs work
- **WHEN** an Agent Process reads context, writes output, proposes an action,
  executes a Tool, opens a view, queries state, updates memory, or changes a
  plan
- **THEN** the action is mediated by descriptors, files, process state, service
  files, or app commands
- **AND** the agent does not bypass access rights or agent action governance to
  mutate underlying systems directly

### Requirement: Alan Apps are real products
Alan Apps SHALL be treated as user-facing domain products inside Alan, not as
demos whose primary purpose is proving Alan OS.

#### Scenario: App product is proposed
- **WHEN** a future change proposes or updates an Alan App
- **THEN** it defines the app's user-facing job, domain files or objects,
  domain commands, and product experience before describing Alan OS proof value
- **AND** its domain core remains separable from the current Alan host
  implementation

### Requirement: Extensibility uses Rust core plus WASM components
The product SHALL treat a Rust core plus WASM Component Model extensions and
WIT interfaces as the product's extension direction.

#### Scenario: Extension capability is added
- **WHEN** a future change adds extension behavior
- **THEN** it uses explicit descriptor and access grants for filesystem access,
  network access, command execution, service mounts, buffer/view access, query
  registration, or agent participation

### Requirement: Incubation validates product assumptions
Future incubation work SHALL validate the product constitution before expanding
into a broad environment implementation.

#### Scenario: First incubation scope is proposed
- **WHEN** a future change proposes the first prototype, runtime substrate, or
  MVP for this product
- **THEN** it identifies the concrete workflow or substrate boundary that proves
  local-first discovery, file/process representation, mounted service use,
  command execution, host rendering, agent participation, and extension-shaped
  boundaries
- **AND** it states which constitution criteria are proven by the slice and
  which are intentionally deferred
