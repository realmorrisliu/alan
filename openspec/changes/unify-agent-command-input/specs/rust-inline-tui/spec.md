## MODIFIED Requirements

### Requirement: TUI is keyboard-only and preserves terminal-native selection
The TUI SHALL NOT capture mouse input and SHALL leave text selection and copy to
the host terminal. It SHALL restore terminal modes on normal exit, EOF, and
error, and leave the latest prompt/output in terminal order when it exits.
The renderer SHALL restore its terminal and release file attachments on exit.
The owning foreground application SHALL then shut down its instance. Detaching a
Herdr view while retaining the native process is not an application exit.

#### Scenario: Mouse capture is disabled
- **WHEN** the TUI is running
- **THEN** the terminal's native mouse selection and copy behavior is available
- **AND** the TUI does not enable mouse capture

#### Scenario: The foreground application exits while a task is active
- **WHEN** the user quits Alan or the terminal input ends during a running task
- **THEN** the renderer restores terminal modes and the application shuts down owned work
- **AND** completed effects are retained and an unknown outcome does not trigger automatic replay

#### Scenario: Ctrl-D exits from an empty prompt
- **WHEN** the user presses Ctrl-D with an empty prompt and no pending Agent input
- **THEN** the renderer exits and restores terminal modes
- **AND** the foreground application shuts down its owned Agent and services

#### Scenario: Ctrl-D preserves pending Agent input
- **WHEN** the user presses Ctrl-D while a confirmation or structured-input request is pending
- **THEN** the renderer remains attached
- **AND** the pending request remains available for a response
