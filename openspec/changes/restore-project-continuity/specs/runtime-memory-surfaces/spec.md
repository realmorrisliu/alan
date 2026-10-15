# Spec Delta

## ADDED Requirements

### Requirement: Project continuity summaries are scoped and user-controlled
Alan SHALL offer bounded project summaries of goals, completed/pending work and verification linked to retained evidence. Users SHALL be able to inspect, correct and delete them. Summary deletion SHALL remove the derived material from recall without silently regenerating it from the same old sources. Durable execution evidence SHALL remain distinct and its retention SHALL be explained. Summaries MUST NOT grant authority or automatically propagate between projects.

#### Scenario: User corrects summary
- **WHEN** a user corrects a generated project summary
- **THEN** the new summary retains user/source provenance and older generated material cannot silently overwrite it

#### Scenario: User deletes summary
- **WHEN** the user deletes derived project continuity material
- **THEN** it is excluded from recall while source evidence is retained separately and explained

#### Scenario: Another project is selected
- **WHEN** the user starts work in a different project
- **THEN** the first project’s summary is not automatically inserted or converted into authorization
