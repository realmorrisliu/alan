# Agent Action Auto-Run Depends On Governability

> **Refined by [ADR-0024](0024-plan9-kernel-model.md), the canonical kernel model (also ADR-0025/0026).** Read retired terms accordingly: the kernel has a single `Process` category (no `Agent Process` kernel type — agent-ness is an `/agent` file-layout convention); Subscription, Agent Capability, Agent Run, Context Grant, and Result Contract are retired; Object/Buffer/View/Command/Query are app/service surfaces, not kernel primitives; the LLM provider tree is `/mnt/llm` with the handle posted at `/srv/llm`.

Agent Runtime Service should decide automatic execution of agent-proposed or
autonomous actions from governability, not from a coarse read/write split alone.
An Agent Process action may run automatically only when policy, effect class,
target scope, reversibility, execution guard strength, and auditability support
it; high-risk effects such as delete, publish, irreversible modify, privilege
escalation, cross-app writes, and opaque shell/process work without strong
confinement must require approval or denial.
