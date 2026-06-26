# Alan Kernel Uses A File-Tree UNIX Core

> **Refined by [ADR-0024](0024-plan9-kernel-model.md), the canonical kernel model (also ADR-0025/0026).** Read retired terms accordingly: the kernel has a single `Process` category (no `Agent Process` kernel type — agent-ness is an `/agent` file-layout convention); Subscription, Agent Capability, Agent Run, Context Grant, and Result Contract are retired; Object/Buffer/View/Command/Query are app/service surfaces, not kernel primitives; the LLM provider tree is `/mnt/llm` with the handle posted at `/srv/llm`.

Alan Kernel should center its ontology on Namespace/Mount, Path, File,
Descriptor, Access Rights, Credential, Process, and Process Table. Streams are
File kinds; process outputs are ordinary Files and stream Files. Capabilities,
Object, Task, Agent Runtime request/action files, Semantic View, Artifact,
Evidence, audit, and replay logs belong above Kernel as Agent/App/Service
descriptors or interpretations over those smaller file-tree primitives. This
keeps Alan OS close to UNIX's composable file, process, descriptor, credential,
and namespace model while still allowing apps and hosts to expose richer
semantic surfaces.
