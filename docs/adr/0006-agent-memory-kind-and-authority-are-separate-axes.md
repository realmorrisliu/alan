# Agent Memory Kind And Authority Are Separate Axes

> **Refined by [ADR-0024](0024-plan9-kernel-model.md), the canonical kernel model (also ADR-0025/0026).** Read retired terms accordingly: the kernel has a single `Process` category (no `Agent Process` kernel type — agent-ness is an `/agent` file-layout convention); Subscription, Agent Capability, Agent Run, Context Grant, and Result Contract are retired; Object/Buffer/View/Command/Query are app/service surfaces, not kernel primitives; the LLM provider tree is `/mnt/llm` with the handle posted at `/srv/llm`.

Agent memory in Alan OS should not be modeled as User Memory, System Memory, and
App Memory as if those were memory kinds. Working, episodic, semantic, and
procedural describe how an agent uses memory; personal, system-continuity, app,
and workspace Memory Stores describe who owns and authorizes the memory file
tree. Agent Processes receive Memory Stores through descriptors, so Root Agent
Process can have continuity without turning app-private history into a global
agent brain.
