# Evidence Belongs Above Kernel

> **Refined by [ADR-0024](0024-plan9-kernel-model.md), the canonical kernel model (also ADR-0025/0026).** Read retired terms accordingly: the kernel has a single `Process` category (no `Agent Process` kernel type — agent-ness is an `/agent` file-layout convention); Subscription, Agent Capability, Agent Run, Context Grant, and Result Contract are retired; Object/Buffer/View/Command/Query are app/service surfaces, not kernel primitives; the LLM provider tree is `/mnt/llm` with the handle posted at `/srv/llm`.

Alan Kernel should not model Evidence or ProvenanceRef as a primitive. Kernel
provides paths, Files, Descriptors, Access Rights, Credentials, Process Table
entries, stream offsets, service-owned stream Files, and native selectors; Alan
Agent, Agent Runtime Service, or domain apps may interpret those primitives as
evidence when supporting a claim, result, memory, command proposal, action, or
decision.
