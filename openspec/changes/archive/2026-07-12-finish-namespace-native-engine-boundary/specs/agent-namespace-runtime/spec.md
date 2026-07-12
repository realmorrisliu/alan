## MODIFIED Requirements

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

### Requirement: Tools are executables invoked through the process namespace
The engine SHALL discover a model-callable Tool only when a visible executable
at `/bin/<tool>` has a valid mounted manifest at
`/lib/exec/<tool>/manifest`. It SHALL derive the Tool's model definition,
capability classification, locality, and execution hints from that package and
invoke the Tool by spawning its executable via `/proc/clone`, then reading the
Tool Process output/result files and projecting them into
`actions/<id>/`. The engine SHALL NOT use or reconstruct an in-process Tool
registry to grant, describe, or execute a Tool effect.

#### Scenario: The engine calls a Tool
- **WHEN** a turn requires a Tool effect and the complete Tool package is
  mounted
- **THEN** the engine resolves `/bin/<tool>`, spawns it via `/proc/clone`, and
  reads its Process output/result files
- **AND** `actions/<id>` references that concrete Tool Process
- **AND** no in-process Tool implementation call is on the effect path

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
