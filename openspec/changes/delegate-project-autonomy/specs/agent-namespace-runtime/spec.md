# Spec Delta

## ADDED Requirements

### Requirement: Task budgets bound autonomous continuation
Every autonomous task SHALL have finite configured elapsed-time, Tool-call and model-call budgets, including review, fallback and retry overhead. Exhaustion SHALL pause further admission and preserve recoverable evidence; resume MUST NOT reset consumption without explicit user extension. Unavailable monetary usage SHALL NOT be reported as known cost.

#### Scenario: Budget exhausted
- **WHEN** a task reaches any configured execution budget
- **THEN** further dependent work pauses with a readable reason and retained state

#### Scenario: Task resumes after restart
- **WHEN** a user selects a paused task for recovery
- **THEN** its consumed budget remains accounted and no completed effect is replayed
### Requirement: Pending decisions preserve dependency ordering
A pending permission or clarification SHALL keep its original correlated operation. Other work SHALL proceed only if it is independently authorized and does not consume that operation’s result or rely on its unresolved authority. Unknown dependence SHALL preserve ordering, and repeated observations SHALL NOT create duplicate prompts or effects.

#### Scenario: Independent read exists
- **WHEN** a pending publication decision does not affect an already-admitted read-only review
- **THEN** the runtime may complete the independent review while retaining the publication wait

#### Scenario: Dependent work exists
- **WHEN** a later operation requires the pending operation’s output
- **THEN** it does not execute before that decision and result settle
