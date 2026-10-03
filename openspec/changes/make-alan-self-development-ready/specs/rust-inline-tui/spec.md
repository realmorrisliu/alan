## MODIFIED Requirements

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

## ADDED Requirements

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

### Requirement: Terminal receipts retire their own pending outcome hints
The terminal renderer SHALL settle an exact locally tracked submission from its correlated terminal completion receipt, regardless of whether the final queue snapshot arrives before or after that receipt. It SHALL refresh only its own stale pending or unknown hint and SHALL preserve unrelated notices and unsubmitted draft content. Empty queue state alone SHALL NOT imply completion.

#### Scenario: Final queue state precedes completion receipt
- **WHEN** a tracked submission leaves the active queue and its correlated completed, failed or cancelled receipt then arrives
- **THEN** its stale unconfirmed outcome hint is retired immediately without requiring another queue event
- **AND** its real terminal result and unrelated UI content remain intact
