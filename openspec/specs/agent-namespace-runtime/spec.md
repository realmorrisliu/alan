# agent-namespace-runtime Specification

## Purpose
Defines the namespace-native Agent Execution Engine transition-loop boundary:
model generation, Tool execution, AgentFS state, Process spawn, and shell
conversation all flow through mounted files rather than injected providers or
side-channel state.
## Requirements
### Requirement: The engine's environment is its namespace
The Agent Execution Engine SHALL store and use exactly one environment value: a
namespace handle (an aP client over its mounted root). It SHALL NOT wrap that
handle in a multi-environment abstraction or take an injected LLM provider,
in-process Tool registry, event-emission sink, or runtime broadcast channel as
its environment. Every capability the engine exercises SHALL be reached by
walking a path in that namespace, so an Agent Process capability set is exactly
what its spawner mounted.

#### Scenario: The engine is constructed
- **WHEN** the engine loop is constructed for an Agent Process
- **THEN** it receives and stores a concrete namespace handle and no
  `LlmProvider`, `ToolRegistry`, event sink, or broadcast sender
- **AND** anything not present in that namespace is unreachable to the agent

#### Scenario: A capability is withheld
- **WHEN** a model or Tool must be denied to an Agent Process
- **THEN** it is not mounted into that Process namespace
- **AND** no separate injected-capability check or hidden registry is required
  to enforce the denial

#### Scenario: Runtime environment types are inspected
- **WHEN** repository validation inspects the engine live path
- **THEN** no single-variant compatibility wrapper or alternate non-namespace
  runtime environment remains

### Requirement: Generation is a namespace file operation
The engine's transition function SHALL be performed as file operations on the
mounted LLM connection: open `/mnt/llm/connections/<conn>/clone` (clone-via-open),
write the request document to `data` (committed on clunk), and read the token
stream from `events`. The engine SHALL NOT call an `LlmProvider` trait method to
generate. The `events` records are the source of truth for the model's output.

#### Scenario: The engine runs a turn
- **WHEN** the engine assembles a request and needs a model response
- **THEN** it opens the connection's `clone`, writes the request to `data`, and
  reads token records from `events`
- **AND** it does not invoke any `LlmProvider`/`generate_stream` call path

#### Scenario: A model is not mounted
- **WHEN** the agent's namespace has no `/mnt/llm/connections/<conn>`
- **THEN** the generation step fails to resolve the path
- **AND** the engine cannot reach a model by any other means

### Requirement: Tools are executables invoked through the process namespace
The engine SHALL discover an executable model-callable Tool only when a visible
executable at `/bin/<tool>` has a valid mounted manifest at
`/lib/exec/<tool>/manifest`. It SHALL derive the Tool's model definition,
capability classification, timeout, and execution hints from that package and
invoke the Tool by spawning its executable via `/proc/clone`, then reading the
Tool Process output/result files and projecting them into
`actions/<id>/`. The transition loop SHALL NOT use or reconstruct an in-process
Tool registry as Tool visibility, description, or execution authority. During
the current convention-enforced stage, host composition and Process-runner
adapters MAY use in-process implementation registries to materialize and host
mounted Tool packages, but they SHALL NOT make an unmounted Tool visible or
executable and every invocation SHALL still cross `/proc/clone`.
Runtime-owned interaction, governance, plan, and delegation controls MAY expose
model-callable operation schemas without `/bin` packages, but they SHALL be
handled as transition-local control operations that write their defined files
or namespace state and SHALL NOT provide arbitrary executable dispatch.

#### Scenario: The engine calls a Tool
- **WHEN** a turn requires a Tool effect and the complete Tool package is
  mounted
- **THEN** the engine resolves `/bin/<tool>`, spawns it via `/proc/clone`, and
  reads its Process output/result files
- **AND** `actions/<id>` references that concrete Tool Process
- **AND** the transition loop does not call an in-process Tool implementation
  directly; any in-process hosting remains behind the Process runner adapter

#### Scenario: A Tool is not mounted
- **WHEN** the Tool executable is not bound into the Agent Process namespace
- **THEN** it is absent from the model's Tool definitions and spawn cannot
  resolve it
- **AND** the agent cannot perform that effect

#### Scenario: Executable has no valid Tool manifest
- **WHEN** `/bin/<name>` is visible but its Tool manifest is missing or invalid
- **THEN** the command is not exposed as a model-callable Tool
- **AND** an engine-private catalog cannot supply the missing definition

#### Scenario: Manifest has no visible executable
- **WHEN** `/lib/exec/<tool>/manifest` exists but `/bin/<tool>` is absent from
  the Process namespace
- **THEN** the manifest grants no Tool visibility or execution authority

#### Scenario: Model calls a runtime control operation
- **WHEN** the model calls a runtime-provided request, mount, plan, or
  delegation control
- **THEN** the transition loop handles the defined control operation and writes
  its owning AgentFS, namespace, or machine-control surface
- **AND** the absence of a `/bin` Tool package does not turn the control into an
  executable Tool or permit arbitrary Process spawn

### Requirement: Agent state is written to the agent's files
The engine SHALL have each state owner write directly to the Agent Process files:
assistant output to `io/output`, tape state to `machine/tape`, yields to
`requests/<id>/`, Tool calls to `actions/<id>/`, and renderer-approved state to
`machine/ui/`. These files and their owned streams SHALL be the source of truth.
The engine SHALL NOT publish live state through `EventEnvelope`,
`RuntimeEventEnvelope`, a broadcast sender, or a generic event-to-file
projector.

#### Scenario: The engine produces output
- **WHEN** the engine produces assistant text, a yield, or a Tool call
- **THEN** the owning runtime component appends `io/output`, creates a
  `requests/<id>/` tree, or creates an `actions/<id>/` tree respectively
- **AND** those files are not derived later from a live runtime broadcast

#### Scenario: Renderer-visible state changes
- **WHEN** activity, plan, renderer-visible thinking, or notice state changes
- **THEN** the owning component updates the corresponding `machine/ui/` snapshot
  and appends its file-owned update record
- **AND** no generic runtime event projector mediates the write

#### Scenario: A consumer observes the agent
- **WHEN** a client wants Agent Process output or state
- **THEN** it reads or watches the Agent Process files and streams
- **AND** it does not subscribe to an engine-owned live event channel

#### Scenario: Event and Op types remain in source
- **WHEN** a semantic Event or Op type is retained after this change
- **THEN** every live use is a file-record schema or transition-local value
- **AND** the type does not carry a publish/subscribe runtime transport

### Requirement: An agent is a process with a spawner-assembled namespace
An agent SHALL run as an ordinary `Process` created via `/proc/clone` whose exec
spec assembles the child's namespace. The mounted set — the LLM connection, the
tool executables, the agent's own `/agent/<pid>` tree — SHALL be the agent's
entire capability set. There SHALL be no capability granted to an agent outside
its namespace.

Implementation evidence for this change SHALL state the ADR-0024 R1 boundary:
until the kernel §7.1a amplification check and cross-process/isolation transport
land, this capability boundary is convention-enforced in one address space, not
hard isolation. Do not claim security isolation from absent mounts until that
later enforcement slice is present.

#### Scenario: An agent is spawned
- **WHEN** an agent process is created
- **THEN** it is spawned via `/proc/clone` with an exec spec that mounts its LLM
  connection, tools, and agent tree
- **AND** the agent can do exactly what those mounts permit, and nothing else

#### Scenario: The agent tree is observed
- **WHEN** a client walks `/agent/<pid>` or `/agent/root`
- **THEN** the entry resolves only if the corresponding `/proc/<pid>` process
  exists and has an agent-state backing tree
- **AND** `/agent/root` is an alias for the Root Agent Process pid, not a
  separate state tree
- **AND** generic process files remain under `/proc/<pid>` while agent runtime
  files remain under `/agent/<pid>`

#### Scenario: Generic process control is applied
- **WHEN** a client interrupts or cancels an Agent Process
- **THEN** the generic lifecycle command is written to `/proc/<pid>/ctl`
- **AND** `machine/ctl` remains reserved for agent-runtime tape/checkpoint
  commands such as `compact` and `rollback`
- **AND** the kernel interprets no agent-runtime control semantics

#### Scenario: A sub-agent is given a narrower world
- **WHEN** a parent spawns a sub-agent with fewer mounts (e.g. no model, or a
  restricted tool set)
- **THEN** the sub-agent's namespace omits those trees
- **AND** the sub-agent cannot reach the withheld capabilities by any path

### Requirement: M2 — a real conversation flows entirely through files
The shell-to-agent conversation SHALL be expressible end-to-end as file
operations with no RPC and no provider injection: the shell writes a message to
the agent's `io/input`; the agent reads it, generates by reading `/mnt/llm`, and
writes `io/output`; the shell tails `io/output`.

#### Scenario: The shell talks to a real agent
- **WHEN** the shell writes a user message to a spawned agent's `io/input` and
  tails its `io/output`
- **THEN** the agent reads the input, generates via its mounted LLM connection,
  and the model's response appears on `io/output`
- **AND** no operation outside aP file IO is used to carry the conversation

### Requirement: Runtime context is Process-shaped
Agent Execution Engine SHALL derive file reachability, cwd, Tool execution,
Agent Definition, Skills, policy, memory handles, and durable evidence
references from the Agent Process namespace and descriptors. It MUST NOT own a
workspace identity, workspace root, or Host `.alan` directory.

#### Scenario: Runtime prepares a turn
- **WHEN** an Agent Process begins a transition
- **THEN** every contextual resource is read from a mounted path or descriptor
- **AND** no Host-directory overlay scan occurs

### Requirement: System composition belongs to Alan OS Host
Agent Execution Engine SHALL execute Agent Process transitions behind AgentFS
and MUST NOT create the system Kernel, `/srv`, system Root Agent role, Host
endpoint, or System Store root. During this change only, Alan OS Host MAY use a
fixed internal boot composition that the Service Manager change MUST delete.

#### Scenario: Engine is started for an Agent Process
- **WHEN** the Host composition starts Agent execution
- **THEN** the engine receives an assembled Process namespace and descriptors
- **AND** it does not construct a competing system root

### Requirement: Agent Runtime Service owns Agent Process assembly
Agent Runtime Service SHALL own Process clone inputs, namespace mount assembly,
AgentFS lifecycle wiring, and the handles passed to an Agent Process. Agent
Execution Engine SHALL receive the assembled namespace and transition-owned
file handles and MUST NOT construct Kernel, AgentFS, LLMFS, RouteFS, or child
Process namespace infrastructure.

#### Scenario: Agent Process starts
- **WHEN** Agent Runtime Service starts an Agent Process
- **THEN** it assembles the Process namespace and AgentFS lifecycle before the
  transition loop begins
- **AND** Agent Execution Engine receives only the namespace and files needed to
  execute transitions

#### Scenario: Child Agent Process starts
- **WHEN** an Agent Process requests a child with a narrower capability set
- **THEN** Agent Runtime Service clones and assembles the child Process namespace
- **AND** Agent Execution Engine does not instantiate file servers or Kernel
  Process infrastructure for the child

#### Scenario: Dependency validation runs
- **WHEN** an assembly responsibility has moved out of Agent Execution Engine
- **THEN** its corresponding normal dependency is removed from
  `alan-agent-engine`
- **AND** the repository dependency gate rejects its reintroduction
