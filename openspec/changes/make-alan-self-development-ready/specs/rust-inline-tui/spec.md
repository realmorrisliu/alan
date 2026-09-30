## MODIFIED Requirements

### Requirement: Command and reference completion surface
The TUI SHALL provide keyboard-driven completion for slash commands, skill
references and file references using the current authorized sources. Candidate
UI SHALL be temporary and adjacent to the active inline prompt. Enter on a
selected slash command SHALL run it once; Tab SHALL insert the candidate.
Selecting a Skill or file SHALL insert a reference without submitting the task.
The production bare CLI SHALL supply these sources, not only test entrypoints.

#### Scenario: Slash opens client commands
- **WHEN** the user types `/` at the start of input and presses Enter on a selected command
- **THEN** the local command runs once without an additional Enter
- **AND** it is not sent to the Agent as a task

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

#### Scenario: Attention is required
- **WHEN** work is queued, paused, failed or waiting for approval
- **THEN** the status line distinguishes that state from working and ready
- **AND** the applicable action is visible without consulting logs
- **AND** elapsed activity and queue indicators do not masquerade as completion

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
