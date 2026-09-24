## ADDED Requirements

### Requirement: Interactive Alan uses a shell-like inline transcript
When `alan` runs in an interactive terminal, the renderer SHALL present one
continuous terminal transcript: accepted user input SHALL appear as an `alan >`
line, Agent output SHALL follow it in terminal order, and the next `alan >`
prompt SHALL appear immediately after the output. The prompt SHALL NOT be pinned
to a full-height bottom composer. Committed transcript SHALL remain available
in the host terminal's scrollback.

#### Scenario: A task completes in the inline REPL
- **WHEN** a user submits a task to the attached Root Agent Process
- **THEN** the submitted line is shown with the `alan >` prompt
- **AND** Agent output follows in the same terminal flow
- **AND** the next editable `alan >` prompt follows the output

#### Scenario: Completion candidates are opened
- **WHEN** a user enters a slash command or a `$` skill or `@` file reference
- **THEN** matching candidates appear in a temporary inline list adjacent to
  the current prompt
- **AND** arrow keys, Tab, Enter, and Escape retain their documented selection
  and dismissal behavior
- **AND** closing the list returns to the compact transcript-and-prompt layout

### Requirement: Interactive errors are visible in terminal output
A task, attachment, or input error that prevents or ends an interactive
operation SHALL be shown in the terminal transcript with an actionable
user-facing summary. Diagnostic details MAY remain in logs. One-shot execution
continues to write results only to stdout, diagnostics to stderr, and returns
a nonzero exit code on failure.

#### Scenario: An interactive task fails
- **WHEN** the Agent task or its Connection fails
- **THEN** the user sees the failure in terminal output without consulting logs
- **AND** the error does not claim the task succeeded

#### Scenario: Ctrl-D detaches at an empty prompt
- **WHEN** the user presses Ctrl-D with no pending input
- **THEN** the interactive renderer exits and restores terminal modes
- **AND** any accepted Agent task continues without interruption or replay
