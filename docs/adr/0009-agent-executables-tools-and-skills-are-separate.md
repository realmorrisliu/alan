# Agent Executables, Tools, And Skills Are Separate

> **Refined by [ADR-0024](0024-plan9-kernel-model.md), the canonical kernel model (also ADR-0025/0026).** Read retired terms accordingly: the kernel has a single `Process` category (no `Agent Process` kernel type — agent-ness is an `/agent` file-layout convention); Subscription, Agent Capability, Agent Run, Context Grant, and Result Contract are retired; Object/Buffer/View/Command/Query are app/service surfaces, not kernel primitives; the LLM provider tree is `/mnt/llm` with the handle posted at `/srv/llm`.

Agent work should not be modeled as a list of RPC-style agent API methods.
An Agent Executable is a command that spawns an Agent Process. A Tool is an
external executable command in `/bin` or bound into `/bin`, with `--help`,
`/man/1/<tool>`, and `/lib/exec/<tool>/manifest`. A Skill is a manual-like
knowledge package under `/lib/skill/<name>` and `/man/skill/<name>`. This split
keeps process creation, external action, and instructional knowledge legible in
UNIX terms while keeping Alan-specific package trees out of top-level roots.
