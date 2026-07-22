## MODIFIED Requirements

### Requirement: Agent Runtime Service owns Agent Process assembly
Agent Runtime Service SHALL implement the Agent Executable bound at
`/bin/alan-agent` and SHALL own Process clone inputs, namespace mount assembly,
AgentFS lifecycle wiring, mounted connection selection, Agent Machine startup,
and runtime cleanup. Agent Execution Engine SHALL receive the assembled
namespace and transition-owned file handles and MUST NOT construct Kernel,
AgentFS, LLMFS, RouteFS, Host Mount, Tool Process native sandbox, or child
Process namespace infrastructure. It MUST NOT receive an engine-owned Process
launch context, child assembler, lifecycle callback, or live mount applicator.

#### Scenario: Agent Process starts
- **WHEN** a Process executes `/bin/alan-agent` through `/proc/clone`
- **THEN** Agent Runtime Service binds AgentFS, resolves the mounted connection,
  starts Agent Machine, and wires cleanup before transitions begin
- **AND** Agent Execution Engine receives only the namespace and files needed to
  execute transitions

#### Scenario: Child Agent Process starts
- **WHEN** an Agent Process requests a child with an explicitly delegated,
  possibly narrower capability set
- **THEN** the parent writes an exec spec for `/bin/alan-agent` through
  `/proc/clone`
- **AND** Agent Runtime Service assembles the child AgentFS and runtime from that
  Process namespace
- **AND** Agent Execution Engine does not call a child assembly or lifecycle
  callback

#### Scenario: Agent Process exits
- **WHEN** `/proc/<pid>` reaches a terminal exit state
- **THEN** Agent Runtime Service cleans up its Agent Machine and AgentFS runtime
  backing
- **AND** `/proc/<pid>` remains the lifecycle source of truth

#### Scenario: Dependency validation runs
- **WHEN** repository validation inspects `alan-agent-engine` normal dependencies
- **THEN** `alan-kernel` and displaced system-composition dependencies are absent
- **AND** a development-only dependency used by a public contract test is not
  treated as production ownership
- **AND** the repository dependency gate rejects reintroduction of a removed
  normal edge
