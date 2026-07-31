## ADDED Requirements

### Requirement: Alan OS Host owns fatal storage-integrity transitions
Alan OS Host SHALL provide Agent Runtime Service an internal fatal-transition
adapter during boot. When that adapter accepts a fatal storage-integrity
signal, the Host owner SHALL atomically close attachment and new-work
admission, mark the boot no longer ready in memory, request Service Manager
shutdown, and terminate the Host process. Signal acceptance SHALL commit the
fail-stop transition and SHALL NOT await service shutdown, filesystem cleanup,
or the storage operation that triggered it.

The adapter SHALL NOT be exposed as a Host command, aP file, or renderer API.
Agent Runtime Service SHALL report the failure but SHALL NOT own attachment,
Service Manager, or whole-Host lifecycle. If the adapter is unavailable or
cannot accept its internal signal, the Host-owned adapter SHALL abort the
process rather than return and continue with an uncontained writer.

#### Scenario: Agent Runtime containment reaches its absolute deadline
- **WHEN** Agent Runtime Service signals a fatal storage-integrity failure
- **THEN** the Alan OS Host owner closes attachment and new-work admission
- **AND** it requests Service Manager shutdown and commits process termination
  without awaiting the stuck storage operation
- **AND** existing or new attachments cannot submit more work

#### Scenario: Fatal-transition adapter is unavailable
- **WHEN** Agent Runtime Service cannot deliver a required fatal-transition
  signal
- **THEN** the Host process aborts
- **AND** Alan OS does not continue in a ready state with uncontained storage
