## Context

ADR-0026 D2 adopts Plumber's content/type rule routing for agents, with the
caveat that decoupled routing must stay auditable. `plumbfs` is a user-space aP
file server: it fits the model with no new mechanism (send file, rule files,
port streams).

## Goals / Non-Goals

**Goals:**

- Typed messages routed by content/type rules to ports, sender decoupled from
  receiver.
- Decoupled handoff and human-in-the-loop governance routing.
- Inspectable rules and an observable message log.

**Non-Goals:**

- Become the primary control path (it is a composition mechanism).
- Define agent execution, the LLM, or the file-layout contract (separate).
- Implement a wire transport (in-process v1).

## Decisions

Implements [ADR-0026](../../../docs/adr/0026-plan9-application-ideas-for-agents.md)
D2.

- **Send / rules / ports.** A sender writes a typed message to `send`. Rule files
  match by content/type and route to a destination port. A receiver tails its
  port (a D8 blocking-read stream).
- **Decoupled.** The sender does not name a receiver; rules decide. Handoff is
  "emit type T", not "call actor X".
- **Auditable by construction.** Every message is appended to an observable log
  stream; rules are plain files; nothing routes invisibly.
- **Composition, not control.** Plumbing composes actors; it does not replace an
  agent's own loop or governance. Critical control stays explicit.

## Risks / Trade-offs

- **Hidden control flow.** Rule routing can obscure who handled what; mitigated by
  the message log and `cat`-able rules. If a flow needs to be obvious, do not
  plumb it.
- **Rule conflicts / no match.** Define deterministic match order and a default
  (dead-letter) port so messages are never silently dropped.

## Migration Plan

1. Land `plumbfs` with send/rules/ports and the message log.
2. Use it first for human-in-the-loop governance (results plumb to a human port).
3. Then for agent→tool/agent handoff by type.
