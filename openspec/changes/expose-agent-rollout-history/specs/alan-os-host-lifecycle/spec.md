## ADDED Requirements

### Requirement: Alan OS Host owns fatal storage-integrity transitions
Alan OS Host SHALL provide Agent Runtime Service an internal fatal-transition
adapter during boot whose call is synchronously non-returning. On a fatal
storage-integrity call, the Host owner SHALL atomically close attachment and
new-work admission, mark the boot no longer ready in memory, request Service
Manager shutdown, and enter immediate fail-stop Host termination. The call
SHALL NOT return to Agent Runtime Service, Agent terminal finalization, Alan
Kernel, or its caller, and SHALL NOT await service shutdown, filesystem
cleanup, or the storage operation that triggered it.

The adapter SHALL NOT be exposed as a Host command, aP file, or renderer API.
Agent Runtime Service SHALL report the failure but SHALL NOT own attachment,
Service Manager, or whole-Host lifecycle. If the adapter cannot deliver its
internal shutdown signal, it SHALL abort the process instead of returning.

#### Scenario: Published Rollout containment reaches its absolute deadline
- **WHEN** Agent Runtime Service signals a fatal storage-integrity failure
  because a published Rollout inode could not be quarantined
- **THEN** the Alan OS Host owner closes attachment and new-work admission
- **AND** the adapter requests Service Manager shutdown and enters fail-stop
  termination without awaiting the stuck storage operation
- **AND** the adapter call never returns to Agent terminal finalization
- **AND** existing or new attachments cannot submit more work

#### Scenario: Internal shutdown signaling fails
- **WHEN** the Host-owned adapter cannot deliver its internal shutdown signal
- **THEN** it aborts the Host process without returning
- **AND** Alan OS does not continue in a ready state with uncontained storage
