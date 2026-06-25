# Plan 9 Kernel Model (Consolidated)

Status: Accepted. Supersedes the "Agent Process as a Kernel category" framing and
tightens kernel identity. This is the durable anchor for the new `alan-kernel`
spec, the Agent file-layout contract spec, and the deprecation of the older
kernel-contract specs.

## Context

The kernel design had drifted into two incompatible worldviews living side by
side: an older "semantic runtime" ontology (Object / Buffer / View / Command /
Query / Subscription / Task / Artifact / Evidence / Journal / ViewModel, plus a
first-class Agent Process Kernel category) and a newer semantic-UNIX direction
(namespace / file / process / descriptor). Both were called "Alan Kernel." This
record reconciles them onto a single Plan 9-inspired model and shrinks the
durable vocabulary to roughly: file, directory, stream, namespace, mount, bind,
union, fid, process, `/proc`, `/srv`, ctl.

The model is derived from one load-bearing reframing: **an LLM is a typed stream
a process reads; the agent is the process that consumes that stream and turns
its content into effects governed by its own namespace.** Everything below
follows from taking that seriously.

## The coherent model

A process assembles a request from files in its own namespace, opens an LLM
provider stream, reads typed records off it, and — under the constraints of that
same namespace — spawns processes and writes files as effects. The LLM has no
inherent authority; it only emits suggestions into a stream. "Agent" is this
pattern, not a kernel type. Persistent agent identity is a durable home file
tree bound into an otherwise ephemeral process. The kernel itself knows nothing
about agents, LLMs, tape, memory, or providers; it provides only the namespace
engine, the process table, and the `/proc` and `/srv` synthetic devices.

## Decisions

### D1. LLM output is a typed stream; effects are defined by the consumer

The transition function is reading a stream, not calling a tool API. A tool call
is just content in the LLM stream that the consuming process may choose to turn
into a real effect under its namespace and policy. Governance lives in the
process and its namespace, never in the provider. (Closes the "tool call as a
privileged API" model.)

### D2. The request is assembled from the namespace; wire format is provider-local

The logical request to the model is a view over namespace files — the machine
tape, context files, and the `/bin` tools visible in the namespace. The provider
file server translates that provider-agnostic structure into OpenAI / Anthropic /
OpenRouter wire formats. Consequences: changing context = changing the namespace;
changing provider = rebinding the mount; an agent's available tools = the `/bin`
contents visible in its namespace. Tape compaction is a *view/projection* over
`machine/tape` (`machine/tape` is truth; the context-window view is what gets
sent), not a special runtime step.

### D3. The kernel has only one process category

The kernel models `Process`. There is no `Agent Process` kernel type. "Agent",
"subagent", "root agent", "service" are roles/patterns observable at the file
and namespace layer, not kernel categories. Whether a process is an agent is
answered by whether its directory conforms to the agent file-layout convention
(see D4), i.e. filesystem duck typing. (Supersedes the framing of ADR-0004 and
the `add-agent-process-kernel-types` change.)

### D4. Uniform operation comes from convention + `ctl`, not from types

Tools operate agents uniformly for two reasons: the file protocol is mechanically
uniform (walk/open/read/write/stat apply to every file), and a *published
file-layout contract* gives semantic uniformity. An agent is a strict superset
of the generic process file layout: every process exposes the full `/proc/<pid>`
generic layout — identity, parentage, credentials, namespace, exit state, plus
the `io/`, `status`, and `ctl` IO/control subset; an agent additionally exposes
`requests/`, `actions/`,
`machine/`, `context/`, `children/`, and a top-level aggregate `events` stream.
Control is expressed by writing text commands to `ctl` (Plan 9 `/net` style), so
new control actions never require new files. `/agent` is an *overlay* over
`/proc` (for each agent-conforming process it unions the kernel `/proc/<pid>`
generic layout with the agent runtime's agent surfaces, plus friendly aliases
such as `/agent/root`), not a second process table; the kernel renders no
agent-specific files in `/proc`.

### D5. The file-server contract is wire-shaped; v1 transport is in-process

The kernel file-server contract is defined as if it crosses a process boundary —
fid-based, byte/offset-oriented, error-coded, carrying nothing that only means
something in-process (no Rust borrows, no rich typed return values). v1
implements built-in file servers over an in-process fast path (no serialization,
so LLM token streams pay no protocol tax). A 9P-like wire transport for
out-of-process / networked / third-party file servers is a later slice. Typed
event streams are a byte-stream record convention (e.g. one JSON record per
line), not a kernel type. (Tightens ADR-0014/0017; this discipline is what keeps
the model from collapsing into an in-process object system.)

### D6. The per-process namespace is the sole capability boundary

Isolation is achieved by constructing a namespace, not by layering policy. A
child's namespace is given by its spawner (spawn is where the boundary is set);
the child may further restrict its own view but cannot acquire a channel to a
file server it was not granted. There is **no global ambient addressing**: a
resource is reachable iff it is in the namespace (or dialable through a server
already in the namespace). `/srv` is **not** an exception: posted handles carry
access rights and a process's `/srv` view is filtered to what it may mount, so a
withheld service cannot be remounted via `/srv` — otherwise `/srv` would be the
ambient backdoor that defeats denial-by-absent-mount. Opaque ids may
exist but MUST resolve within a namespace and MUST NOT act as a global capability
that bypasses it. Consequences: denying a sub-agent model access = not binding an
llmfs Connection (`/mnt/llm/connections/<connection>`) into its namespace —
`/srv/llm` is only
the rendezvous handle, not the authority; cost/metering/rate-limiting lives in the
provider file server (the
chokepoint), not a global policy engine; cross-agent collaboration is an
explicitly shared mount point, never implicit access by pid. (Amends ADR-0016 by
closing the opaque-id backdoor.)

### D7. The kernel is ephemeral; persistence lives in file servers

The kernel persists nothing: process table, namespaces, and fids are runtime
state, and a restart yields a clean kernel. Durability is a property of a file
server backed by storage. An agent's persistent identity is a durable home tree
(config, memory, tape, machine state) owned by such a server; "running an agent"
is a process with that home bound into its namespace; restart continuity is a new
process re-binding the same home. The pid is ephemeral — durable identity is the
home path, and stable names like `/agent/root` resolve to whichever pid currently
embodies the home. Whether an agent is durable or ephemeral is decided by where
its home is mounted (tmpfs home → ephemeral; disk-backed home → durable), not by
any agent type. (Absorbs Workspace / AgentInstance / Session into home tree +
process + process lifetime. Refines ADR-0002.)

### D8. Observation is a blocking read on an events stream

There is no second event system. To observe anything, a consumer opens and reads
an `events`/`log` stream and blocks until new records arrive; `tail -f` is
literally "watch". Any container with dynamic children (e.g. `requests/`) exposes
a sibling events stream where child add/remove/change records appear. These
streams are bounded-retention append logs with offset resume so a reconnecting
watcher neither misses nor mis-replays. This eliminates Subscription as a
primitive (subscription = a blocking read). (Makes ADR-0017 concrete.)

### D9. The kernel provides only namespace, `/proc`, and `/srv`

To break the bootstrap paradox, exactly three things are kernel-synthesized:
(1) the namespace engine (mount/bind/union/unmount/walk/open/read/write/stat/
clunk + fid); (2) `/proc`, the process table rendered as files, because it *is*
the kernel's own state; (3) `/srv`, the rendezvous device where file servers post
mountable handles, because it must exist before any user server can. Everything
else — the agent runtime (`/agent`), the LLM provider (`/mnt/llm`; handle at
`/srv/llm`), memory (`/mnt/mem`), tools (`/bin`) — is a user-space file server the
kernel knows
nothing about. The root namespace is assembled by init / Service Manager at boot,
not hardcoded. Therefore `alan-kernel` (the crate) = namespace engine + fid/
protocol contract + process table + `/proc` + `/srv`, with no dependency on
agent, llm, provider, tape, memory, or runtime — the crate that should change
least.

## Relationship to existing ADRs

- Builds on and keeps: 0005, 0014, 0017, 0018, 0019, 0020, 0023 (push
  non-primitives above the kernel; streams are file kinds; no kernel journal).
- Refines: 0002 (Root Agent continuity is now a durable home + restart policy),
  0008 (Agent Runtime Service is just one user-space file server among many).
- Amends: 0016 — opaque ids must resolve within a namespace and are never a
  global capability.
- Supersedes: the "Agent Process is a first-class Kernel category" framing in
  0004 and in the `add-agent-process-kernel-types` change. Agent-ness is a
  file-layout convention (D3/D4), not a kernel type.

## Risks

- **R1 (highest): in v1 the capability boundary is convention-enforced, not
  isolation-enforced.** D6's guarantee holds only if a process cannot forge a
  channel. D5's v1 runs all file servers in one address space, so namespace
  isolation is not hardware-enforced and a clever or buggy in-process tool could
  reach across. Plan 9's namespace security also assumed *cooperative* users,
  while an LLM is semi-adversarial. The real enforcement depends on the later
  cross-process / isolation transport slice (same line as D5). This must be
  stated wherever the security model is claimed; do not let it stay silent.
- **R2: async observation (D8) is Plan 9's known weak spot, not a strength.**
  One held blocking read per watcher is cheap in-process but costs a held
  connection per watcher over the wire, and high fan-out (hundreds of agents
  watching each other) may strain it. The decision is to ship the pure Plan 9
  model first and revisit only if it breaks — and crucially to keep "watch = read
  on an events stream" as the *semantic* so any future fix is a transport
  optimization under the same semantic, never a new event-system ontology.

## Surviving vocabulary

file · directory · stream · namespace · mount · bind · union · fid · process ·
`/proc` · `/srv` · ctl. Retired as kernel concepts: Agent Process (type), Object,
Buffer, View, Command, Query, Subscription, Task, Artifact, Evidence, Journal,
ViewModel, Workspace, AgentInstance, Session, Context Grant, Result Contract,
global opaque id.
