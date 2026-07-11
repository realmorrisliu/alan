## ADDED Requirements

### Requirement: Memory surfaces consume store-owned write state
Generated recall, handoff, Episodic Memory, and daily-note surfaces SHALL read
current memory through namespace-mounted Memory Store files. They SHALL keep
write/evidence references bounded, SHALL NOT duplicate the full ledger, and
SHALL NOT reintroduce content whose store-owned write state is reverted.

#### Scenario: A recalled fact came from proactive promotion
- **WHEN** runtime includes the fact in prompt-facing memory
- **THEN** it may include a bounded namespace reference to the owning store's
  ledger or evidence
- **AND** it does not copy the complete ledger record into the prompt

#### Scenario: A write was reverted
- **WHEN** the current store tree or ledger state marks a write reverted
- **THEN** recall, handoff, Episodic Memory, and daily-note surfaces exclude the
  reverted fact as current memory

### Requirement: Channel-scoped workspace memory backs the Workspace Memory Store
The current `.alan/runtime/<channel>/memory/` workspace layout SHALL be treated
as the backing storage of a Workspace Memory Store adapter. Agent-facing
references SHALL use mounted namespace paths under `/mnt/mem` or passed
descriptors rather than raw host workspace paths.

#### Scenario: Workspace memory is loaded
- **WHEN** a workspace stores pure-text memory under its active channel root
- **THEN** the adapter projects it through the authorized
  Workspace Memory Store tree
- **AND** callers do not infer Personal or System-Continuity authority from
  workspace filenames
