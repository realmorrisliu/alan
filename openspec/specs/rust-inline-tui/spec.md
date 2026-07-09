# rust-inline-tui Specification

## Purpose
Define Alan's Rust terminal UI as a terminal-first renderer host whose local
contract reads mounted agent files directly.
## Requirements
### Requirement: Bare alan launches the Rust terminal UI
The `alan` binary SHALL launch the Rust terminal UI when invoked without an
explicit subcommand, and this terminal UI SHALL be linked into the `alan` binary
rather than shipped as a separate executable.

#### Scenario: Bare command enters TUI
- **WHEN** a user runs `alan` in an interactive terminal
- **THEN** alan starts the Rust terminal UI
- **AND** no `alan-tui` executable is required on `PATH`

#### Scenario: Explicit subcommands remain available
- **WHEN** a user runs an explicit supported management subcommand such as
  `alan connection list` or `alan daemon status`
- **THEN** alan runs that subcommand instead of starting the TUI

#### Scenario: Noninteractive terminal is rejected truthfully
- **WHEN** a user runs bare `alan` without an interactive terminal
- **THEN** alan exits with a clear terminal capability error
- **AND** it does not attempt to launch a TypeScript or `alan-tui` fallback

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

### Requirement: Local TUI reads full renderer state from mounted agent files
The Rust terminal UI SHALL treat mounted agent files as the full local terminal
contract, not only for `io/output`, `io/input`, `requests/`, `actions/`, and
`ctl`, but also for renderer-visible runtime state such as activity, thinking,
plan snapshots, and notices.

#### Scenario: Local file-backed mode starts without daemon session APIs
- **WHEN** the user launches local `alan-terminal-ui` against a namespace-native
  runtime
- **THEN** it hydrates and tails the required state directly from mounted agent
  files, including renderer-visible runtime UI state
- **AND** it does not create or attach to a daemon session before rendering

#### Scenario: No daemon-backed TUI mode remains
- **WHEN** the user launches the Rust terminal UI locally after the migration
  cleanup
- **THEN** it uses mounted agent files as its only runtime contract
- **AND** no daemon-backed compatibility or remote TUI mode is available

### Requirement: File-backed local mode preserves local interaction parity
The Rust terminal UI SHALL preserve the current local interaction baseline when
running file-backed: pending input surfaces, command and reference completion,
live activity, collapsed thinking, plan visibility, and renderer-visible
warnings or compaction notices SHALL all work without daemon session event
streams.

#### Scenario: Activity and notices stay in the live region
- **WHEN** a local file-backed turn is active or the runtime emits a recoverable
  warning, compaction notice, or memory-flush notice
- **THEN** the TUI renders that state in the bottom live region from mounted
  agent files
- **AND** it does not require daemon event classification to decide what stays
  ephemeral

#### Scenario: Thinking and plan state render from file surfaces
- **WHEN** the runtime exposes renderer-visible thinking or plan updates through
  agent files
- **THEN** the local file-backed TUI renders collapsed thinking and human-readable
  plan state from those files
- **AND** the user can interact with that state without a daemon-backed session

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

### Requirement: Pending input surfaces are first-class
The Rust TUI SHALL render runtime yields such as confirmation requests,
structured user input, and recoverable interruptions as first-class terminal UI
states rather than raw JSON or debug text.

#### Scenario: Confirmation yield is shown
- **WHEN** the runtime emits a confirmation yield
- **THEN** the TUI presents a focused approval surface with the relevant action,
  choices, and default keyboard behavior
- **AND** the response is submitted as a protocol resume operation

#### Scenario: Structured input yield is shown
- **WHEN** the runtime emits a structured input yield
- **THEN** the TUI presents fields or choices that match the yielded schema
- **AND** it validates the response before submitting it through the active
  control plane

#### Scenario: Recoverable runtime error is shown
- **WHEN** the runtime or renderer host reports a recoverable session, stream,
  or file-surface error
- **THEN** the TUI renders a concise user-facing state with available recovery
  actions
- **AND** raw diagnostic details remain behind an explicit debug surface

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

### Requirement: Renderer updates are classified into display tiers
The TUI SHALL classify each renderer-visible update into exactly one of three
display tiers — permanent transcript content, ephemeral live-region status, or
suppressed — and SHALL render each update only according to its tier.

#### Scenario: Conversational substance is permanent
- **WHEN** the active control plane surfaces a user message, assistant text, a
  completed tool call, a plan snapshot, or a fatal error
- **THEN** the TUI renders it as permanent transcript content eligible for terminal scrollback

#### Scenario: Internal lifecycle events are suppressed from the transcript
- **WHEN** the active control plane surfaces a turn-started, turn-completed,
  terminal-resize, event-sequence-gap, or session-hydration update
- **THEN** the TUI does not render it as transcript content
- **AND** it MAY record the event to the tracing log only

#### Scenario: Internal identifiers never appear on screen
- **WHEN** any event carrying an internal identifier (such as a yield `request_id` or a tool-call id) is rendered
- **THEN** the visible output contains no internal identifier

#### Scenario: Plan snapshots render as a checklist
- **WHEN** a plan snapshot is rendered
- **THEN** items appear as a human-readable checklist reflecting each item's status
- **AND** no Rust debug formatting of the status enum is shown

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

### Requirement: Composer provides readline editing and persisted history
The TUI composer SHALL support standard readline/emacs editing operations and SHALL recall prior submissions from history persisted across sessions.

#### Scenario: Standard editing keys work
- **WHEN** the user uses line-start/line-end, delete-word, delete-to-line-start, and word-wise cursor movement keys
- **THEN** the composer performs the corresponding grapheme-aware edit

#### Scenario: History recalls prior submissions
- **WHEN** the user presses the history-previous key in an empty or partially edited composer
- **THEN** the composer loads the previous submission, and the history-next key moves forward

#### Scenario: History persists across sessions
- **WHEN** the user submits a message in one session and starts a new TUI session
- **THEN** the earlier submission is available via history recall
- **AND** the history is stored under the user's `~/.alan` state directory

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
- **THEN** a completion popup lists workspace file paths
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
