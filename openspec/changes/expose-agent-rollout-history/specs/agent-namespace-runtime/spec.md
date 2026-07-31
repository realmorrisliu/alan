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
Service SHALL expose a dedicated clone-via-open launch capability tree. Service
Manager SHALL bind that tree at `/mnt/agent-runtime` only in the authorized
renderer attachment view over a Local Entry Shell Process namespace. The
underlying Shell Process namespace SHALL omit the tree, and the attachment
overlay SHALL NOT change `/proc/self/namespace`. Service Manager MUST NOT
publish the tree through `/srv`, add it to `/agent`, or retain it when
assembling any child Process namespace.

Opening `/mnt/agent-runtime/clone` SHALL pin the current `/agent/root` Process
as parent, allocate an ordinary pending Process slot through that parent's
`/proc/clone` context, and return the allocated PID. The caller SHALL write one
existing `AgentExecutableRequest` and commit it on clunk. Agent Runtime Service
SHALL derive the child from the Root Agent Process's registered runtime
template and SHALL reject the commit if the Root Agent is unavailable, has
been replaced since open, or the request would amplify the parent's
capabilities. The launch capability SHALL NOT create a second Process owner,
launch identity, or lifecycle surface; `/proc/<pid>` remains authoritative.

When assembling an Agent Process namespace, Agent Runtime Service SHALL bind
`/agent` through an ordinary quota-scoped handle. Delegation of that mounted
handle SHALL share its history-fid quota account rather than minting another
account. The account SHALL remain capability-local resource policy, not
Process identity or durable state.

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
- **WHEN** an authorized local renderer opens `/mnt/agent-runtime/clone`
  through its attachment view
- **THEN** Agent Runtime Service returns the PID of an ordinary Process whose
  parent is the current Root Agent Process
- **AND** committing the `AgentExecutableRequest` launches it through the
  pinned Root Agent Process's `/proc/clone` context
- **AND** the Shell Process is not treated as an Agent parent

#### Scenario: Shell launches an ordinary child
- **WHEN** the Shell copies the namespace described by
  `/proc/self/namespace` into a child Process manifest
- **THEN** `/mnt/agent-runtime` is absent from that manifest
- **AND** the child cannot exercise the renderer-only top-level launch
  capability

#### Scenario: Delegated Agent Process has a read-write agent mount
- **WHEN** a delegated Agent Process receives read-write `/agent`
- **THEN** its namespace still omits `/mnt/agent-runtime`
- **AND** it cannot use the top-level launch capability to recover capabilities
  withheld by its parent
- **AND** its inherited `/agent` handle shares the parent's history-fid quota

#### Scenario: Root Agent changes during top-level launch
- **WHEN** `/agent/root` is unavailable or no longer identifies the Process
  pinned when `/mnt/agent-runtime/clone` was opened
- **THEN** Agent Runtime Service rejects the launch commit
- **AND** it does not reparent the pending launch or fall back to the caller's
  Process context

#### Scenario: Agent Process exits
- **WHEN** `/proc/<pid>` reaches a terminal exit state
- **THEN** Agent Runtime Service cleans up its Agent Machine and AgentFS runtime
  backing
- **AND** `/proc/<pid>` remains the lifecycle source of truth

#### Scenario: Agent Process is still pre-exit
- **WHEN** terminal Rollout finalization has completed but `/proc/<pid>` has not
  yet published its terminal state
- **THEN** `/agent/<pid>` remains bound and continues to identify the Process
  as an Agent Process
- **AND** AgentFS cleanup begins only after `/proc/<pid>` becomes terminal

#### Scenario: Dependency validation runs
- **WHEN** repository validation inspects `alan-agent-engine` normal dependencies
- **THEN** `alan-kernel` and displaced system-composition dependencies are absent
- **AND** a development-only dependency used by a public contract test is not
  treated as production ownership
- **AND** the repository dependency gate rejects reintroduction of a removed
  normal edge
