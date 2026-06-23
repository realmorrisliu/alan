## ADDED Requirements

### Requirement: Protocol events are classified into display tiers
The TUI SHALL classify each protocol event into exactly one of three display tiers — permanent transcript content, ephemeral live-region status, or suppressed — and SHALL render each event only according to its tier.

#### Scenario: Conversational substance is permanent
- **WHEN** the daemon emits a user message, assistant text, a completed tool call, a plan snapshot, or a fatal error
- **THEN** the TUI renders it as permanent transcript content eligible for terminal scrollback

#### Scenario: Internal lifecycle events are suppressed from the transcript
- **WHEN** the daemon emits a turn-started, turn-completed, terminal-resize, event-sequence-gap, or session-hydration event
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
- **THEN** the TUI issues an interrupt operation to the daemon

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
- **THEN** a completion popup lists skills sourced from the daemon skills catalog
- **AND** selecting one inserts a skill-reference token into the message, which is submitted as part of a normal turn

#### Scenario: At references a file inline
- **WHEN** the user types `@` anywhere in the composer
- **THEN** a completion popup lists workspace file paths
- **AND** selecting one inserts the path into the message

#### Scenario: Skill catalog unavailable degrades gracefully
- **WHEN** the daemon skills catalog cannot be retrieved
- **THEN** the `$` popup shows no candidates and the user may continue typing freely
- **AND** the TUI does not crash or block input

### Requirement: TUI is keyboard-only and preserves terminal-native selection
The TUI SHALL NOT capture mouse input and SHALL leave text selection and copy to the host terminal.

#### Scenario: Mouse capture is disabled
- **WHEN** the TUI is running
- **THEN** the terminal's native mouse selection and copy behavior is available
- **AND** the TUI does not enable mouse capture
