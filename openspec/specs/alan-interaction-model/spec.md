# alan-interaction-model Specification

## Purpose
Define Alan's interactive terminal contract for an inline Agent REPL, including
transcript order, temporary completion UI, actionable errors, and safe detach.

## Requirements

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
The interactive renderer SHALL show an actionable terminal summary whenever a
task, attachment, or input error prevents or ends an interactive operation.
Diagnostic details MAY remain in logs. For one-shot execution, results SHALL be
written only to stdout, diagnostics SHALL be written to stderr, and failure
SHALL return a nonzero exit code.

#### Scenario: An interactive task fails
- **WHEN** the Agent task or its Connection fails
- **THEN** the user sees the failure in terminal output without consulting logs
- **AND** the error does not claim the task succeeded

#### Scenario: Ctrl-D detaches at an empty prompt
- **WHEN** the user presses Ctrl-D with no pending input
- **THEN** the interactive renderer exits and restores terminal modes
- **AND** any accepted Agent task continues without interruption or replay
