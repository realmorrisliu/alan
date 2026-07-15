## ADDED Requirements

### Requirement: Repository quality gate ratchets Apple architecture debt
The repository quality gate SHALL run Apple architecture-maintainability
validation for every commit and pull request. The 15 currently recorded
large-file and bridge-seam warnings SHALL remain an explicit ceiling until the
immediate debt burn-down reduces them, and a warning reduction MUST tighten the
recorded count in the same change.

#### Scenario: Apple architecture warning grows
- **WHEN** a change increases the current Apple architecture warning count or
  broadens a recorded warning class
- **THEN** the repository quality gate fails

#### Scenario: Apple architecture warning is removed
- **WHEN** a focused refactor removes or narrows a recorded warning
- **THEN** the Apple architecture ledger and executable ceiling are tightened in
  the same change
- **AND** later reintroduction fails

#### Scenario: Non-Apple CI runner validates ownership
- **WHEN** the repository quality gate runs on a non-macOS CI runner
- **THEN** source-layout, ownership, project-membership, and warning-ledger
  checks still run without requiring an Apple app launch
