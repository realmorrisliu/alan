# Alan Vocabulary

This glossary is descriptive. Normative behavior lives in OpenSpec; architectural
decisions live in `docs/adr/`.

## Product and system

**Alan** — A programmable personal computing environment.

**Alan OS** — The system boundary formed by Alan Kernel, File-Server Services,
Service Manager, Root Agent Process, Agent Runtime Service, hosts, and app
integration conventions.

**Standard Namespace** — The canonical root layout: `/proc`, `/agent`, `/srv`,
`/bin`, `/lib`, `/man`, and `/mnt`.

**Alan Kernel** — Namespace and mounts, paths, files, descriptors, access
rights, credentials, the Process table, and synthetic `/proc` and `/srv`
devices. It depends only on aP among Alan crates.

**aP** — Alan's byte-oriented file-service protocol. It carries file
operations, directory entries, clone-via-open allocation, and offset-readable
streams without embedding higher-level domain objects.

**Service Manager** — The system Process that starts, stops, restarts, and
supervises services and boot units inside one Alan OS instance. It owns system
Process lifecycle beneath the external Alan OS Host.

**Boot Unit** — A system-package-owned file describing one system Process
launch, its required descriptors and mounts, dependency ordering, restart
policy, and published handles. Boot Units are read by the Service Manager and
are not Host CLI commands or arbitrary shell scripts.
_Avoid_: Launch profile, Service script

**File-Server Service** — A long-running Process that exports a tree other
Processes can mount or bind into their namespace.

**Service Handle Registry (`/srv`)** — The rendezvous tree where a running file
server posts a mountable handle. Service state belongs in the service's own
tree, not in `/srv`.

**Alan OS System Store** — Host-provided durable backing storage partitioned by
the File-Server Services that own packages, rollouts, Memory Stores, Agent
Definitions, and service metadata. Its raw Host OS path is not Alan OS identity
or an automatically mounted user file tree.
_Avoid_: Alan home, Workspace state directory, global state file

## Execution

**Process** — A bounded execution with PID, parent, descriptors, credentials,
lifecycle, streams, status, and exit state.

**Process Launch Context** — The parent namespace snapshot plus explicit
mounts, descriptors, credentials, and initial namespace current directory used
to create a Process. It carries execution context without assigning a workspace
identity.
_Avoid_: Workspace binding, Workspace runtime

**Process Reference** — An Alan OS boot identity plus PID that names one
specific Process without becoming durable Process identity. It becomes invalid
when the Alan OS Host restarts, preventing PID reuse from attaching to a
different Process.
_Avoid_: Session ID, durable Process ID

**Shell Process** — An ordinary Process executing Alan Shell with its own Alan
OS credentials, namespace, descriptors, and current directory. Renderer input
attaches to it; commands it starts become child Processes.
_Avoid_: Host shell session, Runtime console

**Agent Process** — An ordinary Process that conforms to the agent file layout.
Its lifecycle source of truth is `/proc/<pid>` and its AgentFS view is
`/agent/<pid>`.

**Root Agent Process** — The always-available root of the agent process tree,
surfaced through `/agent/root`. It coordinates child Agent Processes.

**Agent Executable** — An executable bound into `/bin` that creates an Agent
Process when spawned.

**Agent Definition** — The file tree that supplies an Agent Process persona,
Skills, policy, model selection, and other launch knowledge. It is resolved by
Alan OS and passed at Process creation rather than discovered from a Host OS
workspace overlay.
_Avoid_: Agent CLI profile, Workspace agent overlay

**Agent Runtime Service** — The internal File-Server Service that executes
Agent Processes and serves AgentFS.

**Local Entry Service** — The system File-Server Service that creates a local
Shell Process from the Login Namespace Template and hands its namespace to an
authorized local renderer. It owns entry creation, not the child Processes
later started from that Shell.
_Avoid_: Local Session Service, Shell Manager

**Host Mount Service** — The Host-backed File-Server Service that owns Host
Mount requests, grants, hostfs exports, revocation, and projection into Process
namespaces. Alan OS records use grant identity and namespace paths while the
platform adapter retains raw Host OS paths.
_Avoid_: Workspace Registry, Path permission manager

**Connection Service** — The system File-Server Service that owns LLM
Connection profile metadata, defaults, selection, validation status, and
publication of callable connection trees. Platform credential adapters own
secrets and native login, returning only opaque credential references.
_Avoid_: Host connection registry, Inline provider credentials

**Agent Machine** — Tape and transition-local state for one Agent Process. It
is surfaced through `/agent/<pid>/machine`.

**Tape** — Ordered messages, context items, Tool records, and compaction state
consumed by the transition function.

**Turn** — One user-input transition through model generation and any resulting
Tool loop.

**Yield** — A transition pause that exposes a pending request through AgentFS.

**Checkpoint** — Durable Agent Machine evidence tied to a tape root and
execution record.

**Rollout** — Append-only durable execution evidence. A rollout has its own
identifier and records the Process path that produced it.

**Child Run Registration** — Process-local metadata describing one delegated
child Agent Process launch. Live lifecycle remains authoritative in `/proc`.

## Files, commands, and knowledge

**AgentFS** — The file server mounted at `/agent`. It exposes agent IO,
requests, actions, children, machine state, plans, notices, and streams.

**Tool** — A reusable executable installed in the command namespace.
Permissions come from descriptors, access rights, policy, and the selected
execution backend.

**Skill** — A manual-like knowledge package passed to Agent Processes. A Skill
does not execute by itself.

**Memory Stores** — File trees that own personal, system-continuity, app, and
mounted-domain memory authority.

**Working Memory** — Process-local continuity material keyed by Process path.

**Episodic Memory** — Durable summaries of prior Agent Process execution.

**Handoff** — A compact file describing current goal, completed work, open
loops, next steps, and evidence references for a later Agent Process.

**LLM Connection** — A resolved provider/model/credential-reference binding
used by an Agent Process. Secret material remains in its owning host store.

## Hosts and apps

**Alan OS Host** — The dedicated per-user, per-device, per-install-channel
process that owns one Alan OS instance, exposes its namespace attachment
surface, and shuts down the whole instance. It owns the system's external
lifecycle, while the Service Manager owns internal service and Process
lifecycle. Renderer hosts attach instead of booting their own instance.
_Avoid_: Runtime Manager, Session Host, app-owned runtime, per-window runtime

**Alan OS Attachment** — An aP client view of a ready Alan OS Standard
Namespace. It discovers Processes and services through stable paths such as
`/agent/root`, `/proc`, and `/srv`, without receiving engine-internal handles or
runtime event receivers.
_Avoid_: Session connection, Runtime handle

**Agent Attachment** — A renderer's view of one Process Reference together
with that renderer's caller-held stream offsets. It owns no Agent Machine or
Process lifecycle state and may be recreated or duplicated without creating a
new Agent Process.
_Avoid_: Agent Session, Runtime snapshot

**Host Command Plane** — The external command surface for Alan OS instance
lifecycle, attachment, Host Mount authorization, credentials, and native host
integration. It does not duplicate namespace commands or service control files.

**Alan OS Command Plane** — Alan Shell file operations, `/bin` executables, and
service-owned control files used through a Process namespace. Executing a
command creates a Process through `/proc/clone`.
_Avoid_: Manager API, typed runtime command API

**Alan Shell** — The primary file-native interaction model for namespace,
files, Processes, Agent Processes, Tools, Skills, Memory Stores, and services.
Running `alan` enters this Shell; Agent Process renderers are attachable views
within it rather than the system boot surface. The current interactive
implementation is the Rust TUI.

**Alan Renderer Host** — A renderer/input host that consumes mounted AgentFS
and `/proc` files and writes to their control surfaces.

**Alan for macOS** — The native Apple terminal host, renderer, input shell,
windowing layer, and OS integration surface. Its future Alan OS attachment is a
separate design decision governed by ADR-0029.

**Alan Agent** — An optional Agent Workspace app for inspecting, steering, and
organizing Agent Processes through files.

**Alan App** — An app with an app-owned domain core and an Alan file-server
adapter. Its UI, Tools, and Agent Processes read the same authoritative tree.

**Host-backed capability** — A File-Server Service whose adapter may call
platform frameworks, XPC helpers, device SDKs, or other host-local mechanisms
while keeping the exported file tree authoritative for Alan OS clients.

**Host Mount** — A host-authorized hostfs file tree mounted at an Alan OS path
inside a Process namespace. The raw Host OS path belongs to the host adapter and
authorization evidence, not to Agent Process identity.
_Avoid_: Workspace root, Project binding

## Current implementation names

**Agent Execution Engine (`alan-agent-engine`)** — The current model-call,
Tool, policy, Skill, memory, compaction, and persistence loop in
`crates/agent-engine`.

**Alan terminal UI (`alan-terminal-ui`)** — The linked Ratatui renderer and
input loop in `crates/tui`, backed by AgentFS and `/proc` files.

**Shell surface core (`alan-shell-core`)** — Platform-neutral spaces, tabs,
panes, terminal activity, settings, and persistence domain model shared with
Alan for macOS.
