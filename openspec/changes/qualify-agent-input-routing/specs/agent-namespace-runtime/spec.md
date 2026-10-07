## ADDED Requirements

### Requirement: Automatic routing requires qualified typed evaluation
Automatic input routing SHALL use the reachable typed evaluation capability and
its bounded failure contract. Until qualification, unprefixed input SHALL retain
the governed Agent baseline. Qualification SHALL preserve complete evidence and
SHALL NOT treat missing cost or failed observations as successful zero-cost work.

#### Scenario: Candidate measurement is prepared
- **WHEN** candidate evaluation is prepared
- **THEN** labeled cases and numeric qualification budgets are frozen before measurement
- **AND** shadow mode causes no classification effects and compares baseline behavior
- **AND** intent accuracy, false execution classifications, latency and cost are recorded
- **AND** activation requires an explicit decision after the gates pass

#### Scenario: Evidence is incomplete
- **WHEN** observations are missing, duplicated, refer to different input or lack valid cost provenance
- **THEN** the report rejects inconsistent observations or records the affected gate as incomplete
- **AND** failures remain in denominators and no missing price is replaced with zero

#### Scenario: Shadow classification chooses a command
- **WHEN** the evaluator selects command intent during qualification
- **THEN** that selection itself starts no command
- **AND** it is recorded for comparison with the labeled intent and baseline

#### Scenario: A discussion is classified as execution
- **WHEN** qualification reveals a known false execution classification
- **THEN** automatic routing is not enabled until the case is resolved and qualification repeated
