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
The first Rust TUI SHALL provide a Codex-like terminal interaction baseline:
explicit terminal mode ownership, ratatui-style frame rendering, a bottom
composer, inline viewport rendering, terminal scrollback transcript insertion,
typed transcript cells, resize reflow, and frame coalescing.

#### Scenario: Streaming assistant output renders incrementally
- **WHEN** renderer-visible runtime state streams thinking, text, tool, plan,
  warning, or error updates
- **THEN** the TUI updates typed transcript cells without rebuilding the entire
  transcript as plain strings
- **AND** it coalesces redraws so high-frequency deltas do not overwhelm the
  terminal

#### Scenario: Completed content enters terminal scrollback
- **WHEN** visible transcript content is committed beyond the active viewport
- **THEN** the TUI inserts committed lines into terminal scrollback
- **AND** the active inline viewport remains focused on current interaction

#### Scenario: Resize preserves readable state
- **WHEN** the terminal is resized during a turn or while editing input
- **THEN** transcript cells, the active viewport, and the bottom composer reflow
  without corrupting input or losing streamed content

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
The TUI SHALL maintain a persistent bottom live region, redrawn independently of committed scrollback, that surfaces in-progress activity and the interrupt affordance while a turn is running.

#### Scenario: Activity indicator appears during a running turn
- **WHEN** a turn is in progress
- **THEN** the live region shows an animated activity line with the current action, elapsed time, and an `esc to interrupt` affordance
- **AND** the activity line disappears when the turn completes without leaving a transcript cell

#### Scenario: Interrupt is always available during a turn
- **WHEN** the user presses Esc while a turn is running
- **THEN** the TUI issues an interrupt through the active control plane

#### Scenario: Ephemeral status does not enter scrollback
- **WHEN** a running tool, recoverable warning, compaction notice, or memory-flush notice is surfaced
- **THEN** it is shown in the live region only and is not committed to terminal scrollback

#### Scenario: Streaming text commits at line boundaries
- **WHEN** assistant text streams into the live region
- **THEN** completed lines are committed to terminal scrollback without duplicating or dropping content across the live-region boundary

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
The TUI SHALL provide a completion popup driven by trigger characters that distinguishes client commands from agent-bound references.

#### Scenario: Slash opens client commands
- **WHEN** the user types `/` at the start of the composer
- **THEN** a completion popup lists data-driven client commands (such as compact, rollback, clear, help, quit, toggle-thinking)
- **AND** selecting one runs a local action that is not sent to the agent

#### Scenario: Dollar references a skill inline
- **WHEN** the user types `$` anywhere in the composer
- **THEN** a completion popup lists skills sourced from the active skill catalog
- **AND** selecting one inserts a skill-reference token into the message, which is submitted as part of a normal turn

#### Scenario: At references a file inline
- **WHEN** the user types `@` anywhere in the composer
- **THEN** a completion popup lists file paths visible in the Process namespace
- **AND** selecting one inserts the path into the message

#### Scenario: Skill catalog unavailable degrades gracefully
- **WHEN** the active skill catalog cannot be resolved
- **THEN** the `$` popup shows no candidates and the user may continue typing freely
- **AND** the TUI does not crash or block input

### Requirement: TUI is keyboard-only and preserves terminal-native selection
The TUI SHALL NOT capture mouse input and SHALL leave text selection and copy to the host terminal.

#### Scenario: Mouse capture is disabled
- **WHEN** the TUI is running
- **THEN** the terminal's native mouse selection and copy behavior is available
- **AND** the TUI does not enable mouse capture

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

The Rust terminal UI SHALL render confirmation requests, structured input, recoverable Process errors, and recoverable file-surface gaps as focused user-facing states.

#### Scenario: Confirmation request is rendered

- **WHEN** AgentFS exposes a pending confirmation request
- **THEN** the TUI presents the action, choices, and default keyboard behavior
- **AND** the answer is written through the request's file control surface

#### Scenario: File stream cannot resume completely

- **WHEN** an offset-readable renderer stream reports that retained data cannot satisfy the last cursor
- **THEN** the TUI shows a concise recoverable state and available recovery actions
- **AND** diagnostic details remain behind an explicit debug surface

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
