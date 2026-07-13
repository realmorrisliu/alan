## ADDED Requirements

### Requirement: Runtime context is Process-shaped
Agent Execution Engine SHALL derive file reachability, cwd, Tool execution,
Agent Definition, Skills, policy, memory handles, and durable evidence
references from the Agent Process namespace and descriptors. It MUST NOT own a
workspace identity, workspace root, or Host `.alan` directory.

#### Scenario: Runtime prepares a turn
- **WHEN** an Agent Process begins a transition
- **THEN** every contextual resource is read from a mounted path or descriptor
- **AND** no Host-directory overlay scan occurs
