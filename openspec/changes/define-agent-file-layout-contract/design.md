## Context

With `Agent Process` removed from the kernel (ADR-0024 D3), "is this an agent?"
becomes "does this process directory conform to the agent layout?" — filesystem
duck typing. This change defines that layout as an OS-level contract so uniform
tooling works without a kernel agent type, the same way Plan 9 operates diverse
resources through directory conventions and `ctl` files.

## Goals / Non-Goals

**Goals:**

- Define the process layout and the agent superset as a conformance contract.
- Define `ctl`-based control and `/agent` as an overlay over `/proc`.
- Define the LLM-stream consumer model and namespace-assembled requests.
- Define requests/responses as files and the durable home tree.
- Define the reference boundary for LLM provider, memory, tool, and skill file
  servers consumed by the agent layout.

**Non-Goals:**

- Specify the full LLM provider, memory, tool, or skill file-server protocols
  (separate changes); this contract only names their mount boundaries and how an
  agent consumes them.
- Specify wire formats for any provider (provider-local per ADR-0024 D2).
- Re-introduce any retired noun (Session, Workspace, AgentInstance,
  Subscription, Context Grant, Result Contract).

## Decisions

Implements [ADR-0024](../../../docs/adr/0024-plan9-kernel-model.md):

- D1 → LLM is a typed stream the process consumes; effects are namespace-governed.
- D2 → request assembled from namespace files; compaction is a view over
  `machine/tape`; tools are the `/bin` visible in the namespace.
- D4 → process layout + agent superset; `ctl` control; `/agent` as a `/proc`
  overlay.
- D6 (agent-facing) → metering/cost live in the provider file server, reached
  only if bound into the namespace.
- D7 (agent-facing) → durable identity is a home tree; pid is ephemeral; durable
  vs ephemeral is decided by where the home is mounted.
- D8 (agent-facing) → dynamic containers (`requests/`, `actions/`) expose an
  events stream watched by blocking read.
- ADR-0025 D3 → external capability trees are reached through named file-server
  mounts: `/mnt/llm`, `/mnt/mem`, `/bin` plus `/lib/exec` and `/man/1`, and
  `/lib/skill` plus `/man/skill`.

## Risks / Trade-offs

- **R1 (from ADR-0024): in v1 conformance and namespace scoping are
  convention-enforced, not isolation-enforced.** A claim that this contract
  governs a semi-adversarial LLM must restate that real enforcement awaits the
  cross-process transport slice.
- **Contract drift:** because conformance is a published convention rather than a
  kernel type, a non-conforming runtime silently breaks tools. The contract is
  the enforcement; conformance is each runtime's responsibility (this is also
  what makes third-party runtimes pluggable).

## Migration Plan

1. Land this contract above `define-plan9-kernel-substrate`.
2. Map existing session / tape / yield / tool-call behavior onto this layout in
   the agent-runtime implementation change (a separate change), keeping current
   transport as compatibility only.
3. Keep the detailed LLM, memory, tool, and skill protocols in their owning
   OpenSpec capabilities while this contract remains the shared consumer
   boundary.
