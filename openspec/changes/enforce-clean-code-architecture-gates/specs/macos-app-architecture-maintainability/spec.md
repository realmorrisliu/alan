## ADDED Requirements

### Requirement: Repository quality gate ratchets Apple architecture debt
The repository quality gate SHALL run Apple architecture-maintainability
validation for every commit and pull request. The 15 currently recorded
large-file and bridge-seam warnings SHALL have stable entries in a structured
ledger. The live report MUST exactly match that ledger, and the ledger MUST be
compared with the pre-change Git reference so it cannot be raised in the same
change as the source. A warning reduction MUST tighten the ledger and recorded
count in the same change.

#### Scenario: Apple architecture warning grows
- **WHEN** a change increases the current Apple architecture warning count or
  broadens a recorded warning class
- **THEN** the repository quality gate fails

#### Scenario: Apple source and warning ledger grow together
- **WHEN** an existing large Swift file and its recorded line ceiling are both
  increased in one change
- **THEN** comparison with the pre-change ledger reports the debt growth
- **AND** the repository quality gate fails

#### Scenario: Apple architecture warning is removed
- **WHEN** a focused refactor removes or narrows a recorded warning
- **THEN** the Apple architecture ledger and executable ceiling are tightened in
  the same change
- **AND** later reintroduction fails

#### Scenario: Non-Apple CI runner validates ownership
- **WHEN** the repository quality gate runs on a non-macOS CI runner
- **THEN** source-layout, ownership, project-membership, and warning-ledger
  checks still run without requiring an Apple app launch
