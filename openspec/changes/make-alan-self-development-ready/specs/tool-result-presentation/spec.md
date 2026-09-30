## MODIFIED Requirements

### Requirement: TUI renders each presentation primitive distinctly
The TUI SHALL render each existing presentation primitive appropriately through
the production file-backed path. Routine results SHALL use bounded summaries;
the user SHALL be able to open retained detail and return to the same draft.
Summary bounds SHALL account for rendered physical rows, including long single
lines, rather than only newline counts. Raw evidence SHALL remain distinct from
its user-facing summary.

#### Scenario: Diff renders with change markers
- **WHEN** a `Diff` payload is rendered
- **THEN** its summary shows the affected path and change counts
- **AND** retained detail distinguishes additions and removals with text markers as well as optional color

#### Scenario: Command renders cmdline and exit status
- **WHEN** a `Command` payload is rendered
- **THEN** the command and exit status are visible in its summary
- **AND** available stdout and stderr remain distinguishable in details

#### Scenario: File content renders path and counts
- **WHEN** a `FileContent` payload is rendered
- **THEN** the path and available line count appear in its summary
- **AND** retained content can be inspected without rendering escaped JSON as the default result

#### Scenario: Large output collapses
- **WHEN** a payload exceeds the summary's physical-row budget, including a single long line
- **THEN** it remains bounded with a visible detail action
- **AND** closing details restores the draft and inline transcript position

#### Scenario: Details were not retained
- **WHEN** the result is truncated or its evidence is no longer available
- **THEN** the UI labels the missing portion or unavailable detail truthfully
- **AND** expanding does not fabricate content or re-execute the Tool

## ADDED Requirements

### Requirement: Tool failures explain an available next action
Tool failure summaries SHALL identify the failed operation, an understandable
cause and an available next action when known. Internal diagnostics SHALL remain
available in details without becoming the entire default user-facing message.

#### Scenario: Project execution lacks authorization
- **WHEN** a project Tool cannot execute because no suitable project authority is present
- **THEN** Alan explains that project access is needed and points to the authorization flow
- **AND** it does not describe the Tool or operation as successful

#### Scenario: A failure has no known remedy
- **WHEN** a Tool fails without a reliable user action that resolves it
- **THEN** Alan reports the failure and offers available diagnostic detail
- **AND** it does not invent an authorization action, retry outcome or fix
