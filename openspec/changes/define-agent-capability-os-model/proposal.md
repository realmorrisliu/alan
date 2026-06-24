## Why

Alan OS needs agent capabilities to become operating-system-level
abilities that any Alan App can call without depending on the Alan Agent product
UI. This is the next step after defining Alan OS: keep the useful Alan
Agent runtime, governance, sandbox, memory, and delegation work, but place each
capability at the right OS layer.

## What Changes

- Define Agent Capability as a first-class Alan OS model for AI-mediated
  reading, planning, transformation, delegation, memory, and action.
- Introduce the System Agent Supervisor as the always-available Alan OS agent
  supervisor with long-lived identity, memory, system awareness, and cross-app
  continuity.
- Define bounded Agent Runs as the system-call shaped execution unit for agent
  work, distinct from long-lived Alan Agent sessions and the System Agent
  Supervisor.
- Define Context Grants and Result Contracts so Alan Apps pass typed, governed
  context into Agent Runs and receive typed results instead of prompt dumps and
  plain-text responses.
- Define Agent Capability Service as a Host Service API, while Alan Kernel owns
  the semantic model for agent actors, runs, context grants, result contracts,
  commands, tasks, permissions, evidence, and audit.
- Generalize existing Alan Agent tool governance, approval, sandbox, and
  auto-execution ideas into OS Command Governance with effect classes,
  command risk, execution guards, and audit records.
- Define migration classes for existing Alan Agent capabilities: OS Primitive,
  Host Service Capability, Alan Agent App Feature,
  compatibility-only behavior, or rewrite candidate.
- Add a migration map for current Alan Agent capabilities, covering session,
  tape, turn loop, provider/runtime execution, tools, skills, policy, approval,
  sandbox, memory, compaction, child runs, rollout, daemon transport, and TUI
  projection.
- Clarify that Alan Agent is the built-in Agent Workspace and first complete app
  experience for agent work, not the System Agent Supervisor itself.
- Keep the current Alan Kernel implementation slice bounded: this change is
  target model and migration planning, not an expansion of the first Kernel
  implementation tasks.

## Capabilities

### New Capabilities

- `agent-capability-os-model`: Defines Alan OS's first-class agent
  capability model, including System Agent Supervisor, Agent Capability Service,
  Agent Runs, Context Grants, Result Contracts, Command Governance, memory
  ownership, and migration of existing Alan Agent capabilities.

### Modified Capabilities

- None.

## Impact

- Affected OpenSpec planning: follow-up implementation changes should map Alan
  Agent work into this model instead of creating a greenfield agent system or
  copying current `alan-runtime` details into Alan Kernel.
- Affected architecture: `alan-kernel` should own
  agent capability semantics, while provider/runtime execution, streaming,
  yielding, sandbox, memory storage, scheduling, and process supervision remain
  Host Service API / implementation concerns.
- Affected Alan Agent migration: existing session, tape, tool, skill, policy,
  sandbox, memory, child-agent, rollout, and conversation capabilities must be
  classified before migration as OS primitives, host-service
  capabilities, Alan Agent app features, compatibility-only behavior, or rewrite
  candidates.
- Affected future apps: UPDF, Groove Master, and other Alan Apps should request
  AI features through Agent Capability calls with typed context grants and
  result contracts rather than embedding app-local chatbots or depending on the
  Alan Agent UI.
