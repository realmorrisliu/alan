# Alan Agent Is The Agent Workspace, Not The Root Agent

> **Refined by [ADR-0024](0024-plan9-kernel-model.md), the canonical kernel model (also ADR-0025/0026).** Read retired terms accordingly: the kernel has a single `Process` category (no `Agent Process` kernel type — agent-ness is an `/agent` file-layout convention); Subscription, Agent Capability, Agent Run, Context Grant, and Result Contract are retired; Object/Buffer/View/Command/Query are app/service surfaces, not kernel primitives; the LLM provider tree is `/mnt/llm` with the handle posted at `/srv/llm`.

Alan Agent should be the built-in user-visible Agent Workspace for inspecting,
steering, and organizing agent work, while the Root Agent remains
the always-available system intelligence layer. Other Alan Apps start agent work
the same way — by spawning Agent Executables and reading/writing AgentFS files —
without routing through the Alan Agent UI (Agent Capability / Agent Run / Context
Grant APIs are retired; ADR-0024). Alan Agent can surface root-agent-raised
tasks, memory, evidence, and cross-app continuity.
