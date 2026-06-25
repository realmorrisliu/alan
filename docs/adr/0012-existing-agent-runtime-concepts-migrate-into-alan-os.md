# Existing Agent Runtime Concepts Migrate Into Alan OS

> **Refined by [ADR-0024](0024-plan9-kernel-model.md), the canonical kernel model (also ADR-0025/0026).** Read retired terms accordingly: the kernel has a single `Process` category (no `Agent Process` kernel type — agent-ness is an `/agent` file-layout convention); Subscription, Agent Capability, Agent Run, Context Grant, and Result Contract are retired; Object/Buffer/View/Command/Query are app/service surfaces, not kernel primitives; the LLM provider tree is `/mnt/llm` with the handle posted at `/srv/llm`.

Existing Alan Agent runtime concepts should be migrated into Alan OS where they
fit the new file/process model, not discarded in favor of a greenfield agent
system. Tape, turn execution, policy, sandbox ideas, memory, skills, child
agent orchestration, rollout persistence, and session compatibility should be
preserved or adapted when they map cleanly to Agent Runtime Service, AgentFS,
Agent Processes, Tools, Skills, descriptors, Memory Stores, or compatibility
transport. Concepts that conflict with Process/Agent Process, Service Manager,
file-server services, or descriptor-passed context should be reshaped.
