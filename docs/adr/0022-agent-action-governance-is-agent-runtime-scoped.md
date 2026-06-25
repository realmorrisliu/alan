# Agent Action Governance Is Agent Runtime Scoped

> **Refined by [ADR-0024](0024-plan9-kernel-model.md), the canonical kernel model (also ADR-0025/0026).** Read retired terms accordingly: the kernel has a single `Process` category (no `Agent Process` kernel type — agent-ness is an `/agent` file-layout convention); Subscription, Agent Capability, Agent Run, Context Grant, and Result Contract are retired; Object/Buffer/View/Command/Query are app/service surfaces, not kernel primitives; the LLM provider tree is `/mnt/llm` with the handle posted at `/srv/llm`.

Effect classes, risk scoring, approval checkpoints, auto-execution policy, and
execution guard metadata should be scoped to Agent Runtime Service, AgentFS
action files, and Alan Agent compatibility/workspace paths. Kernel should only
expose Paths, Files, stream Files, Descriptors, Access Rights, Credentials, a
single `Process` category (agent-ness is an `/agent` file-layout convention above
the kernel; ADR-0024 D3), Access Checks, namespaces, and mounts. Other Alan
Apps may use Access Checks and host consent services, but they should not be
forced to model ordinary user actions through agent action risk.
