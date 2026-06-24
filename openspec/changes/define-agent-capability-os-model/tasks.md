## 1. Model Definition

- [x] 1.1 Capture System Agent Supervisor, Agent Run, Context Grant, Result
  Contract, Agent Capability Service, Command Governance, memory ownership, and
  migration class terms in the repository glossary.
- [x] 1.2 Record ADRs for the core agent capability OS decisions.
- [x] 1.3 Create the OpenSpec proposal, design, and
  `agent-capability-os-model` spec delta.

## 2. Existing Spec Alignment

- [x] 2.1 Align `define-programmable-environment-product` with Agent Capability
  as a first-class Alan OS ability.
- [x] 2.2 Align `introduce-alan-kernel-runtime` so Kernel work defines semantic
  primitives without pulling Agent Capability Service execution into
  `alan-kernel`.
- [x] 2.3 Record cross-reference targets for relevant agent, governance, memory,
  sandbox, child-run, delegated-result, TUI, and macOS host specs when they are
  next touched.

## 3. Alan Agent Capability Migration Map

- [x] 3.1 Inventory existing Alan Agent capabilities: session/tape/turn loop,
  tool registry, skills, policy, approval, sandbox, memory, compaction, child
  agents, rollout, evidence, and conversation projection.
- [x] 3.2 Classify each capability as OS Primitive, Host Service
  Capability, Alan Agent App Feature, compatibility-only behavior, or rewrite
  candidate.
- [x] 3.3 Identify which existing implementation modules can be reused directly,
  adapted behind Host Service APIs, or rewritten because they conflict with the
  new model.

## 4. Follow-Up Implementation Planning

- [x] 4.1 Define the first Agent Capability Descriptor taxonomy for
  implementation, including explain, summarize, plan, transform, propose
  commands, delegate, and remember or a smaller first subset.
- [x] 4.2 Split a follow-up implementation change for Kernel semantic types:
  AgentCapabilityDescriptor, AgentRun identity, ContextGrant, ResultContract,
  EffectClass, CommandRisk, ExecutionGuard metadata, and audit shapes.
- [x] 4.3 Split a follow-up implementation change for Agent Capability Service as
  a compatibility Host Service API over the current Agent Execution Engine and
  daemon-backed session APIs.
- [x] 4.4 Split a follow-up implementation change for Alan Agent as the Agent
  Workspace over System Agent Supervisor and Agent Run projections.

## 5. Verification And Review

- [x] 5.1 Run `openspec validate define-agent-capability-os-model --strict`.
- [x] 5.2 Run `openspec validate --all --strict`.
- [x] 5.3 Review generated OpenSpec artifacts for terminology consistency with
  `CONTEXT.md`, README, AGENTS.md, and the existing Alan OS roadmap.
- [x] 5.4 Decide whether this model is ready to be accepted as a target contract
  or needs another grilling pass before implementation changes are split.
