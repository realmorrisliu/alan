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
editable contextual two-line prompt with a `: ` or `! ` route marker, preserve committed content in terminal scrollback,
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
references and file references using the current authorized sources. Candidate
UI SHALL be temporary and appear below the editable input, after all wrapped
composer lines. With unchanged terminal geometry, transcript and input wrapping,
changes to candidate count or candidate wrapping SHALL NOT move the input line
or its cursor anchor. Candidate rows SHALL remain distinct from composer-height
limits, with usable bounded disclosure near the terminal edge and no permanent
reserved panel. Ordinary input spacing MAY retain one blank row below the
composer from the initial Ready frame; that stable row SHALL remain independent
of candidate count and SHALL be included in the retained-history budget. When
space is bounded, the selected candidate SHALL remain observable below the
composer and keyboard selection SHALL remain usable. This spacing SHALL NOT
create a fixed-position panel or reserve the entire candidate window. Enter on a
selected slash command SHALL run it once; Tab SHALL insert the candidate.
Selecting a Skill or file SHALL insert a reference without submitting the task.
The production bare CLI SHALL supply these sources, not only test entrypoints.
Skill candidates SHALL use the current Process PromptAssemblyCache's enabled,
available, explicitly mentionable canonical IDs, including explicit-only Skills.
A safe typed observation SHALL carry Process identity, version and known state,
without Skill content, Host paths or registry diagnostics. It SHALL initialize
before Ready and refresh with actual cache ensure during prompt build/resolve;
this does not require idle scanning or dynamic Host capability refresh. Missing,
failed, invalid or oversized observations SHALL clear stale candidates rather
than use an old cache. The whole serialized document SHALL use the existing
AgentFS 1 MiB document budget, checked before publishing known state; overflow
SHALL publish a small unknown observation, never silently truncate IDs. No new
candidate-count or per-ID length limit SHALL replace canonical Skill authority.
The consumer SHALL match the actual Root Process identity and accept publication
versions only within that owner. A newer valid observation SHALL refresh an open
Skill menu. Skill observation failure SHALL invalidate Skill candidates without
changing the draft, cursor, input intent or unrelated completion menus; existing
Root replacement cleanup remains applicable.

#### Scenario: Slash opens client commands
- **WHEN** the user types `/` at the start of input and presses Enter on a selected command
- **THEN** the local command runs once without an additional Enter
- **AND** it is not sent to the Agent as a task

#### Scenario: Candidate filtering preserves the input anchor
- **WHEN** slash, Skill or file candidates change as the user edits input without changing its wrapping or the terminal geometry
- **THEN** candidates render below the input and its cursor anchor remains stable
- **AND** candidate rows do not consume the composer height limit or hide the active input
- **AND** the temporary menu remains usable near the terminal edge without a permanent blank panel

#### Scenario: Completion opens near the terminal bottom
- **WHEN** the normal input area is near the terminal bottom in a pane with room for context, input and one candidate row
- **THEN** the initial Ready frame already budgets that single row of ordinary spacing
- **AND** showing or filtering candidates does not move the input or permanently drain transcript history
- **AND** the selected candidate can be read below the composer and changed with the existing selection keys

#### Scenario: Details opens after completion
- **WHEN** the user opens retained Action details from a near-bottom input area
- **THEN** the existing details view receives its full-terminal viewport rather than a candidate-only height clamp
- **AND** closing details restores the draft and normal completion/input geometry

#### Scenario: Dollar references a skill inline
- **WHEN** the user types `$` in the composer
- **THEN** candidates come only from installed or explicitly supplied Skill descriptors
- **AND** selecting one inserts its reference without submitting the task

#### Scenario: At references a file inline
- **WHEN** the user types `@` in the composer with an authorized project selected
- **THEN** candidates come only from the currently accessible project files
- **AND** selecting one inserts its reference without submitting the task

#### Scenario: Skill catalog unavailable degrades gracefully
- **WHEN** the active Skill catalog cannot be resolved
- **THEN** no candidates are invented and the user may continue typing
- **AND** the TUI does not crash or block input

#### Scenario: Explicit-only Skills remain mentionable
- **WHEN** the current Process resolves an enabled, available Skill with implicit invocation disabled
- **THEN** its canonical ID remains available for explicit dollar completion
- **AND** unreferenced, disabled or unavailable Skills do not become candidates

#### Scenario: Process Skill observation fails or changes owner
- **WHEN** the actual cache ensure operation fails, its observation is missing or invalid, or the current Root Process changes
- **THEN** stale candidates and an already open candidate menu are invalidated
- **AND** stale Tab selection cannot insert a candidate from the previous observation
- **AND** a valid cache with no eligible IDs is known-empty rather than unavailable

#### Scenario: Skill display respects immutable Process references
- **WHEN** Package content is upgraded or uninstalled while an older Process retains its leased explicit references
- **THEN** its candidates reflect that Process capability view rather than the global Package catalog
- **AND** a fresh Process resolves only its own explicit references
- **AND** unavailable or oversized display observation does not revoke Runtime Skill authority

#### Scenario: Permission changes invalidate candidates
- **WHEN** a project grant is revoked
- **THEN** its file candidates cease to be exposed
- **AND** selecting an old reference cannot restore the revoked permission

### Requirement: TUI is keyboard-only and preserves terminal-native selection
The TUI SHALL NOT capture mouse input and SHALL leave text selection and copy to
the host terminal. It SHALL restore terminal modes on normal exit, EOF, and
error, and leave the latest prompt/output in terminal order when it exits.
The TUI SHALL NOT directly issue Process or Host shutdown controls. When bare
`alan` owns the mounted instance, its CLI SHALL shut that instance down after
the renderer exits. A terminal-host view detach while retaining the Alan process
is distinct from actual application exit.

#### Scenario: Mouse capture is disabled
- **WHEN** the TUI is running
- **THEN** the terminal's native mouse selection and copy behavior is available
- **AND** the TUI does not enable mouse capture

#### Scenario: The foreground application exits while a task is active
- **WHEN** the user quits Alan or terminal input ends during a running task
- **THEN** the renderer restores terminal modes and returns to its owning CLI
- **AND** the CLI shuts down its owned instance and active work
- **AND** completed effects remain completed and uncertain outcomes are not
  replayed automatically

#### Scenario: Ctrl-D exits from an empty prompt
- **WHEN** the user presses Ctrl-D with an empty prompt and no pending Agent input
- **THEN** the renderer exits and restores terminal modes
- **AND** the owning foreground application shuts down its Agent and services

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

### Requirement: Agent input uses a contextual two-line prompt
In its ordinary single-line idle state, the inline prompt SHALL have one Agent
context/status line immediately above the editable input line. It SHALL show
the selected project/cwd, effective model or explicit unknown value, and current
Agent state. It SHALL follow transcript flow rather than occupy a fixed screen
position. Multiline drafts and temporary controls SHALL expand only as needed.

#### Scenario: Idle Agent accepts input
- **WHEN** the Agent is ready for a new single-line input
- **THEN** the first prompt line shows actual context and ready state
- **AND** the second line accepts input with an explicit Agent or command route marker

#### Scenario: Cursor follows the inline draft after earlier output
- **WHEN** the inline viewport starts below prior terminal output or moves after scrollback insertion
- **THEN** the terminal cursor identifies the actual insertion position in the rendered draft using terminal coordinates
- **AND** wrapped lines, Chinese text and emoji retain correct cursor placement without assuming a zero-origin viewport

#### Scenario: Attention is required
- **WHEN** work is queued, paused, failed or waiting for approval
- **THEN** the status line distinguishes that state from working and ready
- **AND** the applicable action is visible without consulting logs
- **AND** elapsed activity and queue indicators do not masquerade as completion

#### Scenario: Settled queue pause stays calm
- **WHEN** activity is paused with no waiting submission, no pending user request, and no active work
- **THEN** the context line retains truthful paused queue state without repeating the same paused label
- **AND** the live footer does not show a working spinner glyph, interrupt invitation or elapsed-work clock
- **AND** the renderer does not change Machine pause state or automatically continue queued work

#### Scenario: Actual waiting and model controls retain truthful cues
- **WHEN** a real pending confirmation or structured input requires a response
- **THEN** the live area keeps a static waiting or form cue rather than a working clock
- **WHEN** model selection is pending or its outcome is uncertain
- **THEN** the final status styling corresponds to that state instead of inheriting the ready-state cue

#### Scenario: Status adapts to a narrow viewport
- **WHEN** the prompt cannot fit all metadata on its status line
- **THEN** it abbreviates project context and moves secondary metadata to details before hiding the effective model or active state
- **AND** critical authorization controls remain readable, wrapping when necessary

#### Scenario: Narrow prompt keeps meaningful model and state
- **WHEN** project, model, reasoning and queue details exceed the line budget
- **THEN** project and secondary controls yield before the model identity and current state
- **AND** empty model fields and orphan separators are not rendered
- **AND** detailed model and queue observations remain available through their existing commands

#### Scenario: Narrow action and queue priorities stay truthful
- **WHEN** current approval or project selection competes with secondary queue metadata
- **THEN** the action and model identity take priority over those queue details
- **AND** otherwise compact status preserves execution activity separately from admitted or pending work
- **AND** admission alone is not displayed as working

#### Scenario: A confirmed next model differs from active work
- **WHEN** the compact header is idle without a pending Confirmation or StructuredInput response
- **THEN** it prefers the confirmed next-input model
- **AND** running, paused work or a current response retains active-model priority
- **AND** full status can still distinguish both bindings

### Requirement: Semantic transcript hierarchy survives terminal constraints
The TUI SHALL distinguish user input, assistant prose, code, Tool summaries,
failures and transient status through consistent spacing and semantic styles.
It SHALL preserve code indentation and readable continuation alignment, keep
committed content in host scrollback and retain terminal-native selection.
Critical meaning SHALL remain available without color.

#### Scenario: Assistant returns structured text
- **WHEN** an answer includes headings, emphasis, lists and fenced code
- **THEN** these forms have readable hierarchy without displaying formatting markers as ordinary prose
- **AND** code indentation and literal code content remain copyable

#### Scenario: A narrow terminal shows active work
- **WHEN** the viewport shrinks while work or authorization is pending
- **THEN** route, active state and available action remain readable
- **AND** secondary metadata yields space before essential controls
- **AND** the draft and transcript order are preserved

#### Scenario: Idle draft is cleared
- **WHEN** Ctrl+C is pressed with an editable draft and no active operation or pending request
- **THEN** the draft clears without issuing a misleading runtime cancellation notice

#### Scenario: Fenced answer streams through scrollback
- **WHEN** a fenced code block with an optional language label arrives incrementally
- **THEN** its visible boundary and language label distinguish it from prose without raw fence markers
- **AND** literal code indentation and diff addition/removal meaning are preserved
- **AND** partial drains, resize and later reconciliation do not repeat the label or committed code

#### Scenario: Answer separation survives partial drains
- **WHEN** nonempty assistant content follows prior transcript content
- **THEN** a single leading separation row distinguishes the answer
- **AND** the row is committed at most once across streaming, resize and reconciliation

### Requirement: Terminal receipts retire their own pending outcome hints
The terminal renderer SHALL settle an exact locally tracked submission from its correlated terminal completion receipt, regardless of whether the final queue snapshot arrives before or after that receipt. It SHALL refresh only its own stale pending or unknown hint and SHALL preserve unrelated notices and unsubmitted draft content. Empty queue state alone SHALL NOT imply completion.

#### Scenario: Final queue state precedes completion receipt
- **WHEN** a tracked submission leaves the active queue and its correlated completed, failed or cancelled receipt then arrives
- **THEN** its stale unconfirmed outcome hint is retired immediately without requiring another queue event
- **AND** its real terminal result and unrelated UI content remain intact
