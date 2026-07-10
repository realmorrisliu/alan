## MODIFIED Requirements

### Requirement: Child Run Registration
The system SHALL create a child-run record before submitting the first operation to a delegated child runtime.

#### Scenario: Delegated child is launched
- **WHEN** a parent runtime launches a delegated child runtime
- **THEN** the child-run registry contains a record with parent session id, child session id, workspace metadata, rollout path when available, launch metadata, created time, and `starting` or `running` status before the child receives its initial turn
- **AND** the launch metadata includes a bounded summary of the capability-bearing mounts and `/bin` bindings the child was spawned with, so capability investigations do not require the child process to still be alive

#### Scenario: Child launch fails after runtime startup
- **WHEN** child launch fails after a child session id or rollout path is known
- **THEN** the child-run record is updated to `failed` with terminal metadata instead of disappearing from the registry
