# Agent Tool Governance Becomes Agent Action Governance

> **Refined by [ADR-0024](0024-plan9-kernel-model.md), the canonical kernel model (also ADR-0025/0026).** Read retired terms accordingly: the kernel has a single `Process` category (no `Agent Process` kernel type — agent-ness is an `/agent` file-layout convention); Subscription, Agent Capability, Agent Run, Context Grant, and Result Contract are retired; Object/Buffer/View/Command/Query are app/service surfaces, not kernel primitives; the LLM provider tree is `/mnt/llm` with the handle posted at `/srv/llm`.

Alan OS should not generalize the existing Alan Agent tool governance model into
governance for all app commands. The current allow, deny, escalate policy
engine, approval checkpoints, audit metadata, and sandbox backend ideas remain
valuable, but they are source material for Agent Action Governance inside Agent
Runtime Service and AgentFS action files: governing agent-proposed or autonomous
actions. Ordinary app commands should use lower-level Access Checks, host
consent, and app-domain rules unless they are being proposed or executed by an
Agent Process.
