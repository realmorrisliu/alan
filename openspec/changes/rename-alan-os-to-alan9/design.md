# Design

## Context

See proposal.md for motivation. `product-brand-identity` already owns product
spelling and identifier exceptions. ADR-0057 records the naming decision; the
current repository still uses `Alan OS` throughout active prose and identifiers.

## Goals / Non-Goals

**Goals:** make product, system, Kernel and Host references unambiguous with one
vocabulary table and a scoped prose migration.

**Non-Goals:** executable/crate renames, storage migration, aP changes, service
ownership changes, TUI/Shell unification or model-assisted dispatch.

## Decisions

### Product and system have separate names

| Existing concept | Adopted name |
| --- | --- |
| Product | Alan |
| Alan OS | alan9 |
| Alan Kernel | alan9 Kernel |
| Alan OS Host | alan9 Host |
| User entry | `alan` |
| File-native Shell | Alan Shell |

Service Manager, Agent Runtime Service, Agent Process, Package Service and
Quartermaster retain their role names. `alan9` names the existing system scope;
it introduces no new component. Renaming the complete product would discard the
useful product/system distinction. Retaining `Alan OS` would leave the naming
ambiguity this change addresses.

### Align prose without renaming machine identifiers

Update active explanatory prose in README, AGENTS.md, current guides and
OpenSpec. Preserve literal crate/type/command names, paths, links and capability
IDs such as `alan-os-host-lifecycle`. Earlier ADRs and immutable archived changes
remain evidence of their original decisions; link the new ADR from the docs index.
Historical quotations and migration explanations may explicitly say `Alan OS`.
Do not use a blind global replacement or create compatibility aliases.

Brand validation must distinguish lowercase system branding from the existing
capitalized Alan product brand. Reuse its existing mechanism if a change is
needed; do not introduce another checker solely for this rename.

### Interaction design keeps its current owners

The naming change has no input-routing delta. The interaction change records
the unified-input exploration; cognition owns any future typed evaluation and
governed command transition. ADR-0056 remains the implemented entry contract
until a separately accepted behavior change replaces it.

## Risks / Trade-offs

- Readers infer 9P compatibility → describe aP explicitly when explaining the name.
- Active prose and old identifiers differ → document the exact mapping and retain
  working links; machine identifiers are not user-facing system branding.
- Historical plans look newly approved → preserve dispositions and implementation
  status while editing terminology.

## Migration Plan

Record this accepted decision and delta first. Then inventory and align active
prose, review identifier exclusions, and validate OpenSpec and existing quality
gates. After implementation and merge, sync the delta and archive the change.
Rollback is a prose revert; there is no data or runtime migration.
