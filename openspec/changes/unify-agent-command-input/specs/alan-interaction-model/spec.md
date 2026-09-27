## MODIFIED Requirements

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
- **WHEN** the user presses Ctrl-D with an empty composer and no pending Agent input
- **THEN** the renderer restores terminal modes and the application shuts down its owned instance
- **AND** pending work is not left executing in a detached Host or replayed automatically
- **AND** completed effects remain completed and uncertain outcomes are reported truthfully
