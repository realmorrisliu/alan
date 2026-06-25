# Agent Processes Are Request-Owned By Default

> **Refined by [ADR-0024](0024-plan9-kernel-model.md), the canonical kernel model (also ADR-0025/0026).** Read retired terms accordingly: the kernel has a single `Process` category (no `Agent Process` kernel type — agent-ness is an `/agent` file-layout convention); Subscription, Agent Capability, Agent Run, Context Grant, and Result Contract are retired; Object/Buffer/View/Command/Query are app/service surfaces, not kernel primitives; the LLM provider tree is `/mnt/llm` with the handle posted at `/srv/llm`.

Agent Processes should be owned by the app, shell, user, or parent Agent
Process that spawned them. Root Agent Process provides continuity for the agent
process tree, but it does not own every AI-mediated activity in Alan OS. This
keeps UPDF reading assistance, Groove Master practice help, Alan Shell agent
commands, and Alan Agent workspace tasks inside their product or process
semantics instead of making every agent action a root-owned global task.
