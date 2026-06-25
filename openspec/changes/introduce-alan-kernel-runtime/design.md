## Context

`crates/tui` and the current Agent Execution Engine speak the existing session
protocol. ADR-0024 sets the target as a Plan 9 substrate plus an agent
file-layout convention. This change is the bridge: a user-space projection that
presents the current engine as agent-conforming process files, so the substrate
and the file-layout contract can be exercised against real runtime behavior
without rewriting the engine first.

The kernel ontology and the agent file convention are owned by
`define-plan9-kernel-substrate` and `define-agent-file-layout-contract`
respectively. This change owns only the compatibility projection.

## Goals / Non-Goals

**Goals:**

- Map current sessions, conversation, tape, yields, tool calls, and checkpoints
  onto `/proc`/`/agent` files per the agent file-layout contract.
- Keep `alan-kernel` free of protocol/provider/runtime dependencies; the
  projection is a user-space file server.
- Keep current `crates/tui` behavior working throughout migration.

**Non-Goals:**

- Define kernel ontology (owned by `define-plan9-kernel-substrate`).
- Define the agent file-layout convention (owned by
  `define-agent-file-layout-contract`).
- Build a "semantic view snapshot" renderer model (retired by ADR-0024).
- Implement a 9P wire transport, Service Manager, or real process isolation.

## Decisions

- The projection is a user-space file server above the substrate, not kernel
  code, and not a dependency of `alan-kernel`.
- Mappings follow ADR-0024 D2/D4/D7: tape → `machine/` (with the context window
  as a view over `machine/tape`), yields → `requests/`, tool calls → `actions/`
  linked to `/proc/<tool-pid>`, sessions → agent-conforming process directories.
- The current session protocol stays as compatibility transport behind the
  projection until file-surface parity holds.

## Risks / Trade-offs

- **R1 (ADR-0024): in v1 the projection runs in one address space**, so the
  namespace capability boundary is convention-enforced, not isolation-enforced.
- **Projection fidelity:** the mapping must not leak session-specific concepts
  upward; session ids stay internal runtime references, never kernel identity.

## Migration Plan

1. Land `define-plan9-kernel-substrate` and `define-agent-file-layout-contract`.
2. Implement the projection file server mapping current engine behavior onto the
   agent file surfaces.
3. Migrate `crates/tui` from the private session/application model toward reading
   agent files and writing `ctl`.
4. Retire the legacy transport once file parity is proven.
