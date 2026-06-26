# Agent Runtime Concepts Migrate By OS Class

> **Refined by [ADR-0024](0024-plan9-kernel-model.md), the canonical kernel model (also ADR-0025/0026).** Read retired terms accordingly: the kernel has a single `Process` category (no `Agent Process` kernel type — agent-ness is an `/agent` file-layout convention); Subscription, Agent Capability, Agent Run, Context Grant, and Result Contract are retired; Object/Buffer/View/Command/Query are app/service surfaces, not kernel primitives; the LLM provider tree is `/mnt/llm` with the handle posted at `/srv/llm`.

Existing Alan Agent runtime concepts should be classified before migration as
Kernel primitives, Agent Runtime Service behavior, AgentFS files, Alan Agent
workspace features, Tool/Skill packages, Memory Store files, compatibility
transport, or rewrite candidates. This keeps useful existing session, tool,
policy, sandbox, memory, child-agent, rollout, and conversation work while
preventing every runtime detail from becoming Alan Kernel.
