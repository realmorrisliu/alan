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

**Non-Goals:**

- Specify the LLM provider, memory, tool, or skill file servers themselves
  (separate changes); this contract only references how an agent consumes them.
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
