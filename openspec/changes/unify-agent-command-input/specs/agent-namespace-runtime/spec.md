## MODIFIED Requirements

### Requirement: Agent state is written to the agent's files
The engine SHALL have each state owner write directly to the Agent Process files:
assistant output to `io/output`, tape state to `machine/tape`, yields to
`requests/<id>/`, Tool calls to `actions/<id>/`, and renderer-approved state to
`machine/ui/`. These files and their owned streams SHALL be the source of truth.
The engine SHALL NOT publish live state through `EventEnvelope`,
`RuntimeEventEnvelope`, a broadcast sender, or a generic event-to-file
projector.

Command outputs, selected route and submission outcomes SHALL also be written
by those existing owners, with executable effects referencing their Process and
Action evidence. No final assistant generation is required for command results.

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

### Requirement: Agent Machine owns transition-local state
Agent Execution Engine SHALL keep Tape, the current accepted submission, turn
state, pending Yield, Tool replay state, active-task state, and deferred
transition action inside one Agent Machine owner. Those values MUST NOT be
independently mutable through a shared runtime field bag, and Agent Machine
internals MUST NOT be a public cross-crate integration surface.

This same owner SHALL manage accepted command intent, ordinary submission
ordering and queue pause state. Cwd changes SHALL update the existing Process
context authority; no independent renderer or Machine copy may become a second
source of cwd truth.

#### Scenario: A submission starts a transition
- **WHEN** the outer Process loop accepts a `Submission`
- **THEN** Agent Machine records and advances all state local to that transition
- **AND** sibling runtime modules cannot mutate its Tape or turn state directly

#### Scenario: Engine API visibility is inspected
- **WHEN** repository validation inspects `alan-agent-engine` exports and field
  visibility
- **THEN** Agent Machine state is private to the engine implementation
- **AND** supported observation remains AgentFS, `/proc`, rollout, and checkpoint
  files

#### Scenario: A pending transition resumes
- **WHEN** Agent Runtime Service restores a Yield, Tool replay, or deferred action
  from durable files
- **THEN** Agent Machine resumes from that restored transition state
- **AND** no parallel runtime field bag must be reconciled with it

### Requirement: Runtime context is Process-shaped
Agent Execution Engine SHALL derive file reachability, cwd, Tool execution,
Agent Definition, Skills, policy, memory handles, and durable evidence
references from the Agent Process namespace and descriptors. It MUST NOT own a
workspace identity, workspace root, or Host `.alan` directory.

The shared interaction cwd SHALL be owned by the Agent Process, visible to all
attachments and updated by an authorized explicit user cd in execution order.
Per-action cwd SHALL NOT update that shared value implicitly.

#### Scenario: Runtime prepares a turn
- **WHEN** an Agent Process begins a transition
- **THEN** every contextual resource is read from a mounted path or descriptor
- **AND** no Host-directory overlay scan occurs

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
- **AND** `machine/ctl` owns Agent work interruption, queue controls and
  tape/checkpoint commands such as `compact` and `rollback`
- **AND** the kernel interprets no agent-runtime control semantics

#### Scenario: A sub-agent is given a narrower world
- **WHEN** a parent spawns a sub-agent with fewer mounts (e.g. no model, or a
  restricted tool set)
- **THEN** the sub-agent's namespace omits those trees
- **AND** the sub-agent cannot reach the withheld capabilities by any path

### Requirement: M2 — a real conversation flows entirely through files
For Agent-route work requiring generation, the shell-to-agent conversation SHALL be expressible end-to-end as file
operations with no RPC and no provider injection: the shell writes a message to
the agent's `io/input`; the agent reads it, generates by reading `/mnt/llm`, and
writes `io/output`; the shell tails `io/output`.

#### Scenario: The shell talks to a real agent
- **WHEN** the shell writes a user message to a spawned agent's `io/input` and
  tails its `io/output`
- **THEN** the agent reads the input, generates via its mounted LLM connection,
  and the model's response appears on `io/output`
- **AND** no operation outside aP file IO is used to carry the conversation

### Requirement: Generation is a namespace file operation
When an Agent Machine operation requires generation, it SHALL use file
operations on the mounted LLM connection: open `/mnt/llm/connections/<conn>/clone` (clone-via-open),
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

## ADDED Requirements

### Requirement: One Agent Machine owns command and reasoning execution
An Agent Machine SHALL advance accepted work through deterministic command
operations or generation using its existing namespace and governance. Direct
commands SHALL not require a generation step. Native shell execution SHALL use
the ordinary governed Tool Process launch boundary and existing action evidence,
shared with Agent commands. The Host adapter SHALL track native descendants;
each shell fork need not create a separate Alan Process. Standalone user cd
SHALL update the owning Process context only after authorization. No renderer,
classifier or second execution manager SHALL own a competing execution path.

#### Scenario: Explicit command needs no model
- **WHEN** a valid explicit command has the authority required by its execution policy
- **THEN** it executes without intent evaluation or generation
- **AND** unavailable models do not grant missing authorization

#### Scenario: Model-callable Tool and human executable differ
- **WHEN** a human script names a native executable without an Alan Tool manifest
- **THEN** execution still requires ordinary Process authority and applicable policy
- **AND** the command does not thereby become a model-callable Tool

### Requirement: Agent Process owns ordered input and shared cwd
Ordinary accepted submissions SHALL execute in one Agent-owned order. Controls
and request responses SHALL retain their separate handling. The Agent Process
SHALL own one cwd reference to a delegated Host Mount and normalized relative
location, resolved by the Host adapter into native execution cwd. Standalone
explicit user `cd <directory>` SHALL accept one quoted/escaped literal Host or
relative directory and update it in execution order after access checks. No-arg,
`-`, expansion and substitution forms SHALL fail explicitly in this initial
builtin. Composed scripts SHALL retain native shell cd semantics without changing
shared cwd. Virtual aP directories SHALL NOT serve as native execution cwd. Per-action Agent
cwd SHALL NOT silently change it. Each submission SHALL have correlated outcomes.

#### Scenario: Command follows a directory change
- **WHEN** standalone `!cd subdir` succeeds before a queued relative command
- **THEN** that command uses the updated authorized native cwd at execution time

#### Scenario: Script changes its own directory
- **WHEN** a user submits `!cd subdir && make` or Agent work changes its action cwd
- **THEN** the change is local to that native action
- **AND** later submissions use the unchanged shared Process cwd

#### Scenario: Standalone directory change requests expansion
- **WHEN** standalone user cd requests `~`, `$HOME`, command substitution or no operand
- **THEN** it reports the supported literal-directory form without changing cwd

#### Scenario: Directory change fails
- **WHEN** explicit `cd` names an unavailable or unauthorized directory
- **THEN** it fails and the prior cwd remains authoritative

#### Scenario: Agent uses a temporary action directory
- **WHEN** an Agent action executes in a different authorized directory
- **THEN** later human commands retain the shared cwd unless an explicit user `cd` changed it

#### Scenario: Interrupt pauses queued work
- **WHEN** current work is interrupted with ordinary submissions waiting
- **THEN** active work is cancelled and pending work remains paused for explicit continue or discard
- **AND** controls stay responsive and completed effects are not automatically undone

### Requirement: Restart does not imply input replay or directory substitution
After Agent or Host restart, recoverable pending work SHALL remain paused until
explicit continuation. Unknown effects SHALL require reconciliation before any
retry. Cwd SHALL be restored only from reliable state with valid current access;
otherwise directory-dependent work SHALL require an explicit directory choice.
Missing records SHALL be reported rather than reconstructed from textual Tape or
reused PIDs. Client detach alone SHALL NOT impose this restart behavior.

#### Scenario: Pending input is recovered
- **WHEN** recovery finds a reliable pending submission record
- **THEN** it exposes that work as paused rather than dispatching it automatically

#### Scenario: Directory is no longer available
- **WHEN** restored cwd cannot be validated against current namespace access
- **THEN** directory-dependent work waits for an explicit choice or fails without a response channel
- **AND** the runtime does not silently execute from another directory

### Requirement: Automatic routing requires qualified typed evaluation
Automatic input routing SHALL use the reachable typed evaluation capability and
its bounded failure contract. Before activation it SHALL run in shadow mode with
no effects caused by classification, compare baseline behavior and record intent
accuracy, false execution classifications, latency and cost. Known cases that
classify discussion as execution SHALL block activation. Numeric qualification
budgets SHALL be set before candidate evaluation and activation SHALL be explicit.
Until qualification, unprefixed input SHALL retain the governed Agent baseline.

#### Scenario: Shadow classification chooses a command
- **WHEN** the evaluator selects command intent during qualification
- **THEN** that selection itself starts no command
- **AND** it is recorded for comparison with the labeled intent and baseline

#### Scenario: A discussion is classified as execution
- **WHEN** qualification reveals a known false execution classification
- **THEN** automatic routing is not enabled until the case is resolved and qualification repeated
