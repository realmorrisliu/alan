## Context

This change defines the product constitution for the alan repo. The long-term
product model is Alan: a programmable personal computing environment that is
local-first, UNIX-respecting, Plan 9-inspired, extensible, and agent-native.
Alan OS is the operating-system boundary inside that product.

This is not a new product beside Alan. Alan Agent, Alan Shell, Alan for macOS,
future Alan Apps such as Groove Master, host surfaces, runtime substrates, and
adapters should all be able to describe how they run on Alan OS. The
constitution is therefore a repo-level organizing model, not a current
implementation boundary.

The current Alan product already has useful assets: agent execution, terminal
workflows, permission checks, session state, shell workspaces, model/provider
integration, skills, memory, and OpenSpec-driven planning. Those are source
material. They should not force Alan OS to inherit today's HTTP server, daemon
shape, session API, TUI boundary, or app-specific runtime model.

The key product shift is to make the substrate smaller and more UNIX-like:
everything important should be addressable as files, descriptors, processes,
namespaces, mounts, and file-server services. Richer concepts such as objects,
buffers, views, queries, evidence, artifacts, tasks, plans, and semantic
snapshots are valuable, but they live above Kernel as app, agent, service, or
host interpretations over files and processes.

## Goals / Non-Goals

**Goals:**

- Define Alan and Alan OS as the repo-level product constitution.
- Preserve Alan OS from being collapsed into the current macOS shell, terminal
  UI, HTTP/WS server, CLI server, or agent chat UI.
- Establish file/process/descriptor/namespace composition as the core product
  principle.
- Make an agent the standard OS execution form as an ordinary Process recognized
  by the agent file layout (not a separate kernel type; ADR-0024 D3).
- Define Agent Runtime Service as a Plan 9-style file-server service serving
  AgentFS, not an app-facing HTTP API.
- Separate Agent Executable, Tool, Skill, Memory Store, policy descriptor, and
  Alan Agent workspace into clear layers.
- Define how future OpenSpec changes declare their Alan OS role and migration
  boundary.
- Define roadmap order without freezing the complete implementation.

**Non-Goals:**

- Implement Alan Kernel, Service Manager, Agent Runtime Service, AgentFS, or a
  real 9P stack in this change.
- Replace the current Agent Execution Engine, HTTP/WS compatibility transport,
  CLI, terminal, or macOS behavior in this change.
- Define objects, tasks, evidence, artifacts, or semantic views as Kernel
  primitives.
- Define a universal URI/resource protocol.
- Rewrite every existing OpenSpec in this change.
- Choose the first complete visual interface.

## Product Family Model

Future specs should classify themselves against this hierarchy:

```text
Alan OS (programmable personal computing environment)
  |
  +-- Alan Kernel
  |     namespace, mounts, paths, files, descriptors, access rights,
  |     credentials, a single Process category (agent-ness = file layout),
  |     standard namespace roots: `/proc`, `/agent`, `/srv`, `/bin`,
  |     `/lib`, `/man`, `/mnt`
  |
  +-- Service Manager
  |     starts, stops, restarts, reaps, and supervises boot units and
  |     system file-server services; replaces the former daemon concept
  |
  +-- File-server services
  |     Agent Runtime Service, credential/profile service, memory service,
  |     app services, host integration services; services post handles in
  |     `/srv` and are mounted or bound into namespaces, usually under `/mnt`
  |
  +-- Agent Runtime Service / AgentFS
  |     executes Agent Processes, maintains Root Agent Process, serves
  |     `/agent`, projects requests/actions/io/machine files
  |
  +-- Alan Apps and packages
  |     built-in app: Alan Agent
  |     domain apps: Groove Master, UPDF-like workflows, future products
  |     app-owned domain models mapped into Alan files/processes
  |
  +-- Hosts / frame surfaces
  |     Alan Shell, Alan for macOS, future iOS/iPadOS/web hosts;
  |     physical layout, native chrome, input translation, rendering,
  |     windowing, and host-specific interaction
  |
  +-- Native and domain adapters
  |     existing Agent Execution Engine, filesystems, Git, terminal,
  |     model/provider systems, app-owned stores, remote services
  |
  +-- Compatibility paths
        current HTTP/WS session routes, CLI server behavior, daemon-shaped
        code, and client adapters that stay stable while migration happens
```

This model lets a spec be concrete without pretending to be the whole product.
For example:

- `introduce-alan-kernel-runtime` is a Kernel incubation slice, not middleware
  between the current Agent Execution Engine and Alan Shell.
- `define-agent-process-os-model` defines the standard Agent Process model that
  agent-enabled apps use by spawning Agent Executables with descriptors.
- Alan Agent is a built-in optional Agent Workspace app. It inspects and steers
  Agent Processes through AgentFS and host surfaces; it is not Root Agent
  Process, Agent Runtime Service, Service Manager, or the only way to run
  agents.
- Alan Shell is the primary Alan OS interaction surface. It should evolve from
  today's compatibility client toward a shell over namespaces, files,
  descriptors, processes, and mounts.
- Alan for macOS is a host/frame surface. It owns native windows, chrome,
  input, layout, and rendering, not Kernel truth.
- `add-macos-shell-component-system` is a macOS host surface/design-system
  capability. It can make Alan views coherent in the macOS host, but it is not
  Alan Kernel.
- Groove Master is an Alan App. Its domain core owns musical practice behavior
  while an Alan adapter maps sessions, loops, journals, and agent assistance
  into Alan OS files/processes.

## Roadmap Sequence

The durable roadmap is:

1. **Alan OS spine:** finish the minimal Kernel, Service Manager contract,
   file-server service model, process/file/descriptor/access path, `/srv`,
   `/proc`, app registration, and compatibility projection needed to host real
   apps. This does not mean building every future OS feature first.
2. **Alan Agent on Agent Processes:** migrate the existing agent product so the
   current Agent Execution Engine backs Agent Runtime Service, Agent work is
   represented as Agent Processes, and Alan Agent becomes an optional workspace
   over `/agent`.
3. **Alan Shell and Alan for macOS as hosts:** migrate existing UI surfaces so
   they mount, watch, render, and write Alan OS file/process surfaces rather
   than owning app or Kernel truth.
4. **Groove Master as the first domain Alan App:** bring a non-agent product
   onto Alan OS with an app-owned domain core and Alan app adapter.
5. **UPDF as a complex content Alan App:** validate document files, multi-target
   buffers/views, comments, import/export, preview, and long-running publishing
   tasks after the App/Host contracts are exercised.

This roadmap avoids waiting for a perfect Alan OS. The first milestone is an OS
spine strong enough to support one real app. Each following migration should
tighten shared files/process/service contracts rather than bypassing them with
private host or compatibility paths.

## Decisions

### 1. Product constitution first

The first artifact should define what Alan is, not how to implement the whole
runtime. Alan is broad enough that an implementation-oriented first change would
collapse it into whichever subsystem is easiest to build first.

Alternative considered: write a detailed architecture RFC for object graph,
command registry, buffer/view, query, WASM, and agent actors. Those pieces are
useful later, but too early as the constitutional baseline because they risk
introducing richer ontology before the file/process substrate is stable.

### 2. Alan OS is file/process first

Alan OS should be legible in UNIX terms: files, descriptors, processes,
namespaces, mounts, credentials, access rights, and file-server services. When
Alan needs richer product concepts, they should be projected through ordinary
files or service-owned file trees first.

Alternative considered: make Object, Task, Evidence, Artifact, Semantic View,
and Agent Run Kernel primitives. That made early diagrams expressive, but it
was not sufficiently UNIX-like and would force app/agent concepts into Kernel.

### 3. Plan 9-style services

System services should be file-server Processes. A service posts a handle under
`/srv`; clients mount or bind the service tree into their namespace and use
open/read/write/stat/watch-style operations. This keeps service interaction
inside the same file/process model instead of creating a separate product API
layer.

### 3a. Standard namespace stays layered

Alan OS should keep top-level roots small: `/proc`, `/agent`, `/srv`, `/bin`,
`/lib`, `/man`, and `/mnt`. `/proc`, `/agent`, and `/srv` are live
Kernel/service views. `/bin`, `/lib`, and `/man` are command, package, and
documentation roots. `/mnt` is where service, app, memory, policy, and data
trees are mounted. Alan-specific trees such as Skills, Tool manifests, Memory
Stores, and policy packages should not become default top-level roots.

Alternative considered: define Host Service APIs as the canonical boundary.
That sounded clean but became too service-framework-shaped. In Alan OS, the
canonical boundary is the mounted file tree plus descriptors and access rights.

### 4. Service Manager replaces daemon conceptually

The long-lived lifecycle manager is Service Manager. It starts boot units,
supervises file-server services, restarts them, reaps processes, and exposes a
control surface. The current daemon-shaped code may remain as compatibility
implementation while the architecture migrates, but it should not be the target
concept.

Alternative considered: keep "alan daemon" as the central OS term. That would
preserve current code vocabulary but keep Alan anchored to HTTP/session server
semantics instead of OS lifecycle semantics.

### 5. An agent is an ordinary Process recognized by file layout

Alan Kernel should know a single `Process` category (ADR-0024 D3); there is no
separate `Agent Process` kernel type. An "Agent Process" is an ordinary Process
recognized by conforming to the agent file layout (its AgentFS surfaces under
`/agent/<pid>`), discovered by walking the process directory. The Kernel anchors
only `/proc`, descriptors, access rights, namespace, and service mounts. Agent
execution details such as model calls, tapes, skills, tools, policy, memory,
actions, and requests belong to Agent Runtime Service and AgentFS.

Alternative considered: keep Agent Capability, Agent Run, and subagent as core
process concepts. That duplicated UNIX process terminology and made the model
harder to reason about.

### 6. Agent Runtime Service owns AgentFS

Agent Runtime Service executes Agent Processes and serves `/agent`. Root Agent
Process is the root of the agent-process tree and appears at `/agent/root`.
It is not OS PID 1, not Service Manager, not Alan Agent, and not a global chat
session. Child agent work is modeled as child Agent Processes.

### 7. Apps request agent work by spawning Agent Executables

An app such as Groove Master or UPDF requests agent help by opening bounded
context, memory, skill, and policy descriptors and spawning an Agent Executable.
Shell syntax may make this ergonomic, but the system model is still spawn/open
over files and descriptors.

Alternative considered: expose an Agent Capability Service API. That made
agent work look like RPC and encouraged prompt/result contracts to become a
parallel product protocol.

### 8. Tools are commands; Skills are manual-like packages

Tools should be executable command files in `/bin` or bound into `/bin`. A Tool
has `--help`, `/man/1/<tool>`, and `/lib/exec/<tool>/manifest`. Skills are
knowledge/instruction packages installed under `/lib/skill/<name>` and surfaced
via `/man/skill/<name>`. Skills are passed to Agent Processes by descriptor;
argv or environment names are shell sugar.

Alternative considered: model Tool and Skill as Agent Capability subtypes. That
blurred external action, executable command, and instruction package.

### 9. Alan Agent is optional workspace, not the agent system

Alan Agent should be built in because it is the richest default workspace for
agent processes, but it should not be required to run agents. Alan Shell and
Alan Apps can spawn Agent Processes and inspect AgentFS directly. Alan Agent
adds rich organization, history, steering, comparison, and review views.

### 10. Current implementation is compatibility material

The current `alan-runtime` crate is the Agent Execution Engine. It can back
Agent Runtime Service, and it may later be renamed to `alan-agent-engine`. The
current HTTP/WS server, daemon-shaped modules, and CLI session paths are
compatibility transport and legacy service implementation. They remain valuable
while clients migrate, but they are not Alan OS architecture.

### 11. Local-first does not mean file-only

Alan should prefer ordinary files, directories, manifests, sidecars, caches,
imports, exports, mounts, and projections. External systems may still be source
of truth. Alan can represent their state through mounted service files or app
files without importing everything into a private object store.

### 12. WASM extensibility stays above descriptors

Rust core plus WASM Component Model and WIT interfaces remain the extension
direction. Extensions should receive explicit descriptors and access rights
rather than implicit global access.

## Risks / Trade-offs

- [Risk] A repo-level constitution can stay too abstract to guide work. ->
  Require role and file/process mapping metadata in follow-up specs.
- [Risk] "Everything is file" can become slogan-only. -> Require service, app,
  host, and agent specs to name their file trees, descriptors, and ownership
  boundaries.
- [Risk] Existing Alan specs could be invalidated too broadly. -> Classify and
  migrate specs incrementally; preserve current behavior in this change.
- [Risk] Hosts may accidentally define Kernel. -> Require host specs to name
  host-owned concerns such as rendering, layout, windowing, input, and native
  chrome.
- [Risk] Alan Agent could again become the product boundary. -> Keep Alan Agent
  as optional workspace over Agent Processes; keep Agent Runtime Service and
  Root Agent Process separate.
- [Risk] Product apps can become OS demos. -> Require Alan Apps to state their
  user-facing product boundary before their Alan adapter.
- [Risk] Compatibility transport may linger. -> Keep daemon/HTTP language
  explicitly labeled compatibility or current implementation when it appears.

## Incubation Path

The next product work should validate product assumptions rather than build a
complete Alan OS. A future incubation or MVP change should prove:

1. A first launch can reveal and organize real local work from files or an
   existing workspace.
2. One real workflow can move through files, process surfaces, mounted service
   trees, commands, host views, and agent help.
3. Alan Shell and ordinary UI can operate over the same file/process surfaces.
4. Agent work is represented as Agent Processes and leaves inspectable AgentFS
   files.
5. Tools and Skills are installed and discovered through command/manual/package
   files.
6. Extension boundaries are descriptor-shaped from the start.
7. Existing Alan subsystems used by the slice are classified as Kernel,
   service, app, host, adapter, or compatibility boundaries.

Canonical sequencing:

1. Accept this constitution as the long-lived spec baseline.
2. Build the Alan OS spine through `introduce-alan-kernel-runtime` and related
   Kernel/service specs.
3. Implement Agent Runtime Service and AgentFS over the existing Agent
   Execution Engine as a compatibility-backed file-server service.
4. Migrate Alan Agent as the optional built-in Agent Workspace over Agent
   Processes.
5. Migrate Alan Shell and Alan for macOS as hosts that consume mounted
   file/process/service surfaces rather than owning app or Kernel truth.
6. Migrate Groove Master as the first domain Alan App.
7. Migrate UPDF as a complex content Alan App after App/Host contracts are
   stable enough for document files, multi-target views, review comments, and
   publishing tasks.

## Open Questions

- Which exact service filesystem should be implemented first after AgentFS?
- How much of Plan 9's 9P protocol should Alan adopt directly versus adapting
  its file-server discipline to local Rust traits first?
- Which Alan Shell commands should prove namespace, process, and AgentFS
  operations first?
- Which WASM extension hook should be validated first?
