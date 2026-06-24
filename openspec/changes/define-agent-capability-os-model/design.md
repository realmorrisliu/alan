## Context

Alan OS has been defined as the product substrate for Alan Kernel, Host
Service APIs, hosts, and Alan Apps. The current roadmap puts Alan Agent first as
the built-in Alan App that dogfoods Alan OS before later domain apps such
as Groove Master and UPDF.

That framing still leaves one important ambiguity: whether agent behavior is
only an Alan Agent app feature, or whether agent capabilities should become
standard OS abilities that any Alan App can call. The desired direction is
closer to Alan OS: UPDF should be able to request reading assistance, Groove
Master should be able to request practice help, and Alan Agent should remain the
full agent workspace, all without each app embedding its own chatbot or runtime.

The current Alan Agent implementation is valuable source material. It already
contains sessions, tape, tool execution, skills, child agents, memory,
compaction, rollout persistence, policy decisions, approval yields, sandbox
backend selection, and autonomous execution rules. This change keeps that work
and relocates it into the right Alan OS boundaries instead of replacing it with a
greenfield agent system.

## Goals / Non-Goals

**Goals:**

- Define agent capability as a first-class Alan OS model.
- Distinguish the always-available System Agent Supervisor from bounded Agent
  Runs and long-lived Alan Agent sessions.
- Make Agent Capability Service a Host Service API while keeping Alan Kernel
  focused on semantic contracts.
- Define Context Grants and Result Contracts as the typed input/output boundary
  for app-requested agent work.
- Generalize current Alan Agent tool governance into OS Command
  Governance for all commands.
- Define migration classes for existing Alan Agent capabilities so useful work
  is preserved, adapted, or rewritten deliberately.
- Keep Alan Agent as the Agent Workspace and first complete app experience for
  agent work.

**Non-Goals:**

- Implement Agent Capability Service, System Agent Supervisor, or a new runtime
  in this change.
- Replace the current Agent Execution Engine, daemon session APIs, TUI, or
  governance implementation.
- Move provider clients, LLM loops, sandbox execution, or memory storage into
  Alan Kernel.
- Require UPDF, Groove Master, or other future apps to route through the Alan
  Agent UI to get AI behavior.
- Turn every Alan App into an agent-first product or hide app domain semantics
  behind a global chat surface.

## Decisions

### 1. System Agent Supervisor is always available but is not a session

Alan OS should have an always-available System Agent Supervisor with long-lived
identity, memory, system awareness, and cross-app continuity. It supervises
agent work and can raise or connect tasks, but actual reasoning and action
happen in bounded Agent Runs.

Alternative considered: make the resident root agent an always-open Alan Agent
session. That would leak context across apps, make permissions and audit unclear,
grow unbounded conversation state, and collapse app-specific product experiences
into one global chat.

### 2. Agent Runs are bounded and app-owned by default

Agent Runs are the system-call shaped execution unit for agent work. They are
scoped to an app, object, task, context grant, permission scope, and audit
record. By default, the requesting app owns the run while the System Agent
Supervisor provides continuity across runs.

Alternative considered: make every Agent Run supervisor-owned. That would make
apps passive context providers and would blur product semantics for UPDF reading
assistance, Groove Master practice help, and Alan Agent task work.

### 3. Agent Capability Service is a Host Service API

Alan Kernel should own agent capability semantics: Agent Capability Descriptors,
Agent Actors, Agent Runs, Context Grants, Result Contracts, tasks, commands,
permissions, evidence, and audit. Starting, scheduling, streaming, yielding, and
completing Agent Runs belongs to Agent Capability Service as a Host Service API.

Alternative considered: put the agent runtime directly inside Alan Kernel. That
would force provider clients, LLM execution, memory storage, sandboxing, and
runtime supervision into the Kernel boundary and make alternate host
implementations harder.

### 4. Context Grants and Result Contracts replace prompt dumps

Apps should request agent work by issuing typed Context Grants and expected
Result Contracts. Context Grants name app identity, object references, view
references, selected ranges, task goals, allowed reads, allowed commands,
privacy policy, evidence requirements, and result expectations. Result Contracts
describe typed outputs such as answers, citations, evidence, proposed commands,
draft objects, follow-up questions, uncertainty, memory updates, and audit
summary.

Alternative considered: expose a prompt-style API. That would be easy to wire
but would force each app to invent its own permission, evidence, output parsing,
and audit conventions.

### 5. Agent capabilities use typed descriptors over common runs

Agent work shares a common Agent Run substrate, but apps and users should see
typed Agent Capability Descriptors such as explain, summarize, plan, transform,
propose commands, delegate, and remember. App-specific features such as
UPDF's reading assistance or Groove Master's practice suggestions adapt those
descriptors into domain language.

Alternative considered: expose one untyped `agent.run` call everywhere. That
keeps the low-level API small but loses permission precision, result typing, and
product language.

### 6. Alan Agent is the Agent Workspace, not the supervisor

Alan Agent remains the built-in Alan App for agent work. It is the user-visible
Agent Workspace for inspecting, steering, and organizing agent sessions, agent
runs, supervisor-raised tasks, memory, evidence, and cross-app work. Other apps
may call Agent Capability Service directly.

Alternative considered: make Alan Agent the only visible form of the System
Agent Supervisor. That would make direct app-level AI features depend on the
Alan Agent UI and would make Alan Agent too large a product boundary.

### 7. Memory is layered by owner

Agent memory should be layered into User Memory, System Memory, and App Memory.
The System Agent Supervisor may use permitted User and System Memory for
continuity. App Memory remains app-owned and is exposed through app-controlled
memory surfaces or Context Grants.

Alternative considered: one global supervisor memory. That would improve recall
but would make app privacy, provenance, and user control hard to reason about.

### 8. Command Governance generalizes existing Alan Agent tool governance

The current Alan Agent implementation already has useful governance ideas:
policy decisions, approval checkpoints, audit metadata, capability classes,
red-line detection, sandbox backend selection, and safe degradation when
containment is weak. Alan OS should generalize those ideas into Command
Governance for shell commands, app commands, domain actions, and agent-proposed
commands.

Command Governance evaluates Command Risk using policy, coarse capability,
Effect Class, target scope, reversibility, Execution Guard strength, and
auditability. Automatic execution depends on governability, not just whether an
operation is a write.

Alternative considered: leave tool governance inside Alan Agent and let each app
define its own AI action policy. That would duplicate safety logic and make
cross-app agent behavior inconsistent.

### 9. Existing Alan Agent capabilities migrate by OS class

Every existing Alan Agent capability should be classified before migration:

- **OS Primitive:** agent actor identity, tasks, commands, audit,
  evidence, capability descriptors, and agent run identity.
- **Host Service Capability:** Agent Capability Service, provider/runtime
  execution, streaming/yield, memory storage, scheduling, sandbox/execution
  guard, and process supervision.
- **Alan Agent App Feature:** full conversation workspace, session organization,
  long-running project assistant workflows, and user-facing transcript UX.
- **Compatibility-only behavior:** current daemon/TUI/session pathways that
  preserve behavior while semantic parity is built.
- **Rewrite candidate:** behavior that conflicts with the new System Agent
  Supervisor, Agent Run, Context Grant, Result Contract, or Command Governance
  boundaries.

Alternative considered: either copy all current Alan Agent internals into Alan
Kernel or discard them for a new OS agent runtime. Both are wrong: one bloats the
Kernel boundary, the other wastes proven implementation work.

## Risks / Trade-offs

- [Risk] The OS spine grows too large before implementation starts. ->
  Keep this change as a target model and require later implementation changes to
  slice the work.
- [Risk] Alan Kernel accidentally depends on providers, session protocol, or
  sandbox execution. -> Treat Agent Capability Service and Execution Guards as
  Host Service APIs / implementations.
- [Risk] System Agent Supervisor becomes root automation. -> Preserve governed
  context grants, command mediation, and audit paths for app-private reads and
  side effects.
- [Risk] App products become thin wrappers around global agent chat. -> Keep
  Agent Runs app-owned by default and require app-owned domain semantics.
- [Risk] Existing Alan Agent features are rewritten unnecessarily. -> Require
  explicit migration classification before replacement.
- [Risk] Existing sandbox and auto-execution rules are over-generalized. ->
  Generalize their decision model, not every shell-specific mechanism.

## Migration Plan

1. Accept this OS model as a target contract.
2. Align `define-programmable-environment-product` wording so Agent Capability is
   part of the Alan OS constitution.
3. Align `introduce-alan-kernel-runtime` so the first Kernel slice defines only
   semantic primitives and does not pull Agent Capability Service execution into
   `alan-kernel`.
4. Create a migration map for current Alan Agent capabilities, classifying each
   as OS Primitive, Host Service Capability, Alan Agent App Feature,
   compatibility-only behavior, or rewrite candidate.
5. Implement the first Agent Capability Service slice as a compatibility adapter
   over the current Agent Execution Engine and daemon-backed session APIs.
6. Gradually move Alan Agent UI toward the Agent Workspace model while preserving
   current session behavior.
7. Add domain-app proof points after the Agent Capability boundary exists:
   UPDF reading assistance and Groove Master practice assistance.

## Follow-Up Implementation Split

The target model is ready to split into implementation changes:

1. `add-agent-capability-kernel-types` adds Kernel semantic types only:
   descriptor ids, Agent Run identity, Context Grants, Result Contracts, Effect
   Classes, Command Risk, Execution Guard metadata, evidence, and audit shapes.
2. `add-agent-capability-service-adapter` adds the Host Service API and first
   compatibility adapter over the current Agent Execution Engine and
   daemon-backed session APIs.
3. `migrate-alan-agent-to-agent-workspace` moves Alan Agent toward the
   user-visible Agent Workspace over Agent Runs, memory, evidence, and
   supervisor-raised tasks while preserving current session behavior.

The first descriptor taxonomy is defined in `descriptor-taxonomy.md` and starts
with `agent.explain`, `agent.summarize`, `agent.plan`,
`agent.propose_commands`, and `agent.delegate`. `agent.transform` and
`agent.remember` are deferred until draft-object/edit contracts and memory-write
ownership are ready.

System Agent Supervisor resident behavior should ship after the first Agent
Capability Service compatibility adapter exists. Before then, Alan Agent can
model supervisor-raised tasks as workspace projections rather than exposing a
global root session. The first visible surface for those tasks should be Alan
Agent rendered through Alan TUI; Alan for macOS can consume the same host
contract later.

## Remaining Open Questions

- Which existing memory paths become User Memory, System Memory, and App Memory
  first?
- Which domain app proof point should exercise Agent Capability first after the
  adapter exists: UPDF reading assistance or Groove Master practice assistance?
