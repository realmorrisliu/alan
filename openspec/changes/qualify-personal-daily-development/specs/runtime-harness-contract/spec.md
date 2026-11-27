# Spec Delta

## ADDED Requirements

### Requirement: Personal daily-use qualification retains a consecutive real-work sample
Personal daily-use qualification SHALL retain >=50 real tasks over >=14 consecutive days across Alan and a second real repo, freezing intake, budgets, eligibility, and ordinary-task rules upfront and requiring >=90% completion without unplanned rescue. Grants and clarification SHALL be reported separately from rescues; pilots and full-trust runs MUST NOT replace the default sample, and every failed, blocked, abandoned, or budget-exhausted task SHALL remain in the denominator.

#### Scenario: Task requires rescue
- **WHEN** an admitted task needs an operator to repair the runtime or finish the solution
- **THEN** the task remains in the ledger and cannot count as completion without rescue

#### Scenario: Retried task succeeds
- **WHEN** a task succeeds on a later attempt
- **THEN** the first outcome, interventions and all attempts remain independently visible
### Requirement: Personal daily-use reporting includes interruption and reliability gates
Qualification SHALL require median zero permission interruptions after initial authorization for ordinary tasks, report P95 and the full denominator, and fail on any observed unauthorized write, repeated effect or lost work. It SHALL pass all frozen cancellation, revocation and explicit recovery controls. Comparison SHALL identify model/environment mismatches and report unavailable usage/cost without fabrication.

#### Scenario: Repeated approval interrupts ordinary work
- **WHEN** a previously delegated ordinary operation asks again
- **THEN** the interruption is counted and attributed to its actual cause

#### Scenario: Comparison model differs
- **WHEN** Codex cannot use the same effective model and effort
- **THEN** the result is labeled a whole-product comparison rather than a harness-only ranking
