# agent-root-layout-contract Specification

## Purpose
Defines descriptor-local Agent Definition layout ownership. Persona, Skills,
policy, and model selection remain inside the explicitly supplied file tree,
with no implicit Host-directory overlay chain.
## Requirements
### Requirement: Agent Definition layout is file-tree local
Alan SHALL interpret persona, Skills, policy, and model selection relative to
the explicitly supplied Agent Definition tree. Production code MUST NOT derive
global, workspace, default, or named Host-directory root chains.

#### Scenario: Definition descriptor is opened
- **WHEN** an Agent Process receives an Agent Definition descriptor
- **THEN** its assets resolve within that tree
- **AND** no other definition tree is overlaid implicitly

### Requirement: Root Agent is a supervised role
Service Manager SHALL start Root Agent from a Boot Unit with a system-owned
Agent Definition descriptor and SHALL publish the current Process at
`/agent/root`. Host arguments and Host directories MUST NOT change the Root
Agent Definition.

#### Scenario: Root Agent restarts
- **WHEN** its Process exits and restart succeeds
- **THEN** `/agent/root` resolves to the replacement Process
- **AND** the old PID remains terminal rather than being reused as continuity
