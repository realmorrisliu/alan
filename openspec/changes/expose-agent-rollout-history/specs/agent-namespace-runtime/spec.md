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

For a top-level Agent Process requested by a non-Agent Process, Agent Runtime
Service SHALL expose clone-via-open `/agent/clone`. Opening the file SHALL pin
the current `/agent/root` Process as parent, allocate an ordinary pending
Process slot through that parent's `/proc/clone` context, and return the
allocated PID. The caller SHALL write one existing `AgentExecutableRequest`
and commit it on clunk. Agent Runtime Service SHALL derive the child from the
Root Agent Process's registered runtime template and SHALL reject the commit
if the Root Agent is unavailable, has been replaced since open, or the request
would amplify the parent's capabilities. `/agent/clone` SHALL NOT create a
second Process owner, launch identity, or lifecycle surface; `/proc/<pid>`
remains authoritative.

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

#### Scenario: Non-Agent Process requests a top-level Agent Process
- **WHEN** an authorized Shell or renderer Process opens `/agent/clone`
- **THEN** Agent Runtime Service returns the PID of an ordinary Process whose
  parent is the current Root Agent Process
- **AND** committing the `AgentExecutableRequest` launches it through the
  pinned Root Agent Process's `/proc/clone` context
- **AND** the Shell or renderer Process is not treated as an Agent parent

#### Scenario: Root Agent changes during top-level launch
- **WHEN** `/agent/root` is unavailable or no longer identifies the Process
  pinned when `/agent/clone` was opened
- **THEN** Agent Runtime Service rejects the launch commit
- **AND** it does not reparent the pending launch or fall back to the caller's
  Process context

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
