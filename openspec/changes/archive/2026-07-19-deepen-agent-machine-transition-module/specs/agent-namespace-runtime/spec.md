## ADDED Requirements

### Requirement: Agent Machine owns transition-local state
Agent Execution Engine SHALL keep Tape, the current accepted submission, turn
state, pending Yield, Tool replay state, active-task state, and deferred
transition action inside one Agent Machine owner. Those values MUST NOT be
independently mutable through a shared runtime field bag, and Agent Machine
internals MUST NOT be a public cross-crate integration surface.

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

### Requirement: Process-loop control is separate from transition execution
Agent Execution Engine SHALL keep input polling, channel closure, shutdown,
cancellation, and heartbeat control in its outer Process loop. A concrete
transition module SHALL own execution after a `Submission` is accepted and
SHALL return only the compact outcome required for the outer loop to continue.

#### Scenario: No submission is available
- **WHEN** the outer Process loop is polling input or handling heartbeat and
  shutdown state
- **THEN** it does not enter or mutate an Agent Machine transition

#### Scenario: An accepted submission completes or yields
- **WHEN** the transition module advances an accepted submission to completion,
  Yield, or deferred continuation
- **THEN** it updates Agent Machine and the owning file surfaces
- **AND** it returns a compact control outcome rather than transferring Machine
  state ownership to the outer loop

#### Scenario: A Process is cancelled
- **WHEN** Process lifecycle control cancels an Agent Process
- **THEN** the outer loop applies cancellation to the active transition
- **AND** cancellation does not become a second Agent Machine state owner

### Requirement: Transition dependencies are narrow and concrete
The accepted-submission transition boundary SHALL receive the one concrete
namespace-backed runtime environment. Its child modules SHALL receive only the
specific paths, handles, records, or operations required by their workflow and
MUST NOT accept the complete runtime environment or a new one-implementation
abstraction for convenience.

#### Scenario: A transition child operation is added
- **WHEN** a child module needs a namespace capability during a transition
- **THEN** the transition owner resolves or passes the narrow concrete input
  required by that operation
- **AND** the child module does not gain the complete environment field bag

#### Scenario: The ownership refactor is verified
- **WHEN** the transition module refactor completes
- **THEN** existing AgentFS, `/proc`, aP, Tool, Yield, compaction, persistence,
  recovery, memory, and child-process contract tests remain unchanged in
  behavior
- **AND** no compatibility transition path remains
