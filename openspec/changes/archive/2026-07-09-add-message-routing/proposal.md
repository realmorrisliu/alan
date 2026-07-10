## Why

ADR-0024 Q6 left cross-actor collaboration as "shared mount points" — passive.
ADR-0026 D2 answers how agents, tools, and apps communicate without point-to-point
coupling: typed messages routed by rules, where the sender does not name the
receiver. (The idea is borrowed from Plan 9's Plumber; the design and naming here
are Alan's own.) This is the decoupled-handoff and human-in-the-loop-governance
mechanism the agent OS otherwise lacks.

## What Changes

- Add `routefs`, a file server: a sender writes a typed message to a `send` file;
  rule files match the message by content/type and route it to a destination port
  (a stream the receiver tails).
- Make handoff content-based, not hardcoded: an agent emits "a patch", "a
  citation", or "a task to approve" and rules dispatch it (to a review agent, an
  apply-patch tool, or a human inbox port) — no agent names another agent.
- Make governance routing a first-class use: results needing human judgment route
  to a human port; the rules that decide this are inspectable text files.
- Keep it auditable: every routed message is logged to an observable stream, rules
  are `cat`-able, and routing is a composition mechanism, not the primary control
  path.

## Capabilities

### New Capabilities

- `message-routing`: `routefs` — typed messages written to `send`, content/type
  rule files, destination ports as streams, an observable message log, and
  decoupled content-based routing for agent/tool/app handoff and human-in-the-loop
  governance.

### Modified Capabilities

- None.

## Impact

- A user-space file server (ADR-0024 D9) over the aP protocol; ports are
  blocking-read streams (D8). Depends on `define-plan9-kernel-substrate`.
- Gives agents decoupled handoff and a clean human-in-the-loop routing surface
  without an orchestrator monolith.
- ADRs: implements ADR-0026 D2.
