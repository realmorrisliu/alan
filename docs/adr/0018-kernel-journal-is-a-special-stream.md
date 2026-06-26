# No Kernel Journal Primitive

> **Refined by [ADR-0024](0024-plan9-kernel-model.md), the canonical kernel model (also ADR-0025/0026).** Read retired terms accordingly: the kernel has a single `Process` category (no `Agent Process` kernel type — agent-ness is an `/agent` file-layout convention); Subscription, Agent Capability, Agent Run, Context Grant, and Result Contract are retired; Object/Buffer/View/Command/Query are app/service surfaces, not kernel primitives; the LLM provider tree is `/mnt/llm` with the handle posted at `/srv/llm`.

Alan Kernel should not model a Kernel-owned semantic journal. Services and apps
may expose named stream Files for activity, audit, recovery, replay, and
projection rebuilds, but those stream Files stay owned by the service or app
that understands the events. Kernel provides namespace/mounts, paths, Files,
Descriptors, Access Rights, Credentials, Processes, and the Process Table; it
does not become the system audit database or projection replay authority.
