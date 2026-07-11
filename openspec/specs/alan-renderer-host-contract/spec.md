# alan-renderer-host-contract Specification

## Purpose
Define what an Alan renderer host is in the Plan 9 model: a client that renders
from Alan OS file surfaces and expresses user input as file writes and `ctl`
commands. This replaces the retired "semantic view snapshot" pull model
(ADR-0024); renderer hosts own presentation, never runtime truth.
## Requirements
### Requirement: Renderer hosts project mounted Alan OS file state

Alan renderer hosts SHALL derive durable presentation state from files under `/proc`, `/agent`, and mounted service trees, and SHALL translate user actions into file or `ctl` writes.

#### Scenario: Renderer host boundary is reviewed

- **WHEN** an Alan renderer host is reviewed
- **THEN** its durable truth source is the mounted Alan OS namespace
- **AND** it owns presentation only, not Process, Agent Machine, or service truth

### Requirement: A mounted namespace is sufficient for local renderer launch

A local renderer host SHALL start from a mounted Alan OS root plus a concrete Agent Process path.

#### Scenario: Renderer opens a root Agent Process

- **WHEN** the renderer receives a namespace root and `/agent/root`
- **THEN** it reads and tails AgentFS output and state files
- **AND** it writes input and Process control through the corresponding files
