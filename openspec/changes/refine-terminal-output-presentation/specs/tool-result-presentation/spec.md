## ADDED Requirements

### Requirement: Routine Action summaries avoid redundant scaffolding
The TUI SHALL render bounded Action summaries using the Runtime-owned title,
existing presentation primitive and authoritative outcome. It SHALL avoid
repeating the same operation, path or command in title and summary. Missing
presentation metadata SHALL retain a readable bounded fallback without inferred
success. The existing primitive-specific information and retained-detail
requirements SHALL continue to apply.

#### Scenario: A command completes with a structured result
- **WHEN** command title and payload identify the same command
- **THEN** the routine summary presents the command once and keeps its actual exit state visible
- **AND** detail retains distinguishable stdout and stderr without re-execution

#### Scenario: A file result repeats its path
- **WHEN** the title and structured file or diff payload identify the same path
- **THEN** the routine summary shows that path once with the available content or change metadata
- **AND** the original contents or diff remain inspectable

#### Scenario: A dynamic Tool has only fallback metadata
- **WHEN** a Tool has no supported structured presentation
- **THEN** the summary uses the available human title or Tool name and bounded fallback
- **AND** the TUI does not interpret Tool arguments or call a model to invent a summary

### Requirement: Adjacent successful read-only Actions may share presentation
The TUI SHALL support compact groups of adjacent successful Actions only when
Runtime-owned semantics positively identify them as read-only and their order,
turn and authority context are correlated. Missing eligibility SHALL produce a
standalone result. Grouping MUST NOT infer eligibility from titles, result
content, a presentation primitive or an arbitrary shell command. Individual
Action identity, status and retained detail SHALL remain accessible.

#### Scenario: A sequence of eligible reads completes
- **WHEN** adjacent successful read or search Actions have eligible semantics in the same correlated turn and authority context
- **THEN** the renderer can present their uncommitted summaries as one bounded group
- **AND** the user can select and inspect each member's own result

#### Scenario: An operation ends a group
- **WHEN** a message, explicit command, plan change, request, write, failure, rejection, cancellation, unknown operation or authorization change separates eligible Actions
- **THEN** they are not grouped across that boundary
- **AND** significant outcomes and requests remain distinct

#### Scenario: Eligibility is unknown or claims come from content
- **WHEN** an older record lacks classification or Tool text claims that a call is read-only
- **THEN** the result remains standalone unless authoritative Runtime metadata independently establishes eligibility
- **AND** ordinary content never changes authority or grouping eligibility

#### Scenario: Completed group rows are already in native scrollback
- **WHEN** another eligible Action finishes after the earlier group's rows have been committed
- **THEN** the renderer leaves those rows intact and presents subsequent work without rewriting native scrollback
- **AND** a running Action remains visible without waiting for group completion
