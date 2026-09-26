## ADDED Requirements

### Requirement: Automatic routing requires qualified typed evaluation
Automatic input routing SHALL use the reachable typed evaluation capability and
its bounded failure contract. Before activation it SHALL run in shadow mode with
no effects caused by classification, compare baseline behavior and record intent
accuracy, false execution classifications, latency and cost. Known cases that
classify discussion as execution SHALL block activation. Numeric qualification
budgets SHALL be set before candidate evaluation and activation SHALL be explicit.
Until qualification, unprefixed input SHALL retain the governed Agent baseline.

#### Scenario: Shadow classification chooses a command
- **WHEN** the evaluator selects command intent during qualification
- **THEN** that selection itself starts no command
- **AND** it is recorded for comparison with the labeled intent and baseline

#### Scenario: A discussion is classified as execution
- **WHEN** qualification reveals a known false execution classification
- **THEN** automatic routing is not enabled until the case is resolved and qualification repeated
