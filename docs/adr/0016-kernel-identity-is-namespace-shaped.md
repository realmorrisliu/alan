# Kernel Identity Is Namespace-Shaped

> **Refined by [ADR-0024](0024-plan9-kernel-model.md), the canonical kernel model (also ADR-0025/0026).** Read retired terms accordingly: the kernel has a single `Process` category (no `Agent Process` kernel type — agent-ness is an `/agent` file-layout convention); Subscription, Agent Capability, Agent Run, Context Grant, and Result Contract are retired; Object/Buffer/View/Command/Query are app/service surfaces, not kernel primitives; the LLM provider tree is `/mnt/llm` with the handle posted at `/srv/llm`.

Alan Kernel should use namespace-qualified Paths, Process Table entries, and
mounted file trees as canonical semantic identity, while typed opaque ids remain
runtime references for projections, caches, compatibility surfaces, and
in-flight state. This keeps Alan OS close to a filesystem, process table, and
mount tree, and prevents the Kernel from becoming an object database whose UUIDs
are treated as authority.
