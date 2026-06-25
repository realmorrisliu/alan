# Alan Product Context

This glossary names Alan product concepts that cut across macOS shell, runtime,
and future Alan OS work.

The canonical kernel model is [ADR-0024](docs/adr/0024-plan9-kernel-model.md)
(Plan 9 kernel) and [ADR-0025](docs/adr/0025-target-crate-architecture.md)
(crate architecture). Where this glossary and the ADRs disagree, the ADRs win.
Key consequences: the kernel has a single `Process` category (no `Agent Process`
kernel type — agent-ness is a file-layout convention); `/agent` is a view over
`/proc`; observation is a blocking read on a stream (no Subscription primitive);
`Agent Capability`, `Context Grant`, and `Result Contract` are retired.

## Language

**Alan Agent**:
The built-in optional Agent Workspace app for inspecting, steering, and
organizing Agent Processes. It is not required to run agents; Alan Shell and
other apps can operate through Alan OS files, descriptors, and syscalls.
_Avoid_: OS core, agent backend, required agent entrypoint, Root Agent

**Alan Kernel**:
The small file-tree substrate inside Alan OS: namespace and mounts, paths,
files, descriptors, access rights, credentials, processes, and the process
table.
_Avoid_: UI framework, object database, agent runtime, app backend

**File**:
A named Alan OS object exposed in the mounted namespace and opened through a
Descriptor. Files may be directories, regular data, streams, process status or
control endpoints, service endpoints, app-owned domain objects, or executable
operation files.
_Avoid_: POSIX bytes-only file, private app object copy, Kernel-owned domain record

**Resource**:
An app-, service-, or product-level name for a domain object. At the Kernel
boundary, resources are exposed as Files in the mounted namespace.
_Avoid_: separate Kernel primitive from File, private database primary key

**Process**:
A bounded execution with lifecycle, file descriptors, cancellation, exit status,
owner, and process-table identity. Processes expose status, control, input,
output, and events through Files, and they create or write ordinary Files.
_Avoid_: task as separate Kernel primitive, root session, background chatbot

**Agent Process**:
An ordinary `Process` that runs an agent, recognized by conforming to the agent
file-layout convention (it exposes `io/`, `status`, `ctl` plus `requests/`,
`actions/`, `machine/`, `context/`, `children/`, and `events`). It is NOT a
separate Kernel type — the Kernel has one `Process` category, and agent-ness is
discovered by walking the process directory, not by a Kernel flag. Lowercase
"agent process" is preferred prose for "a process that is an agent".
_Avoid_: Agent Process as a Kernel category, Agent Run, app-local chatbot, hidden
session, API task object

**Stream**:
An ordered File kind for observing process output, events, audit, file changes,
or host updates. Streams are not a separate Kernel primitive; they are opened,
read, tailed, watched, and resumed like Files.
_Avoid_: hidden callback, transport detail, standalone event bus

**Stream Offset**:
A position in a Stream File used to resume reading, tailing, replay, result
lookup, or cross-host recovery.
_Avoid_: renderer scroll position, callback token, app-local sequence only

**Service Event Stream**:
A service- or app-owned Stream File for activity, audit, recovery, replay, or
projection rebuilds. It is not a separate Kernel primitive or Kernel-owned
journal.
_Avoid_: Kernel primitive, universal audit database, object store, event system

**Namespace**:
The mounted Alan OS file tree where apps, hosts, and services expose files,
directories, streams, process endpoints, service endpoints, and executable
operation files.
_Avoid_: private app database, plugin registry, object store

**Standard Namespace**:
The canonical Alan OS root layout. Top-level roots are kept small and stable:
`/proc`, `/agent`, `/srv`, `/bin`, `/lib`, `/man`, and `/mnt`. Alan-specific
packages belong under `/lib`; mounted service, app, and data trees belong under
`/mnt`.
_Avoid_: product-specific top-level roots, global registry sprawl, `/skill`,
`/mem`, `/policy`, `/service` as default roots

**Path**:
A namespace-qualified file name. Prefer Path when naming durable Alan Kernel
identity.
_Avoid_: global object id, private database primary key, universal URI

**Mount**:
A binding that exposes an app, host, service, or native system file tree inside
the Alan OS Namespace.
_Avoid_: plugin registry entry, object collection, app-private folder

**File Server**:
A long-running Process that exports a file tree which other processes can mount
or bind into their Namespace. Alan OS services are file servers, not HTTP APIs.
_Avoid_: REST service, hidden singleton backend, object API

**Service Manager**:
The Alan OS system Process responsible for starting, stopping, restarting, and
supervising system services and boot units. It replaces the former daemon as the
canonical lifecycle concept and exposes its management view as files.
_Avoid_: Alan daemon as architecture concept, Root Agent, app backend

**Service Handle Registry**:
The `/srv` file tree where running file servers post handles that other
processes can mount. `/srv` is a rendezvous point, not the service state tree.
_Avoid_: service database, REST endpoint registry, app launcher

**Process Table**:
The Alan Kernel view of bounded Processes, their lifecycle, descriptors,
parents, owners, credentials, and cancellation or signal targets.
_Avoid_: task database, chat session list, app-owned job queue

**Descriptor**:
A typed opaque authority-bearing reference returned by opening a File or
Process endpoint, paired with Access Rights and any relevant offset state. It is
the Alan Kernel analogue of a file descriptor.
_Avoid_: durable path identity, object database key, capability descriptor

**Access Rights**:
The access modes attached to a Descriptor, such as read, write, watch, spawn,
or signal.
_Avoid_: agent risk score, app-domain policy, product confirmation state

**Runtime Reference**:
A temporary in-memory or compatibility reference used by projections, caches, or
running state. It is not the canonical Path or authority-bearing Descriptor.
_Avoid_: durable path identity, native authority, object database key

**Operation Surface**:
An umbrella term for service/app descriptors exposed through the file tree.
Commands, queries, and subscriptions stay above Kernel while referencing paths,
files, processes, descriptors, and namespaces.
_Avoid_: Kernel primitive, app callback registry, code module boundary

**Command**:
A typed executable operation surface that requests a side effect or spawns a
Process under Descriptor/Access Rights checks and any relevant service or app
governance.
_Avoid_: view callback, tool synonym, Kernel primitive above File

**Query**:
A typed read-only operation surface for inspecting files, processes, streams, or
snapshots through file-tree semantics.
_Avoid_: mutation path, hidden recomputation hook, database query only

**Subscription**:
Retired as a concept. Watching is a blocking read on an `events`/`log` stream
file (`tail -f` semantics): the read blocks until new records arrive. There is no
Subscription primitive, object, or registry.
_Avoid_: Subscription as a primitive, separate event system, hidden callback,
mutable view state

**Agent Runtime Service**:
A system file-server Process managed by Service Manager. It executes Agent
Processes, serves AgentFS, and backs agent executables without exposing a
product-facing HTTP API.
_Avoid_: Agent Capability Service API, app backend, Root Agent

**AgentFS**:
The `/agent` file tree served by Agent Runtime Service. It is a view over
`/proc` (a union/bind of agent-conforming process directories), not a second
process table — `/proc/<pid>` is the source of truth. `/agent/root` is the
stable alias to whichever pid currently embodies the Root Agent's home, and
`/agent/<pid>` exposes the agent file-layout surfaces.
_Avoid_: second process table, executable catalog, agent registry, chat history
database

**Agent Executable**:
An executable file that creates an Agent Process when spawned. Agent
Executables are discovered through the normal command namespace, usually `/bin`,
after bind/union mounting; they are not invoked through an agent API.
_Avoid_: Agent Capability Descriptor, API method, app-local prompt

**Tool**:
A reusable executable installed in the Alan OS command namespace. Agent
Processes use Tools by spawning executables, passing descriptors, reading
stdout/stderr/result files, and waiting for exit status.
_Avoid_: JSON function, runtime-private callback, skill

**Tool Manifest**:
A machine-readable file describing a Tool's argv, stdin/stdout/result
conventions, required descriptors, effect class, exit status, and sandbox hints.
It is stored under `/lib/exec/<name>/manifest` and complements the Tool's
`--help` output and manual page.
_Avoid_: hidden registry entry, agent-only schema, permission source

**Skill**:
A manual-like knowledge package installed under `/lib/skill/<name>` and
documented under `/man/skill/<name>`. Skills are read by Agent Processes through
descriptors and may explain tools, workflows, examples, constraints, and domain
procedures; they do not execute.
_Avoid_: Tool, executable, permission grant, hidden prompt injection

**Agent Context**:
The files bound into an agent's namespace (notably under `context/`, plus the
`/bin` tools and Skills it can see) at spawn time, such as a target file,
selection, Skill directory, Memory Store, or policy file. The model request is
assembled as a view over these namespace files; changing context means changing
the namespace, not calling a grant API.
_Avoid_: Context Grant API, descriptor-passing as the canonical model, prompt
dump, implicit global access

**Credential**:
The Kernel identity and authority context used for access checks, such as a
user, app, service, process, or agent actor credential.
_Avoid_: product persona, chat participant, app-local user profile

**Access Check**:
The Kernel/OS authority check for whether a Credential can obtain a Descriptor
for a path, file, process endpoint, stream, or mounted service with Access
Rights such as read, write, watch, spawn, or signal.
_Avoid_: agent risk scoring, product confirmation flow, app-domain policy,
capability descriptor

**Consent Broker**:
A Host Service or OS service that asks for and records user/system consent for
resource access, such as files, microphone, automation, workspace
writes, or cross-app access.
_Avoid_: Kernel primitive, agent action risk model, app-specific modal only

**Agent Action Governance**:
The agent runtime decision layer that evaluates whether an agent-proposed or
autonomous action can run, must ask, or must be denied.
_Avoid_: universal OS command governance, UNIX permission check, app-local delete button policy

**Agent Action Effect Class**:
A semantic classification of an agent-proposed action's effect, such as inspect,
draft, modify, delete, publish, execute, delegate, remember, or cross-app, used
by Agent Action Governance.
_Avoid_: read/write only, tool capability as full risk model

**Agent Action Risk**:
The governance assessment of whether an agent-proposed action can run
automatically, must ask for approval, or must be denied, based on policy, effect
class, target scope, reversibility, guard strength, and auditability.
_Avoid_: write means unsafe, read means safe

**Agent Execution Guard**:
The containment, validation, or approval mechanism used to constrain
agent-proposed actions, such as an OS sandbox, workspace path guard, app object
guard, domain validator, or human approval gate.
_Avoid_: policy alone, sandbox as the only guard

**Agent Action**:
A specific external effect proposed or initiated by an Agent Process, such as
spawning a Tool, writing a File, editing a resource, requesting consent, or
issuing an app command. Actions are exposed under AgentFS and are not Tools
themselves.
_Avoid_: Tool definition, generic command history, Kernel primitive

**Agent Request**:
A file-tree interaction where an Agent Process asks for confirmation,
structured input, selection, credentials, or another external answer. Requests
are answered by writing response files, not by calling a resume API.
_Avoid_: private callback, HTTP resume operation, modal-only UI

**Agent IO**:
The external input, output, and event surface of an Agent Process. Agent IO is
what shells, apps, and users consume by default; it is distinct from machine
tape.
_Avoid_: tape, full runtime trace, debug log

**Agent Machine**:
The Turing-machine view of an Agent Process exposed by Agent Runtime Service:
tape, machine state, transition events, and checkpoints. This is an AgentFS
surface, not Alan Kernel ontology.
_Avoid_: Kernel state, user-facing transcript, session API

**Object**:
A typed Alan surface for an inspectable File or app domain resource. Prefer
File when naming Alan Kernel ontology.
_Avoid_: Kernel primitive above File, private object store, durable object id

**Task**:
A user-facing or app-facing name for work in progress. Prefer Process when
naming Alan Kernel execution semantics.
_Avoid_: separate Kernel primitive from Process, todo item as execution authority,
task database

**Artifact**:
A service/app-facing presentation or compatibility surface over an ordinary
File produced by a Process.
_Avoid_: Kernel primitive, separate artifact database, durable Kernel artifact id

**Evidence**:
An Agent/App-facing interpretation of paths, stream offsets, process ids,
descriptors, app artifact paths, or native selectors as support for a claim,
result, memory, command proposal, or decision.
_Avoid_: Kernel primitive, ProvenanceRef, evidence database

**Semantic View**:
A host-facing projection or rendering hint over file, process, and stream-file
state. Business truth remains in the underlying Files and Processes.
_Avoid_: UI framework, app state authority, renderer-owned truth

**Agent Memory Kind**:
The agent-cognitive classification of memory as working, episodic, semantic, or
procedural. This describes how an agent uses memory, not who owns it.
_Avoid_: user/app/system ownership bucket, storage location, permission model

**Working Memory**:
Session-local agent memory needed to continue the current task.
_Avoid_: durable user preference, cross-app continuity store

**Episodic Memory**:
Agent memory about what happened in past sessions, handoffs, daily notes, or
run histories.
_Avoid_: stable fact store, behavior rule

**Semantic Memory**:
Stable agent memory about durable facts, preferences, constraints, conventions,
or decisions.
_Avoid_: raw transcript, chronological log, current TODO

**Procedural Memory**:
Behavioral memory expressed through prompts, persona files, skills, or other
rules for how an agent should act.
_Avoid_: user identity fact, session summary, app-owned history

**Memory Store**:
An ownership and authority boundary for memory files. Memory Stores expose
memory as file trees that Agent Processes can access only through Access
Checks, Descriptors, or app-controlled surfaces. When a Memory Store is mounted
for browsing or process use, the conventional mount location is under
`/mnt/mem`.
_Avoid_: memory kind, global agent brain, Kernel primitive

**Personal Memory Store**:
The Memory Store for user-owned preferences, habits, goals, identity, and
stable constraints.
_Avoid_: app history, workspace status, raw transcript dump

**System Continuity Store**:
The Memory Store for Alan OS continuity across apps, active work, and
relationships between runs or tasks.
_Avoid_: app-owned private memory, global scrape, root agent session

**App Memory Store**:
An Alan App-owned Memory Store for domain memory such as reading history,
practice logs, project notes, or app-specific evidence.
_Avoid_: automatic Root Agent memory, global agent memory

**Workspace Memory Store**:
The Memory Store for workspace-scoped agent work, decisions, conventions,
handoffs, and project continuity.
_Avoid_: personal identity memory, app-private history, Kernel memory system

**Agent Execution Engine**:
The current internal implementation of the Agent Runtime Service concept: tape,
machine loop, model calls, tool execution compatibility, skills, policy, memory,
and persistence. It is not Alan Kernel.
_Avoid_: Alan OS, Alan Kernel, app UI, daemon

**Root Agent**:
The always-available Agent Process at the root of the agent process tree, with
long-lived identity, memory, system awareness, and cross-app continuity. It can
coordinate child Agent Processes but is not the Service Manager, root
permission, or an ever-growing chat session.
_Avoid_: root permission, root agent session, agent kernel, global chat

**Root Agent Process**:
The concrete Agent Process exposed through `/agent/root`. It is launched by
Service Manager as a boot unit and appears in both `/proc` and `/agent`.
_Avoid_: Service Manager, daemon, global conversation

**Agent Workspace**:
An optional user-visible workspace, such as Alan Agent, where users inspect,
steer, and organize Agent Processes, requests, actions, memory, evidence, and
cross-app work.
_Avoid_: Root Agent, required agent runtime, root session

**Agent Process Migration**:
The migration of existing Alan Agent capabilities into Alan OS by preserving,
adapting, or rewriting them as Kernel primitives, system file-server behavior,
AgentFS surfaces, Tools, Skills, policy descriptors, compatibility behavior, or
optional workspace UI.
_Avoid_: Agent Capability model, greenfield replacement, copying runtime internals into Kernel

**Root Agent Authority**:
The authority model for the Root Agent: broad system awareness and
suggestion power through default system index, notification, and continuity
descriptors, with app-private reads and side effects mediated through explicit
Descriptors, consent, policy, and audit paths.
_Avoid_: root automation permission, unrestricted agent access

**Generation**:
One LLM call — a single evaluation of the agent's transition function. A
Generation is modeled as a connection directory under an LLM Connection: the
caller writes one complete, neutral request document, then reads a typed token
stream. Generations are visible as files so their progress and cost can be
inspected.
_Avoid_: hidden fd-only session, provider-specific request as the canonical
shape, a generation that cannot be observed as files

**LLM Provider**:
A wire adapter (driver) for one model API — Anthropic, OpenAI Responses, etc.
Served read-only for introspection at `/srv/llm/<provider>`. A Provider knows the
protocol but holds no Credential and no default Model, so it is not callable on
its own.
_Avoid_: provider as a callable endpoint, provider that embeds credentials

**LLM Connection**:
A callable endpoint that binds a Provider, a Model, and a Credential together,
served by `llmfs` at `/srv/llm/<connection>`. Generations happen here. An agent
gains model access only by binding a Connection into its namespace; changing the
model means binding a different Connection. Credentials stay in the Connection /
secret store and never enter the request document or the agent's namespace as
plaintext. Cost, metering, and rate-limiting live here, not in a global quota
service.
_Avoid_: credentials in the request, a global model-quota service, ambient model
access outside the namespace, conflating Provider with Connection

**Alan Shell**:
The primary shell for Alan OS: a Plan 9 `rc`-like and Acme-like interaction
surface for using the Namespace, files, processes, Agent Processes, Tools,
Skills, Memory Stores, and system services. It may be rendered by the current
Ratatui path in `crates/tui` and by Alan for macOS.
_Avoid_: Alan TUI as product name, daemon client, terminal product naming, chat UI

**Primary Shell Window**:
The single main Alan shell window used by the macOS app. Short-term product
work assumes there is only one shell window, and summon behavior targets this
window.
_Avoid_: recent shell window, per-Space shell window, Quick Terminal window

**Primary Window Summon**:
The user action that brings Alan's primary shell window to the user's current
macOS Space and display. It targets the main Alan window, not a detached
terminal panel or separate terminal runtime, and it preserves the current Alan
workspace Space, Tab, and Pane selection. Alan comes to the user's current
desktop context rather than moving the user to Alan's previous desktop context.
It replaces the former Quick Terminal shortcut without keeping Quick Terminal
compatibility aliases. It is an app/window command, not a shell workspace
action.
_Avoid_: Quick Terminal summon, Peak summon, global terminal toggle, quick-terminal alias
