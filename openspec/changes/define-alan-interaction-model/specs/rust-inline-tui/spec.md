## MODIFIED Requirements

### Requirement: Codex-like terminal interaction baseline
The Rust TUI SHALL be a keyboard-driven inline renderer over mounted AgentFS
files. It SHALL render recent transcript content followed immediately by an
editable `alan >` prompt, preserve committed content in terminal scrollback,
and SHALL NOT reserve a full-screen viewport or pin the prompt to a bottom
composer. Transient completion candidates, multiline input, and structured
input MAY temporarily expand the inline viewport and SHALL disappear when the
interaction ends. Existing typed transcript cells, incremental file updates,
resize reflow, and frame coalescing SHALL remain supported.

#### Scenario: Streaming assistant output renders incrementally
- **WHEN** renderer-visible runtime state streams output
- **THEN** the TUI updates typed transcript cells incrementally
- **AND** it coalesces redraws so high-frequency deltas do not overwhelm the
  terminal
- **AND** the editable prompt follows the latest transcript without a fixed
  bottom panel or a blank screen-sized gap

#### Scenario: Completed content enters terminal scrollback
- **WHEN** visible transcript content exceeds the active inline viewport
- **THEN** committed lines are inserted into terminal scrollback
- **AND** the inline viewport remains focused on the current interaction

#### Scenario: Resize preserves readable state
- **WHEN** the terminal is resized during a turn or while editing input
- **THEN** transcript and prompt reflow without losing input or duplicating or
  dropping rendered content

#### Scenario: Long input keeps its editable tail visible
- **WHEN** multiline input extends beyond the inline composer viewport
- **THEN** the inline paragraph scrolls to keep the edit cursor visible
- **AND** the complete input remains available for editing and submission

### Requirement: Live region shows agent activity and interrupt affordance
While a turn is running, the TUI SHALL show concise activity and interrupt
status adjacent to the inline prompt. This status MAY temporarily expand the
inline viewport but SHALL NOT pin input to a full-height bottom panel or
displace committed output from terminal scrollback. It SHALL disappear when
the turn completes. Ctrl-C or Escape during a turn SHALL request interruption
through the active control plane.

#### Scenario: Activity indicator appears during a running turn
- **WHEN** a turn is in progress
- **THEN** the inline interaction shows the current action and an interrupt
  affordance
- **AND** the activity status disappears when the turn completes

#### Scenario: Interrupt is always available during a turn
- **WHEN** the user presses Esc while a turn is running
- **THEN** the TUI issues an interrupt through the active control plane

#### Scenario: Ephemeral status does not enter scrollback
- **WHEN** a warning or task failure is also recorded as transcript content
- **THEN** its user-facing summary appears once in the transcript
- **AND** transient activity status does not enter terminal scrollback
- **AND** diagnostic details remain available through existing logs

#### Scenario: Streaming text commits at line boundaries
- **WHEN** assistant text streams into the inline viewport
- **THEN** completed lines are committed to terminal scrollback without
  duplicating or dropping content across the prompt boundary

### Requirement: Command and reference completion surface
The TUI SHALL provide keyboard-driven completion for slash commands, skill
references, and file references using the current completion sources.
Selecting a slash command runs the local command and does not send it to the
Agent; selecting a skill or file inserts a reference into Agent-bound input.
Candidate UI SHALL be temporary and adjacent to the active inline prompt.

#### Scenario: Slash opens client commands
- **WHEN** the user types `/` at the start of input
- **THEN** a completion popup lists data-driven client commands
- **AND** selecting one runs a local action that is not sent to the Agent

#### Scenario: Dollar references a skill inline
- **WHEN** the user types `$` anywhere in the composer
- **THEN** a completion popup lists active skills
- **AND** selecting one inserts a skill-reference token into Agent-bound input

#### Scenario: At references a file inline
- **WHEN** the user types `@` anywhere in the composer
- **THEN** a completion popup lists available files
- **AND** selecting one inserts a file-reference token into Agent-bound input

#### Scenario: Skill catalog unavailable degrades gracefully
- **WHEN** the active skill catalog cannot be resolved
- **THEN** the `$` popup shows no candidates and the user may continue typing
- **AND** the TUI does not crash or block input

### Requirement: TUI is keyboard-only and preserves terminal-native selection
The TUI SHALL NOT capture mouse input and SHALL leave text selection and copy to
the host terminal. It SHALL restore terminal modes on normal exit, EOF, and
error, and leave the latest prompt/output in terminal order when it exits.
Quitting or closing the renderer SHALL NOT stop the Agent Process or Host.

#### Scenario: Mouse capture is disabled
- **WHEN** the TUI is running
- **THEN** the terminal's native mouse selection and copy behavior is available
- **AND** the TUI does not enable mouse capture

#### Scenario: Renderer exits while a task is active
- **WHEN** the user quits or the terminal input ends during a running task
- **THEN** only the renderer exits
- **AND** it does not kill the Host, stop the Agent, or automatically replay the
  task on a later attach

#### Scenario: Ctrl-D detaches from an empty prompt
- **WHEN** the user presses Ctrl-D with an empty prompt
- **THEN** the renderer exits and restores terminal modes
- **AND** the attached Agent Process and Host continue running
