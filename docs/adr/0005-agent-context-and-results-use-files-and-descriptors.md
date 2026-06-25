# Agent Context And Results Use Files And Descriptors

> **Refined by [ADR-0024](0024-plan9-kernel-model.md), the canonical kernel model (also ADR-0025/0026).** Read retired terms accordingly: the kernel has a single `Process` category (no `Agent Process` kernel type — agent-ness is an `/agent` file-layout convention); Subscription, Agent Capability, Agent Run, Context Grant, and Result Contract are retired; Object/Buffer/View/Command/Query are app/service surfaces, not kernel primitives; the LLM provider tree is `/mnt/llm` with the handle posted at `/srv/llm`.

Agent context and results should be passed through files, directories,
descriptors, stream files, and app/service-owned file trees rather than through
RPC request/response contracts. Kernel provides paths, descriptors, access
rights, credentials, namespaces, mounts, and a single `Process` category
(agent-ness is a file-layout/AgentFS convention above the kernel; ADR-0024 D3);
Agent Runtime Service and apps define the concrete request, action, context,
result, evidence, and audit files above that substrate.
