# os-boundary-security Specification

## Purpose
Defines the OS confinement and explicit mount-grant security boundary for
confined native execution, and establishes mounted project instruction files as
non-authoritative knowledge.

## Requirements

### Requirement: The OS boundary is the security boundary for confined execution
When native execution runs inside an active OS confinement backend, Alan SHALL rely
on that confinement and the explicit mount grants as the security boundary. Alan
SHALL NOT reject or escalate a command solely because its shell text matches a
static pattern, and SHALL NOT invoke Guardian for an effect the active confinement
already contains.

#### Scenario: A contained command runs without review
- **WHEN** a command whose effects stay inside the granted confinement is proposed
- **THEN** it runs without a static text ban and without a Guardian call

#### Scenario: An uncontained effect still escalates
- **WHEN** a proposed effect escapes the active confinement, such as network egress beyond grants, an out-of-grant write, publication or a control-policy change
- **THEN** it follows the existing red-line and review path

#### Scenario: No confinement backend is active
- **WHEN** no confinement backend is active and Host-account execution was not explicitly selected
- **THEN** the conservative pre-sandbox escalation boundary remains in force

### Requirement: Project instruction files are non-authoritative knowledge
An Agent SHALL be able to read `AGENTS.md` and `CLAUDE.md` from an explicitly mounted
project as ordinary knowledge. Their content SHALL NOT widen authority, add grants,
change approval posture or isolation, or alter control policy.

#### Scenario: A mounted project carries an instruction file
- **WHEN** the project contains `AGENTS.md` or `CLAUDE.md` and the Agent reads it
- **THEN** its text informs the work like any other file

#### Scenario: An instruction file demands more authority
- **WHEN** an instruction file asks to disable review or enable full trust
- **THEN** posture, grants and control policy remain unchanged
