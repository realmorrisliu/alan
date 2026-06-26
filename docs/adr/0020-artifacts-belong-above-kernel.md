# Artifacts Belong Above Kernel

> **Refined by [ADR-0024](0024-plan9-kernel-model.md), the canonical kernel model (also ADR-0025/0026).** Read retired terms accordingly: the kernel has a single `Process` category (no `Agent Process` kernel type — agent-ness is an `/agent` file-layout convention); Subscription, Agent Capability, Agent Run, Context Grant, and Result Contract are retired; Object/Buffer/View/Command/Query are app/service surfaces, not kernel primitives; the LLM provider tree is `/mnt/llm` with the handle posted at `/srv/llm`.

Alan Kernel should not model Artifact as a primitive. Kernel provides output
Files created or written by Processes, stream Files emitted by Processes,
service-owned stream Files, and native selectors; Alan Agent, Agent Runtime
Service, or domain apps may interpret those outputs as artifacts for
presentation, sharing, review, compatibility, or evidence.
