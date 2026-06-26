## ADDED Requirements

### Requirement: Kernel ontology is owned by the Plan 9 substrate
This change SHALL NOT own the Alan Kernel ontology. The durable kernel contract
is defined by `define-plan9-kernel-substrate` and anchored by
[ADR-0024](../../../../docs/adr/0024-plan9-kernel-model.md). The earlier
ontology this spec carried — a first-class `Agent Process` kernel category,
typed opaque ids as runtime references, an Activity Ledger / Kernel Journal, and
Object / Buffer / View / Command / Query / Subscription / Task / Artifact /
Evidence kernel surfaces — is retired and SHALL NOT be implemented.

Implementation work in `introduce-alan-kernel-runtime` SHALL target the
substrate contract (namespace engine, wire-shaped file-server contract, single
`Process` category, process table, `/proc`, `/srv`) and the agent file-layout
convention defined by `define-agent-file-layout-contract`.

#### Scenario: A reader looks here for the kernel contract
- **WHEN** a reader opens this spec for the kernel ontology
- **THEN** it is directed to `define-plan9-kernel-substrate` and ADR-0024 as the
  source of truth
- **AND** no kernel concept is defined or duplicated here

#### Scenario: The retired ontology is referenced by old work
- **WHEN** existing tasks, code, or notes reference `Agent Process` as a kernel
  type, global opaque ids, a Kernel Journal, or kernel-level Object/View/Command/
  Query/Subscription/Task/Artifact/Evidence types
- **THEN** they are treated as superseded and remapped onto the substrate plus
  the agent file-layout convention
- **AND** they are not carried forward as kernel ontology

#### Scenario: Adapter and renderer-host contracts remain
- **WHEN** the `alan-agent-adapter-contract` and `alan-renderer-host-contract`
  capabilities of this change are reviewed
- **THEN** they remain valid above the substrate as adapter and host surfaces
- **AND** they reference the substrate's files, processes, streams, and
  namespaces rather than the retired kernel ontology
