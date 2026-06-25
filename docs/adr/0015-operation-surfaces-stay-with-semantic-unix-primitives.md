# Operation Surfaces Stay With Semantic UNIX Primitives

> **Refined by [ADR-0024](0024-plan9-kernel-model.md), the canonical kernel model (also ADR-0025/0026).** Read retired terms accordingly: the kernel has a single `Process` category (no `Agent Process` kernel type — agent-ness is an `/agent` file-layout convention); Subscription, Agent Capability, Agent Run, Context Grant, and Result Contract are retired; Object/Buffer/View/Command/Query are app/service surfaces, not kernel primitives; the LLM provider tree is `/mnt/llm` with the handle posted at `/srv/llm`.

Commands and queries should remain typed operation surfaces over paths, files,
descriptors, access rights, processes, and namespaces rather than becoming
independent Alan Kernel primitives: a command spawns a process or writes a file,
a query reads files or snapshots. Watching is a blocking read on a stream file;
Subscription is retired (ADR-0024 D8), not an operation surface, and there is no
subscription registry. V1 registries may index command and query descriptors for
discovery and compatibility, but durable semantics stay close to executable
files, read-only file inspection, process spawning, and stream-file reads so Alan
Kernel stays OS-shaped instead of app-framework-shaped.
