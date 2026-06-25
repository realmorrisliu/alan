## 1. Model Definition

- [x] 1.1 Capture Agent Process, Root Agent Process, Service Manager,
  file-server service, `/srv`, Agent Runtime Service, AgentFS, Agent
  Executable, Tool, Skill, Agent Request, Agent Action, Agent IO, Agent
  Machine, and descriptor-passed memory/policy terms in the repository glossary.
- [x] 1.2 Record ADRs for the core Plan 9-style agent process decisions.
- [x] 1.3 Create the OpenSpec proposal, design, taxonomy, migration map, and
  `agent-process-os-model` spec delta.

## 2. Existing Spec Alignment

- [x] 2.1 Align `define-programmable-environment-product` with Alan OS as a
  Plan 9-style file/process/service substrate.
- [x] 2.2 Align `introduce-alan-kernel-runtime` so Kernel work defines Process,
  Agent Process, file, descriptor, namespace, and service mount primitives
  without pulling agent runtime execution into `alan-kernel`.
- [x] 2.3 Align `migrate-alan-agent-to-agent-workspace` so Alan Agent is built
  in but optional, while Alan Shell remains the primary OS interaction surface.

## 3. Alan Agent Runtime Migration Map

- [x] 3.1 Inventory existing Alan Agent concepts: session, tape, turn loop, tool
  registry, skills, policy, approval, sandbox, memory, compaction, child agents,
  rollout, evidence, and conversation projection.
- [x] 3.2 Classify each concept as Kernel primitive, Agent Runtime Service
  behavior, AgentFS surface, Tool executable, Skill package, Memory Store,
  policy descriptor, optional Alan Agent workspace UI, compatibility transport,
  or rewrite candidate.
- [x] 3.3 Identify which implementation modules can be reused directly, adapted
  behind file-server services, or rewritten because they conflict with the
  Plan 9-style model.

## 4. Follow-Up Implementation Planning

- [x] 4.1 Define the first Agent Executable / Tool / Skill taxonomy for
  implementation.
- [x] 4.2 Reframe the Kernel follow-up around Agent Process identity,
  descriptors, file trees, process table entries, namespace mounts, `/proc`,
  `/srv`, and AgentFS anchors.
- [x] 4.3 Reframe the former service-adapter follow-up as Agent Runtime Service
  and compatibility transport work over the current Agent Execution Engine.
- [x] 4.4 Reframe the Alan Agent follow-up as optional workspace UI over
  `/agent`, `/proc`, `/lib/skill`, `/man`, `/mnt/mem`, `/mnt/policy`, and
  request/action files.

## 5. Verification And Review

- [x] 5.1 Run `openspec validate define-agent-process-os-model --strict`.
- [x] 5.2 Run `openspec validate --all --strict`.
- [x] 5.3 Review generated OpenSpec artifacts for terminology consistency with
  `CONTEXT.md`, README, AGENTS.md, and the Alan OS roadmap.
- [x] 5.4 Confirm Agent Capability, Agent Run, Context Grant, Result Contract,
  and daemon-shaped language remain only as compatibility/legacy terminology.
