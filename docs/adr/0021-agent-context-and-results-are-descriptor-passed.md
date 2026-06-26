# Agent Context And Results Are Descriptor-Passed

> **Refined by [ADR-0024](0024-plan9-kernel-model.md), the canonical kernel model (also ADR-0025/0026).** Read retired terms accordingly: the kernel has a single `Process` category (no `Agent Process` kernel type — agent-ness is an `/agent` file-layout convention); Subscription, Agent Capability, Agent Run, Context Grant, and Result Contract are retired; Object/Buffer/View/Command/Query are app/service surfaces, not kernel primitives; the LLM provider tree is `/mnt/llm` with the handle posted at `/srv/llm`.

Agent context and results should be descriptor-passed rather than API-contract
passed. An app, shell, user, or parent Agent Process opens bounded files,
directories, streams, Memory Stores, Skills, policy files, or app service
trees, then spawns an Agent Executable with those descriptors. Agent Runtime
Service projects the running work through AgentFS request, action, io, and
machine files; results are conveyed via `io/output` and per-action
`actions/<id>/result`, not a top-level `result` file. Kernel remains responsible
only for files, descriptors, access rights, credentials, namespaces, mounts, and
a single `Process` identity.
