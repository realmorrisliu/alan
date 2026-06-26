# Root Agent Is Not A Session

> **Refined by [ADR-0024](0024-plan9-kernel-model.md), the canonical kernel model (also ADR-0025/0026).** Read retired terms accordingly: the kernel has a single `Process` category (no `Agent Process` kernel type — agent-ness is an `/agent` file-layout convention); Subscription, Agent Capability, Agent Run, Context Grant, and Result Contract are retired; Object/Buffer/View/Command/Query are app/service surfaces, not kernel primitives; the LLM provider tree is `/mnt/llm` with the handle posted at `/srv/llm`.

Alan OS should have an always-available Root Agent for long-lived
identity, memory, system awareness, and cross-app continuity, but actual model
reasoning and action should happen in bounded Agent Processes. This preserves
the feeling of a resident system intelligence without turning Alan OS into one
unbounded root conversation that leaks context, permissions, audit trails, and
product boundaries across apps.
