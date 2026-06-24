## ADDED Requirements

### Requirement: Agent capability is a first-class OS model
Alan OS SHALL provide Agent Capability as a first-class model for
AI-mediated reading, planning, transformation, delegation, memory, and action
that Alan Apps can request without depending on the Alan Agent product UI.

#### Scenario: App requests agent assistance
- **WHEN** an Alan App needs AI-mediated work such as reading assistance,
  practice help, or task planning
- **THEN** it requests an Agent Capability through the OS model
- **AND** it does not need to embed an app-local chatbot or route through the
  Alan Agent UI

#### Scenario: OS role is reviewed
- **WHEN** future specs define agent behavior for Alan Apps
- **THEN** they identify whether the behavior is an OS Primitive, Host
  Service Capability, Alan Agent App Feature, compatibility-only behavior, or
  rewrite candidate

### Requirement: System Agent Supervisor is always available but not a session
Alan OS SHALL define a System Agent Supervisor as the always-available
Alan OS agent supervisor with long-lived identity, memory, system awareness, and
cross-app continuity. The System Agent Supervisor SHALL supervise and start
scoped Agent Runs; it SHALL NOT be modeled as an ever-growing Agent Session or
global root conversation.

#### Scenario: Alan OS starts
- **WHEN** Alan OS starts its OS services
- **THEN** the System Agent Supervisor can maintain resident identity, memory,
  and system awareness
- **AND** model reasoning or action is performed through bounded Agent Runs
  rather than an unbounded supervisor session

#### Scenario: Root session design is proposed
- **WHEN** a future change proposes a resident root agent session
- **THEN** it is rejected or reframed as System Agent Supervisor plus bounded
  Agent Runs
- **AND** it preserves app context isolation, permissions, auditability, and
  resource bounds

### Requirement: Supervisor authority is governed
The System Agent Supervisor SHALL have broad system awareness and suggestion
power, but app-private reads and side effects SHALL be mediated through Context
Grants, Command Governance, permission checks, and audit records.

#### Scenario: Supervisor observes cross-app work
- **WHEN** the System Agent Supervisor connects activity across apps
- **THEN** it may propose help, raise tasks, or start permitted low-risk Agent
  Runs
- **AND** it does not read app-private content or perform side effects unless a
  governed context and command path permits it

#### Scenario: Supervisor attempts privileged action
- **WHEN** the System Agent Supervisor proposes a side effect such as modifying a
  file, publishing content, changing app data, or crossing app boundaries
- **THEN** the action is evaluated by Command Governance
- **AND** the outcome is allow, deny, or yield for approval according to policy,
  risk, guard strength, and auditability

### Requirement: Agent Capability Service is a Host Service API
Agent Capability Service SHALL be a Host Service API that starts, schedules,
streams, yields, and completes Agent Runs using provider, runtime, memory, and
execution implementations outside Alan Kernel. Alan Kernel SHALL own only the
semantic model for Agent Capability descriptors, agent actors, Agent Runs,
Context Grants, Result Contracts, tasks, commands, permissions, evidence, and
audit.

#### Scenario: Kernel dependencies are audited
- **WHEN** Alan Kernel or `alan-kernel` dependencies are reviewed
- **THEN** they do not include provider clients, daemon session clients, sandbox
  execution, or concrete LLM runtime dependencies for Agent Capability execution
- **AND** those implementation concerns remain in Host Service APIs or Host
  Service Implementations

#### Scenario: Host implementation changes
- **WHEN** a daemon-backed, macOS-embedded, cloud, or test implementation runs
  Agent Capability Service
- **THEN** it satisfies the same Agent Run, Context Grant, Result Contract,
  streaming, yield, permission, and audit semantics

### Requirement: Agent runs are bounded and app-owned by default
Agent Runs SHALL be bounded executions of an Agent Capability against a specific
app, object, task, context, permission scope, and audit record. An Agent Run
SHALL be owned by the requesting app, object, or task by default while the
System Agent Supervisor may provide continuity across runs.

#### Scenario: UPDF starts reading assistance
- **WHEN** UPDF requests an explanation for a selected page range
- **THEN** the resulting Agent Run is owned by UPDF and the target document
  context
- **AND** the System Agent Supervisor may connect the run to broader user goals
  only through permitted memory and context surfaces

#### Scenario: Groove Master starts practice assistance
- **WHEN** Groove Master requests a next-step practice suggestion
- **THEN** the resulting Agent Run is owned by the Groove Master practice
  session or task
- **AND** it preserves Groove Master's domain rules such as non-graded,
  low-presence practice assistance

### Requirement: Context grants define agent run input authority
Alan Apps SHALL provide Agent Run input authority through typed Context Grants.
A Context Grant SHALL identify app identity, object references, view references
or selected ranges where applicable, task goal, allowed reads, allowed commands,
privacy policy, evidence requirements, and expected result shape.

#### Scenario: App grants bounded context
- **WHEN** an app requests an Agent Run
- **THEN** it supplies a Context Grant that bounds what the run can read,
  reference, propose, or invoke
- **AND** the Agent Run does not receive implicit full access to app state

#### Scenario: Prompt dump is proposed
- **WHEN** a future app integration proposes passing raw app state or an
  untyped prompt as the main agent input
- **THEN** it is replaced by or wrapped in a typed Context Grant before becoming
  OS behavior

### Requirement: Result contracts define typed agent run output
Alan Apps SHALL request typed Agent Run outputs through Result Contracts. A
Result Contract SHALL be able to include answers, citations, evidence, proposed
commands, draft objects, follow-up questions, uncertainty, memory updates, and
audit summaries.

#### Scenario: App receives an agent result
- **WHEN** an Agent Run completes
- **THEN** the requesting app receives output matching the requested Result
  Contract
- **AND** the app does not need to parse plain text to discover citations,
  evidence, proposed commands, draft objects, or memory updates

#### Scenario: Natural-language answer is requested
- **WHEN** an app only needs a user-visible answer
- **THEN** the Result Contract may include an answer field
- **AND** the Alan OS still records evidence and audit metadata when the run
  used governed context or proposed commands

### Requirement: Agent capabilities use typed descriptors over common runs
Agent Capability behavior SHALL share a common Agent Run substrate while exposing
typed Agent Capability Descriptors for capability kinds such as explain,
summarize, plan, transform, propose commands, delegate, and remember.

#### Scenario: OS capability is invoked
- **WHEN** an app invokes an OS agent ability
- **THEN** it names an Agent Capability Descriptor and starts a bounded Agent Run
  with a Context Grant and Result Contract

#### Scenario: App-specific capability is exposed
- **WHEN** UPDF exposes reading assistance or Groove Master exposes practice
  help
- **THEN** the app-specific feature maps to one or more Agent Capability
  Descriptors instead of inventing an untyped app-local agent protocol

### Requirement: Alan Agent is the Agent Workspace
Alan Agent SHALL be the built-in Alan App and user-visible Agent Workspace for
inspecting, steering, and organizing agent sessions, agent runs,
supervisor-raised tasks, memory, evidence, and cross-app work. Alan Agent SHALL
NOT be treated as the System Agent Supervisor itself.

#### Scenario: User opens Alan Agent
- **WHEN** the user opens Alan Agent
- **THEN** they can inspect and steer agent work through a product workspace
- **AND** the underlying System Agent Supervisor remains an OS service, not
  the app UI itself

#### Scenario: Domain app requests agent work
- **WHEN** a domain app requests an Agent Capability
- **THEN** it can do so directly through Agent Capability Service
- **AND** it does not need to create or focus an Alan Agent conversation unless
  the user or app explicitly promotes the work into the Agent Workspace

### Requirement: Agent memory is layered by owner
Agent memory SHALL be layered into User Memory, System Memory, and App Memory.
User Memory SHALL hold permitted long-lived user preferences, habits, goals, and
working style. System Memory SHALL hold cross-app activity and system-level
continuity. App Memory SHALL remain owned by the app and SHALL be exposed to
Agent Runs only through app-controlled memory surfaces or Context Grants.

#### Scenario: Agent run proposes memory update
- **WHEN** an Agent Run proposes a memory update
- **THEN** the update identifies whether it targets User Memory, System Memory,
  or App Memory
- **AND** it follows the owner, provenance, privacy, and revert rules for that
  memory layer

#### Scenario: Supervisor uses app memory
- **WHEN** the System Agent Supervisor needs app-owned history such as reading
  history or practice logs
- **THEN** it accesses that memory only through app-controlled memory surfaces,
  Context Grants, or explicit permission paths

### Requirement: Command governance generalizes tool governance
Alan OS SHALL provide Command Governance for all OS commands,
including shell commands, app commands, domain actions, and agent-proposed
commands. Command Governance SHALL generalize the existing Alan Agent
allow/deny/escalate policy, approval checkpoint, audit metadata, capability
classification, sandbox backend, and safe-degradation ideas without treating
shell-specific mechanisms as the whole OS model.

#### Scenario: Agent proposes command
- **WHEN** an Agent Run returns proposed commands
- **THEN** each command is evaluated through Command Governance before execution
- **AND** execution is allowed, denied, or yielded for approval with audit
  metadata

#### Scenario: Existing tool governance is migrated
- **WHEN** current Alan Agent tool policy, approval, sandbox, or auto-execution
  behavior is migrated
- **THEN** it is classified as source material for Command Governance,
  Execution Guards, Host Service Capabilities, or compatibility behavior
- **AND** it is not copied wholesale into Alan Kernel

### Requirement: Command risk uses effect classes and execution guards
Command Governance SHALL evaluate Command Risk using policy, coarse capability,
Effect Class, target scope, reversibility, Execution Guard strength, and
auditability. Effect Classes SHALL include inspect, draft, modify, delete,
publish, execute, delegate, remember, and cross-app or equivalent semantic
categories. Execution Guards SHALL include mechanisms such as OS sandbox,
workspace path guard, app object guard, domain validator, and human approval
gate.

#### Scenario: Low-risk draft command is evaluated
- **WHEN** an Agent Run proposes a draft-only app command within the Context
  Grant target scope
- **THEN** Command Governance may allow it automatically when policy,
  reversibility, execution guard strength, and auditability support auto-run

#### Scenario: High-risk command is evaluated
- **WHEN** an Agent Run proposes delete, publish, irreversible modify,
  privilege escalation, cross-app write, external network side effect, or opaque
  shell/process execution without strong containment
- **THEN** Command Governance requires approval or denial according to policy and
  guard strength

### Requirement: Existing Alan Agent capabilities migrate by class
Existing Alan Agent capabilities SHALL be migrated by explicit classification:
OS Primitive, Host Service Capability, Alan Agent App Feature,
compatibility-only behavior, or rewrite candidate. The migration SHALL preserve
or adapt capabilities that fit the new boundaries and SHALL reshape or rewrite
capabilities that conflict with System Agent Supervisor, Agent Capability
Service, Agent Run, Context Grant, Result Contract, or Command Governance
boundaries.

#### Scenario: Capability migration map is reviewed
- **WHEN** a follow-up change migrates existing Alan Agent session, tape, tool,
  skill, policy, sandbox, memory, child-agent, rollout, or conversation behavior
- **THEN** it records the migration class for each affected capability
- **AND** it explains whether the capability is preserved, adapted, left as
  compatibility-only behavior, or rewritten

#### Scenario: Runtime detail is proposed as Kernel behavior
- **WHEN** a migration proposes moving a current Agent Execution Engine detail
  into Alan Kernel
- **THEN** it must show that the detail is a durable OS Primitive rather
  than a Host Service Capability, Alan Agent App Feature, or compatibility
  artifact

### Requirement: Initial implementation scope remains bounded
This OS model SHALL NOT expand the first `introduce-alan-kernel-runtime`
implementation slice into a complete Agent Capability Service or System Agent
Supervisor implementation. Early Kernel work SHALL define semantic primitives
and compatibility projections first; execution services and app migrations SHALL
arrive through follow-up changes.

#### Scenario: Alan Kernel scope is reviewed
- **WHEN** `alan-kernel` or future `alan-kernel` implementation work is
  reviewed against this model
- **THEN** it may define semantic ids, descriptors, commands, tasks, views,
  evidence, permissions, and audit shapes for Agent Capability
- **AND** it does not implement provider execution, resident supervision,
  sandboxing, memory storage, or session protocol clients inside Kernel
