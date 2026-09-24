## MODIFIED Requirements

### Requirement: Coding workflow control modes preserve coding-loop causality
alan SHALL define `steer`, `follow_up`, and `next_turn` semantics for active
coding workflows.

Control semantics:

- `steer` re-plans the active coding loop quickly and may skip remaining safe
  steps when needed.
- `follow_up` queues additional coding intent for the next eligible cycle; a paused ordinary queue
  requires explicit continuation and SHALL NOT be consumed automatically.
- `next_turn` queues future coding context without breaking the current turn's
  causality.

#### Scenario: User steers active coding work
- **WHEN** a user submits `steer` while a coding worker is in an active loop
- **THEN** alan treats the input as active-loop steering and may re-plan before
  continuing or stopping safe remaining steps

#### Scenario: User queues future context
- **WHEN** a user submits `next_turn` during a coding workflow
- **THEN** alan preserves it as future context rather than rewriting the current
  turn's causality

#### Scenario: Interrupt pauses queued coding intent
- **WHEN** an interrupt pauses ordinary queued follow-up work
- **THEN** that work does not begin a new coding cycle until explicitly continued
- **AND** its scheduling mode does not override the queue pause
