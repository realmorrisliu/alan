# rust-inline-tui Specification

## Purpose
Define Alan's Rust terminal UI as a terminal-first renderer host whose local
contract reads mounted agent files directly.

## Requirements

### Requirement: Legacy TUI entrypoints are removed
alan SHALL remove the TypeScript/Bun/Ink TUI, the `alan-tui` shipped executable,
the `ALAN_TUI_PATH` override, and the public `alan chat` and `alan ask`
commands.

#### Scenario: Legacy commands are unavailable
- **WHEN** a user runs `alan chat` or `alan ask`
- **THEN** alan reports the command as unsupported or unknown
- **AND** it does not delegate to the old TypeScript TUI

#### Scenario: Legacy TUI fallback is unavailable
- **WHEN** `ALAN_TUI_PATH` is set in the environment
- **THEN** bare `alan` ignores it or reports it as unsupported
- **AND** no production code loads a TypeScript TUI bundle from that path

#### Scenario: Release artifacts omit alan-tui
- **WHEN** release artifacts are assembled
- **THEN** they include the `alan` executable for terminal use
- **AND** they do not include, sign, link, or install an `alan-tui` executable

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

### Requirement: Terminal behavior has focused verification
The Rust TUI SHALL include focused automated verification for terminal behavior,
including snapshots or vt100-style tests for transcript rendering, scrollback,
resize, composer editing, streaming deltas, pending yield surfaces, and
noninteractive startup failures.

#### Scenario: Terminal snapshots cover core cells
- **WHEN** typed transcript cell rendering changes
- **THEN** snapshot tests cover assistant text, thinking, tool calls, plans,
  warnings, errors, and pending yields

#### Scenario: Scrollback behavior is tested
- **WHEN** transcript viewport or scrollback insertion behavior changes
- **THEN** terminal behavior tests verify committed history, active viewport
  content, and resize reflow

#### Scenario: Legacy fallback cannot pass tests
- **WHEN** a production fallback path to `clients/tui`, Bun, Ink, or `alan-tui`
  is reintroduced
- **THEN** focused TUI or packaging contract checks fail

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

### Requirement: Thinking is collapsed by default with a toggle
The TUI SHALL render assistant thinking collapsed by default and SHALL provide a keybinding to expand it.

#### Scenario: Thinking collapses when complete
- **WHEN** a thinking stream completes
- **THEN** the TUI shows a single-line summary indicating thinking occurred and its duration
- **AND** the full thinking text is not shown by default

#### Scenario: User expands thinking
- **WHEN** the user activates the thinking-toggle keybinding
- **THEN** the TUI shows the full thinking content
- **AND** activating the keybinding again collapses it

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

#### Scenario: Ctrl-D detaches from an empty prompt with no pending input
- **WHEN** the user presses Ctrl-D with an empty prompt and no pending Agent input
- **THEN** the renderer exits and restores terminal modes
- **AND** the attached Agent Process and Host continue running

#### Scenario: Ctrl-D preserves pending Agent input
- **WHEN** the user presses Ctrl-D while a confirmation or structured-input request is pending
- **THEN** the renderer remains attached
- **AND** the pending request remains available for a response

### Requirement: Bare alan launches the file-backed Rust terminal UI

The `alan` binary SHALL launch its linked Rust terminal UI when invoked without an explicit subcommand. Surviving direct management subcommands SHALL run instead of starting the TUI.

#### Scenario: Bare command enters the TUI

- **WHEN** a user runs `alan` in an interactive terminal
- **THEN** Alan starts the linked Rust terminal UI
- **AND** no separate terminal-UI executable is required on `PATH`

#### Scenario: Direct management command is selected

- **WHEN** a user runs a supported command such as `alan connection list`
- **THEN** Alan executes that command directly
- **AND** it does not start the TUI

### Requirement: Mounted AgentFS files are the complete local TUI contract

The Rust terminal UI SHALL hydrate and update renderer state from mounted `/agent` and `/proc` files, including IO, requests, actions, Agent Machine state, activity, plans, and notices.

#### Scenario: Local renderer starts from a mounted Agent Process

- **WHEN** the TUI receives a mounted namespace and concrete Agent Process path
- **THEN** it reads initial renderer state and tails offset-readable files from that surface
- **AND** user input and control actions are file writes to the mounted Process surfaces

### Requirement: File-backed interaction preserves the terminal baseline

The Rust terminal UI SHALL provide pending input, completion, live activity, collapsed thinking, plan visibility, warnings, and compaction notices from AgentFS snapshots and streams.

#### Scenario: Live state is projected from files

- **WHEN** an Agent Process changes activity, thinking, plan, warning, or compaction state
- **THEN** the TUI updates the appropriate transcript or live region from mounted files
- **AND** display classification does not depend on a client transport event taxonomy

### Requirement: AgentFS yields and recovery states are first-class
The Rust terminal UI SHALL render confirmation requests, structured input, and
recoverable Process errors as focused user-facing states. It SHALL NOT promise
recovery from gaps in retained file streams; stream retention and any future gap
recovery contract belong to the owning stream service.

#### Scenario: Confirmation request is rendered
- **WHEN** AgentFS exposes a pending confirmation request
- **THEN** the TUI presents the action, choices, and default keyboard behavior
- **AND** the answer is written through the request's file control surface

#### Scenario: File stream cannot resume completely
- **WHEN** an offset-readable renderer stream reports that retained data cannot satisfy the last cursor
- **THEN** the TUI surfaces the underlying read failure as an actionable error
- **AND** it does not invent a new offset, reconstruct missing output, or replay accepted input
- **AND** diagnostic details remain available through existing logs

### Requirement: Renderer file updates are classified into display tiers

The TUI SHALL classify each renderer-visible file update as permanent transcript content, ephemeral live-region status, or suppressed lifecycle detail.

#### Scenario: Machine hydration is suppressed

- **WHEN** the renderer hydrates Agent Machine state or observes Process attachment lifecycle metadata
- **THEN** it does not print that lifecycle detail into the transcript
- **AND** it MAY retain the detail in tracing output

#### Scenario: Conversational substance is permanent

- **WHEN** AgentFS surfaces user input, assistant output, a completed Tool result, a plan snapshot, or a fatal error
- **THEN** the TUI renders it as permanent transcript content

### Requirement: Composer history persists across launches

The TUI composer SHALL support standard readline editing and SHALL persist prior submissions in channel-scoped user state for recall across launches.

#### Scenario: A later launch recalls history

- **WHEN** a user submits text, exits the TUI, and launches it again in the same channel
- **THEN** history-previous recalls the earlier submission
- **AND** stable and dev installations do not share the history file implicitly
