## ADDED Requirements

### Requirement: Memory surfaces consume store-owned write state
Generated recall, handoff, session-summary, and daily-note surfaces SHALL read
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
- **THEN** recall, handoff, session-summary, and daily-note surfaces exclude the
  reverted fact as current memory

### Requirement: Legacy workspace memory is a compatibility store
The current `.alan/memory/` workspace layout SHALL be treated as the backing
storage of a Workspace Memory Store compatibility adapter. Agent-facing
references SHALL use mounted namespace paths under `/mnt/mem` or passed
descriptors rather than raw host workspace paths.

#### Scenario: Existing workspace memory is loaded
- **WHEN** a workspace still stores pure-text memory under `.alan/memory/`
- **THEN** the compatibility adapter projects it through the authorized
  Workspace Memory Store tree
- **AND** callers do not infer Personal or System-Continuity authority from
  compatibility filenames
