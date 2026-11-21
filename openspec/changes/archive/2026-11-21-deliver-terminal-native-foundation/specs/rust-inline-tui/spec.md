## ADDED Requirements

### Requirement: Assistant output and thinking render incrementally
The terminal UI SHALL render assistant text and thinking as they arrive from the
Agent Process output files and SHALL show elapsed thinking time while thinking is
streaming. The final message SHALL reflect the reconciled tape content.

#### Scenario: A long answer streams
- **WHEN** the model emits text over several seconds
- **THEN** the visible transcript grows before the response completes
- **AND** the final transcript matches the reconciled message

### Requirement: Leaving a modal never stops input
The terminal UI SHALL close only the active modal, chooser, form or project
selection when it is quit or dismissed with Ctrl+Q or Esc. The terminal input
reader SHALL keep running until the application itself exits.

#### Scenario: Ctrl+Q inside the model chooser
- **WHEN** the model chooser is open and the user presses Ctrl+Q
- **THEN** the chooser closes and the composer accepts the next key

### Requirement: Idle Esc and modified characters are inert
Esc at an idle prompt SHALL dismiss completion or do nothing and SHALL NOT send an
interrupt. A character key pressed with Control or Alt that has no binding SHALL
NOT be inserted into the composer.

#### Scenario: Esc at an idle prompt
- **WHEN** no turn, request or modal is active and the user presses Esc
- **THEN** no interrupt is sent

### Requirement: Secret entry does not echo
Interactive secret entry SHALL disable terminal echo while reading from a terminal
and SHALL restore it on every exit path, while piped input remains supported.

#### Scenario: Setting a secret in a terminal
- **WHEN** a user enters a secret at a terminal prompt
- **THEN** the typed characters are not displayed

### Requirement: Terminal behavior is verified on a real PTY
Terminal acceptance SHALL drive the built binary through a Unix pseudo-terminal and
a terminal-state model covering startup frame, resize reflow, wide text, Ctrl+C,
Esc and modal exit. No terminal-host harness SHALL be required.

#### Scenario: Acceptance runs without a terminal host
- **WHEN** the PTY acceptance suite runs in CI
- **THEN** it passes using only the built binary and the terminal-state model
