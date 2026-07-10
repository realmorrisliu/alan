## ADDED Requirements

### Requirement: Substantive Current Goal Selection
Generated fallback memory and handoff surfaces SHALL derive `Current Goal` from
substantive user intent, active plan state, or durable task context rather than
blindly using the latest user message. The preference order SHALL be mechanical
(no additional model request): active plan state, then the latest substantive
user request, then the latest message only when nothing better exists.

#### Scenario: Latest input is a request-response control payload
- **WHEN** the latest user input arrived as a `requests/<id>/response` write
  (approval, selection, credential, or structured input)
- **THEN** generated memory surfaces exclude it from goal derivation
  categorically and keep the prior substantive goal or plan-derived goal

#### Scenario: Latest user message is low-information
- **WHEN** the latest user chat message is a bare acknowledgement-class fragment
  (such as a single letter or "ok") following a substantive task
- **THEN** generated memory surfaces keep the prior substantive task goal, and
  the fragment remains untouched in conversation history

#### Scenario: Latest user message is a new substantive request
- **WHEN** the latest user message contains a new actionable request or changes
  the task objective
- **THEN** generated memory surfaces use that message as `Current Goal` after
  coherent truncation, outranking stale plan state

#### Scenario: No substantive goal exists
- **WHEN** a session contains no plan state and no substantive user request
- **THEN** the fallback surface uses the latest message verbatim rather than
  emitting an empty goal
