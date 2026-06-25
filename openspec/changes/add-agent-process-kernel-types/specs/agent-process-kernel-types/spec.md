## ADDED Requirements

### Requirement: Kernel defines Agent Process anchors
Alan Kernel SHALL define minimal anchors for ordinary Processes and Agent
Processes: process identity, parent process identity, credentials, descriptors,
access rights, lifecycle state, status, exit state, namespace references, and
process-table entries.

#### Scenario: Agent Process is modeled
- **WHEN** an Agent Process is represented in Kernel
- **THEN** Kernel types can represent its process identity, parentage,
  credentials, open descriptors, access rights, lifecycle, status, and exit
  state
- **AND** those types do not require provider, session transport, sandbox,
  memory, Tool manifest, Skill, or renderer dependencies

### Requirement: Service mount anchors are Kernel-owned
Alan Kernel SHALL define path, mount, file, stream file, descriptor, access
right, standard namespace roots, and service-handle anchors needed for `/proc`,
`/agent`, `/srv`, `/bin`, `/lib`, `/man`, `/mnt`, service trees, and future
AgentFS attachment points.

#### Scenario: Service handle is represented
- **WHEN** a file-server service posts a handle under `/srv`
- **THEN** Kernel types can represent the mounted file tree and descriptors used
  to access it
- **AND** concrete service behavior remains outside Kernel

### Requirement: AgentFS schema remains above Kernel
Kernel Agent Process types SHALL NOT durably model AgentFS request/action/tape,
Tool manifest, Skill package, memory store, policy, or execution guard schemas.

#### Scenario: Dependency boundary is audited
- **WHEN** the Kernel crate dependencies are reviewed
- **THEN** Agent Process anchor types do not introduce dependencies on
  `alan-runtime`, `alan-protocol`, compatibility transport clients, provider
  clients, memory stores, sandbox implementations, Ratatui, SwiftUI, or Tokio
  task handles
