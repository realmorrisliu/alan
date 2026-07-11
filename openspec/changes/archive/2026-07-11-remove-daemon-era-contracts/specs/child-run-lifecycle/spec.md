## MODIFIED Requirements

### Requirement: Child Run Registration
Alan SHALL register each delegated child as a child Agent Process before its initial turn. The
record SHALL contain parent and child Process paths or pids, workspace and namespace metadata,
rollout/checkpoint path when available, launch metadata, creation time, and starting or running
status. The record SHALL be a projection of `/proc` and `/agent` truth and SHALL identify execution
only through those Process and durable-evidence owners.

#### Scenario: Child registration precedes initial input
- **WHEN** a parent Agent Process spawns a child Agent Executable
- **THEN** the child record is visible before the initial turn is delivered
- **AND** parent/child identity resolves to concrete Process and AgentFS paths

#### Scenario: Child launch fails after allocation
- **WHEN** launch fails after a child pid, Process path, or rollout/checkpoint path is known
- **THEN** the child record reaches a terminal failure state with that evidence
- **AND** the allocated Process record does not remain in a starting or running state
