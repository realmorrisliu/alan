# Agent Runtime Service Is A File-Server Service

> **Refined by [ADR-0024](0024-plan9-kernel-model.md), the canonical kernel model (also ADR-0025/0026).** Read retired terms accordingly: the kernel has a single `Process` category (no `Agent Process` kernel type — agent-ness is an `/agent` file-layout convention); Subscription, Agent Capability, Agent Run, Context Grant, and Result Contract are retired; Object/Buffer/View/Command/Query are app/service surfaces, not kernel primitives; the LLM provider tree is `/mnt/llm` with the handle posted at `/srv/llm`.

Agent Runtime Service should be a Plan 9-style file-server Process managed by
Service Manager. It posts a service handle under `/srv`, serves AgentFS at
`/agent`, and executes Agent Processes. Starting, inspecting, steering,
scheduling, streaming, yielding, and completing agent work should be expressed
through AgentFS files, descriptors, and process state. The current HTTP/WS
session server remains compatibility transport while clients migrate; it is not
the target OS boundary.
