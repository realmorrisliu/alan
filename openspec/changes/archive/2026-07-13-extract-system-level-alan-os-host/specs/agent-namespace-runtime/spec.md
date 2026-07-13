## ADDED Requirements

### Requirement: System composition belongs to Alan OS Host
Agent Execution Engine SHALL execute Agent Process transitions behind AgentFS
and MUST NOT create the system Kernel, `/srv`, system Root Agent role, Host
endpoint, or System Store root. During this change only, Alan OS Host MAY use a
fixed internal boot composition that the Service Manager change MUST delete.

#### Scenario: Engine is started for an Agent Process
- **WHEN** the Host composition starts Agent execution
- **THEN** the engine receives an assembled Process namespace and descriptors
- **AND** it does not construct a competing system root
