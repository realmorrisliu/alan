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
supervises services and boot units.

**File-Server Service** — A long-running Process that exports a tree other
Processes can mount or bind into their namespace.

**Service Handle Registry (`/srv`)** — The rendezvous tree where a running file
server posts a mountable handle. Service state belongs in the service's own
tree, not in `/srv`.

## Execution

**Process** — A bounded execution with PID, parent, descriptors, credentials,
lifecycle, streams, status, and exit state.

**Agent Process** — An ordinary Process that conforms to the agent file layout.
Its lifecycle source of truth is `/proc/<pid>` and its AgentFS view is
`/agent/<pid>`.

**Root Agent Process** — The always-available root of the agent process tree,
surfaced through `/agent/root`. It coordinates child Agent Processes.

**Agent Executable** — An executable bound into `/bin` that creates an Agent
Process when spawned.

**Agent Runtime Service** — The internal File-Server Service that executes
Agent Processes and serves AgentFS.

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
workspace memory authority.

**Working Memory** — Process-local continuity material keyed by Process path.

**Episodic Memory** — Durable summaries of prior Agent Process execution.

**Handoff** — A compact file describing current goal, completed work, open
loops, next steps, and evidence references for a later Agent Process.

**LLM Connection** — A resolved provider/model/credential-reference binding
used by an Agent Process. Secret material remains in its owning host store.

## Hosts and apps

**Alan Shell** — The primary file-native interaction model for namespace,
files, Processes, Agent Processes, Tools, Skills, Memory Stores, and services.
The current interactive implementation is the Rust TUI.

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

## Current implementation names

**Agent Execution Engine (`alan-agent-engine`)** — The current model-call,
Tool, policy, Skill, memory, compaction, and persistence loop in
`crates/agent-engine`.

**Alan terminal UI (`alan-terminal-ui`)** — The linked Ratatui renderer and
input loop in `crates/tui`, backed by AgentFS and `/proc` files.

**Shell workspace core (`alan-shell-core`)** — Platform-neutral spaces, tabs,
panes, terminal activity, settings, and persistence domain model shared with
Alan for macOS.
