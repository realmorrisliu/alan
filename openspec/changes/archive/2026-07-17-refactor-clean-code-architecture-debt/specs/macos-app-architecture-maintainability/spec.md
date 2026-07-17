## ADDED Requirements

### Requirement: Apple architecture debt reaches zero
Alan for macOS SHALL complete the recorded architecture migration with zero
maintainability warnings. Each ownership slice MUST lower the warning ceiling
without introducing a new warning class, duplicate domain implementation, or
shallow pass-through bridge.

#### Scenario: Apple warning slice completes
- **WHEN** a focused refactor resolves a recorded large-file or bridge-seam
  warning
- **THEN** the architecture ledger and executable warning ceiling are lowered
  in the same PR
- **AND** a fresh Alan Dev build and relevant rendered behavior are verified

#### Scenario: Final Apple architecture validation runs
- **WHEN** all 15 recorded warnings have been resolved
- **THEN** architecture maintainability report mode emits zero warnings
- **AND** strict mode passes with the same source tree
- **AND** the non-zero migration ledger is removed
