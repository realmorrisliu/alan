# agent-namespace-runtime Specification

## Purpose
TBD - created by archiving change refactor-engine-namespace-native. Update Purpose after archive.
## Requirements
### Requirement: The engine's environment is its namespace
The Agent Execution Engine SHALL take exactly one environment: a namespace handle
(an aP client over its mounted root). It SHALL NOT take an injected LLM provider
object, an in-process tool registry, or an event-emission sink as its
environment. Every capability the engine exercises SHALL be reached by walking a
path in that namespace, so an agent's capability set is exactly what its spawner
mounted (ADR-0024 D6).

#### Scenario: The engine is constructed
- **WHEN** the engine loop is constructed for an agent process
- **THEN** it receives a single namespace handle and no `LlmProvider`,
  `ToolRegistry`, or event-sink object
- **AND** anything not present in that namespace is unreachable to the agent

#### Scenario: A capability is withheld
- **WHEN** a model or tool must be denied to an agent
- **THEN** it is simply not mounted into that agent's namespace
- **AND** no separate injected-capability check is required to enforce the denial

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
The engine SHALL invoke a tool by spawning its executable via `/proc/clone`
(exec spec carrying arguments and the child's namespace) and reading the tool
process's output/result files, projecting them into the agent's `actions/<id>/`.
The engine SHALL NOT call an in-process tool registry function to produce a tool
effect.

#### Scenario: The engine calls a tool
- **WHEN** a turn requires a tool effect
- **THEN** the engine spawns the tool executable via `/proc/clone` and reads its
  output files for the result
- **AND** no in-process tool-registry function is on the effect path

#### Scenario: A tool is not mounted
- **WHEN** the tool executable is not bound into the agent's namespace (`/bin`)
- **THEN** the spawn cannot resolve the executable
- **AND** the agent cannot perform that effect

### Requirement: Agent state is written to the agent's files
The engine SHALL write its state directly to the agent's `/agent/<pid>` files —
assistant output to `io/output`, the tape to `machine/tape`, yields to
`requests/<id>/`, and tool calls to `actions/<id>/` — as the source of truth. The
engine SHALL NOT publish its state by emitting the `Event`/`EventEnvelope`
alphabet on its live path; that alphabet remains only as a legacy compatibility
transport behind a file server (ADR-0025 D4).

#### Scenario: The engine produces output
- **WHEN** the engine emits assistant text, a yield, or a tool call
- **THEN** it appends to `io/output`, creates a `requests/<id>/` tree, or creates
  an `actions/<id>/` tree respectively
- **AND** these files are the source of truth, not a derived projection of events

#### Scenario: A consumer observes the agent
- **WHEN** a client wants the agent's output or state
- **THEN** it reads the agent's files (e.g. tails `io/output` or `events`)
- **AND** it does not subscribe to an `EventEnvelope` stream on the engine's live
  path

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

