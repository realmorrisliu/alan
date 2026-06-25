# Streams Are File Kinds

> **Refined by [ADR-0024](0024-plan9-kernel-model.md), the canonical kernel model (also ADR-0025/0026).** Read retired terms accordingly: the kernel has a single `Process` category (no `Agent Process` kernel type — agent-ness is an `/agent` file-layout convention); Subscription, Agent Capability, Agent Run, Context Grant, and Result Contract are retired; Object/Buffer/View/Command/Query are app/service surfaces, not kernel primitives; the LLM provider tree is `/mnt/llm` with the handle posted at `/srv/llm`.

Streams in Alan Kernel should be File kinds that can be read, tailed, and
resumed from offsets rather than internal event transport hidden behind
subscriptions. Watching is a blocking read on a stream File (`tail -f`
semantics); Subscription is retired as a concept (ADR-0024 D8), not a separate
watch surface. Stream Files provide the UNIX-like pipe/log substrate for replay,
Agent/App evidence interpretation, host recovery, and cross-app observation
without becoming a separate Kernel primitive.
