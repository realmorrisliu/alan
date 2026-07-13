## ADDED Requirements

### Requirement: Root Agent is a supervised role
Service Manager SHALL start Root Agent from a Boot Unit with a system-owned
Agent Definition descriptor and SHALL publish the current Process at
`/agent/root`. Host arguments and Host directories MUST NOT change the Root
Agent Definition.

#### Scenario: Root Agent restarts
- **WHEN** its Process exits and restart succeeds
- **THEN** `/agent/root` resolves to the replacement Process
- **AND** the old PID remains terminal rather than being reused as continuity
