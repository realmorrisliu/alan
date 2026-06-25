# Root Agent Authority Is Governed

> **Refined by [ADR-0024](0024-plan9-kernel-model.md), the canonical kernel model (also ADR-0025/0026).** Read retired terms accordingly: the kernel has a single `Process` category (no `Agent Process` kernel type — agent-ness is an `/agent` file-layout convention); Subscription, Agent Capability, Agent Run, Context Grant, and Result Contract are retired; Object/Buffer/View/Command/Query are app/service surfaces, not kernel primitives; the LLM provider tree is `/mnt/llm` with the handle posted at `/srv/llm`.

The Root Agent may maintain broad system awareness and propose
cross-app help, but app-private reads and side effects must still flow through
permission, command, and audit paths. This keeps Alan OS feeling intelligent at
the system level without granting the resident agent unrestricted root-style
automation power over every app.
