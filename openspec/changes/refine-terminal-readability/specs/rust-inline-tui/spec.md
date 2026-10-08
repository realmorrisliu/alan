# Spec Delta

## MODIFIED Requirements

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
