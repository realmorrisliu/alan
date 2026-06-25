## Context

Alan OS is moving toward a Plan 9-style model: resources and services are file
trees, each process has a namespace assembled by mount/bind operations, and
programs compose by opening files and spawning processes rather than calling
product APIs. The previous agent model still carried too much API/service
language: Agent Capability Service, Agent Runs, Context Grants, and Result
Contracts.

The new model makes agents native without creating a parallel abstraction
stack. Alan Kernel recognizes ordinary Processes and Agent Processes. Agent
Runtime Service is a file-server service that executes Agent Processes and
serves AgentFS at `/agent`. Alan Shell is the primary shell over this namespace.
Alan Agent remains built in, but only as an optional workspace UI.

## Goals / Non-Goals

**Goals:**

- Define a single `Process` Kernel category; an Agent Process is an ordinary
  Process recognized by the agent file layout, not a separate kernel category
  (ADR-0024 D3).
- Define Root Agent Process as the root of the agent process tree.
- Replace daemon as architecture concept with Service Manager and file-server
  services.
- Define the standard namespace roots: `/proc`, `/agent`, `/srv`, `/bin`,
  `/lib`, `/man`, and `/mnt`.
- Retire Agent Capability / Agent Run / Context Grant / Result Contract as core
  terminology.
- Define Tools as executables and Skills as manual-like packages.
- Preserve the Turing-machine abstraction under AgentFS machine files.
- Reframe Alan Agent as built-in but optional.

**Non-Goals:**

- Implement Service Manager, Agent Runtime Service, AgentFS, or a real Plan 9
  protocol layer in this change.
- Remove current HTTP/WS/session compatibility behavior immediately.
- Move model providers, tape schemas, memory storage, or sandbox backends into
  Alan Kernel.
- Make Root Agent Process a root permission process or global chat session.

## Decisions

### 1. Kernel process ontology stays small

Alan Kernel should distinguish a single `Process` category (ADR-0024 D3); agent,
app, service, command, task, run, and subagent are roles or relationships, not
process kinds. An agent is an ordinary Process recognized by the agent file
layout; a child agent is simply a child process that is an agent.

Alternative considered: add process kinds for app, service, command, agent run,
and subagent. That recreates product taxonomy inside Kernel and weakens the
UNIX-like process model.

### 2. An agent is an ordinary Process recognized by file layout

Superseded by ADR-0024 D3: the kernel has a single `Process` category and no
`Agent Process` type. An agent is an ordinary Process recognized by conforming to
the agent file layout (AgentFS surfaces under `/agent/<pid>`), discovered by
walking the process directory. Kernel state stays minimal (identity, parent,
credentials, descriptors, lifecycle, status, file surfaces); prompt, provider,
model, tape schema, and tool orchestration remain Agent Runtime Service concerns.

Root Agent, agent process trees, and agent policy are not second-class for being
conventions: Root Agent is a well-known process, the tree is ordinary parentage,
and policy is namespace + access rights — none needs a kernel agent type.

### 3. Service Manager replaces daemon conceptually

Alan OS still needs a lifecycle manager. It should be Service Manager, not the
legacy daemon concept. Service Manager starts boot units, restarts services,
reaps processes, and exposes its management tree by mounting the Service
Manager service under `/mnt/service`. Existing daemon code can remain as
compatibility implementation while its product name retires from target
architecture.

Alternative considered: let Root Agent Process manage services. That confuses
semantic coordination with mechanical lifecycle supervision and gives an agent
responsibility for boot and recovery.

### 4. Services are file servers

Following Plan 9, Alan OS services should be long-running Processes that export
file trees. Service handles are posted under `/srv`; useful service trees are
mounted at conventional paths. `/srv` is a rendezvous point, not a global
service state database.

Alternative considered: keep Host Service API as the canonical abstraction.
That keeps the current HTTP/server mental model and undermines the file/process
architecture.

### 5. Agent Runtime Service owns AgentFS

Agent Runtime Service executes Agent Processes and serves `/agent`. Root Agent
Process appears inside AgentFS but does not own it. This keeps the agent process
tree from becoming the runtime substrate.

Alternative considered: have Root Agent Process provide AgentFS. That would
make the resident agent both workload and runtime server, with unclear
authority and lifecycle boundaries.

### 6. Agent work starts with spawn

App-requested agent work should be `spawn` of an Agent Executable with
descriptors, not a call to an RPC-style agent API. Shell syntax may make this
pleasant, but the underlying model is executable + descriptors + process id.

Alternative considered: use an `agent.run` or Agent Capability Service API.
That creates an object/task model beside files and processes.

### 7. AgentFS separates IO from machine state

Agent IO is the external conversation/request/event surface. Agent Machine is
the Turing-machine surface: tape, state, transition events, and checkpoints.
This preserves the original Alan Turing-machine insight without forcing tape
into Kernel or user-facing transcript semantics.

Alternative considered: expose tape as the primary agent file. That leaks
runtime internals into ordinary shell/app use.

### 8. Requests and actions replace resume/tool-call APIs

Yield, confirmation, approval, structured input, and credentials are request
file trees. Tool executions and other agent-proposed effects are action file
trees. This retains governance and audit while making them inspectable and
controllable with ordinary file operations.

Alternative considered: keep resume and tool-call operations as transport
messages. That works for the current client/server shape but does not compose
with Alan Shell or Plan 9-style namespace use.

### 9. Tool means executable; Skill means manual-like knowledge

Tools live in `/bin` through bind/union and describe themselves through
`--help`, `/man/1/<tool>`, and `/lib/exec/<tool>/manifest`. Skills live under
`/lib/skill` and `/man/skill`, are passed by descriptor, and do not grant
permissions. This removes ambiguity around "capability" while keeping Alan-
specific package trees out of the top-level namespace.

Alternative considered: keep tools as JSON functions and skills as runtime
plugins. That hides actions inside agent runtime and makes reuse by different
agents harder.

### 10. Memory and policy are file trees passed by descriptor

Memory Stores and policy files are ordinary mounted file trees. Parents pass
descriptors to Agent Processes at spawn. AgentFS may project effective memory
and policy, but does not own global memory or policy registries.

When these trees are mounted for browsing or process use, Memory Stores belong
under `/mnt/mem`, policy service views belong under `/mnt/policy`, reusable
policy packages belong under `/lib/policy`, and effective per-process policy
belongs under `/agent/<pid>/policy`.

Alternative considered: keep a global agent memory registry. That would make
Root Agent Process too privileged and blur app/user/workspace ownership.

### 11. Root Agent Process has bounded default visibility

Root Agent Process gets enough descriptors to maintain continuity: system
indexes, notifications, process status, service events, public app indexes, and
system-continuity memory. It does not default to private app content, user
memory, workspace files, or other agents' machine tape.

Alternative considered: give Root Agent Process broad read access. That would
make it root automation in practice.

### 12. Alan Agent is optional workspace UI

Alan Shell must be able to operate the whole system. Alan Agent remains useful
as a built-in Agent Workspace, but it is not required to spawn, inspect, steer,
or complete agent work. It is closer to Activity Monitor, Acme, or htop for
Agent Processes than to an agent backend.

Alternative considered: keep Alan Agent as the required agent app. That binds
the runtime architecture to one UI product and weakens the OS model.

## Risks / Trade-offs

- [Risk] The Plan 9 model becomes too abstract for current implementation. ->
  Keep compatibility surfaces but label them as compatibility transport, not
  target architecture.
- [Risk] AgentFS grows into another object database. -> Require every surface to
  be a file over a real Agent Process or service file tree.
- [Risk] Root Agent Process becomes a privileged global reader. -> Default to
  index/notification descriptors and require explicit descriptors for private
  content.
- [Risk] Tool manifests become a hidden registry. -> Keep executable discovery
  through `/bin`, docs through `/man`, and manifests as metadata, not authority.
- [Risk] Skills become executable plugins. -> Keep Skills read-only/manual-like;
  actions happen through Tools and files.

## Migration Plan

1. Accept this Agent Process OS model as the target contract.
2. Update README, AGENTS, and `CONTEXT.md` to use Agent Process, AgentFS,
   Service Manager, Tool, Skill, and descriptor-passing language.
3. Align `introduce-alan-kernel-runtime` with Agent Process, `/proc`, `/agent`,
   `/srv`, and file-server service anchors.
4. Reframe `add-agent-process-kernel-types` as Kernel support for Agent
   Process and namespace/file/service anchors.
5. Reframe `add-agent-runtime-service-filesystem` as Agent Runtime Service plus
   compatibility transport over the current Agent Execution Engine.
6. Reframe `migrate-alan-agent-to-agent-workspace` so Alan Agent is built in but
   optional and Alan Shell remains the primary OS surface.
7. Preserve current session APIs only as compatibility while file/process
   surfaces reach parity.

## Follow-Up Implementation Split

1. Kernel slice: Process / Agent Process ids, descriptors, access rights,
   namespace/mounts, Files, stream Files, `/proc`, `/srv` handles, and AgentFS
   anchors.
2. Runtime service slice: Agent Runtime Service over current `alan-runtime`,
   spawn compatibility, AgentFS file projections, request/action files, and
   machine surfaces.
3. Tool/Skill package slice: executable Tool install layout, `--help`, man page,
   `/lib/exec/<tool>/manifest`, `/lib/skill`, and `/man/skill`.
4. Shell slice: Alan Shell commands for `ls /agent`, `cat /agent/root/status`,
   `tail /agent/<pid>/io/events`, spawn agent executables, and answer requests.
5. Optional workspace slice: Alan Agent as a richer Agent Workspace over the
   same files.
