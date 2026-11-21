# Spec Delta

## ADDED Requirements

### Requirement: Terminal continuity controls preserve explicit user selection
The terminal SHALL offer authorized history inspection, explicit recovery selection
and fresh start, plus inspect/correct/delete controls for derived project summaries.
It SHALL show source provenance and authoritative pending, unavailable and failure
states. Reading a summary MUST NOT execute work. Deletion controls SHALL distinguish
derived summary recall from retained execution evidence.

#### Scenario: User chooses fresh start
- **WHEN** history exists but the user selects fresh work
- **THEN** no prior pending operation is launched or old authority inferred

#### Scenario: User deletes a summary
- **WHEN** a user chooses the summary deletion control
- **THEN** the UI explains which derived context is removed and that source execution evidence is retained separately

#### Scenario: History cannot be read
- **WHEN** an authorized history reference is unavailable or malformed
- **THEN** the terminal presents that failure without pretending recovery or execution succeeded
