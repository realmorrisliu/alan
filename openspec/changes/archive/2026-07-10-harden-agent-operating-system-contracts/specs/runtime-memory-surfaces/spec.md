## ADDED Requirements

### Requirement: Substantive Current Goal Selection
Generated memory and handoff surfaces SHALL derive `Current Goal` from substantive user intent, active plan state, or durable task context rather than blindly using the latest user message.

#### Scenario: Latest user message is an approval control
- **WHEN** the latest user message is a runtime confirmation, approval control, or internal control payload
- **THEN** generated memory surfaces keep the prior substantive goal or plan explanation as `Current Goal`

#### Scenario: Latest user message is low-information
- **WHEN** the latest user message is a short ambiguous acknowledgement, single-letter fragment, or otherwise low-information follow-up after a substantive task
- **THEN** generated memory surfaces keep the prior substantive task goal unless the active plan explicitly changes

#### Scenario: Latest user message is a new substantive request
- **WHEN** the latest user message contains a new actionable request or changes the task objective
- **THEN** generated memory surfaces may use that message as `Current Goal` after coherent truncation

### Requirement: Memory Surfaces Preserve Evidence Continuity
Generated memory and handoff surfaces SHALL preserve enough evidence context for another runtime to continue or audit active work.

#### Scenario: Turn used truncated evidence
- **WHEN** a turn relied on tool or child evidence that was previewed or truncated in prompt-facing text
- **THEN** generated memory surfaces include bounded references to relevant rollout, child-run, or evidence artifact identifiers when available
