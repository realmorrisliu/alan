# Standard Namespace Is Layered

> **Refined by [ADR-0024](0024-plan9-kernel-model.md), the canonical kernel model (also ADR-0025/0026).** Read retired terms accordingly: the kernel has a single `Process` category (no `Agent Process` kernel type — agent-ness is an `/agent` file-layout convention); Subscription, Agent Capability, Agent Run, Context Grant, and Result Contract are retired; Object/Buffer/View/Command/Query are app/service surfaces, not kernel primitives; the LLM provider tree is `/mnt/llm` with the handle posted at `/srv/llm`.

Alan OS should keep top-level namespace roots small and UNIX/Plan 9-like:
`/proc`, `/agent`, `/srv`, `/bin`, `/lib`, `/man`, and `/mnt`. `/proc`,
`/agent`, and `/srv` are live Kernel/service views; `/bin`, `/lib`, and `/man`
are command, package, and documentation roots; `/mnt` is where mounted service,
app, and data trees appear. Alan-specific package trees such as skills, tool
metadata, policy packages, and memory mounts should live under `/lib` or `/mnt`
instead of becoming new default top-level roots.
