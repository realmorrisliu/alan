## MODIFIED Requirements

### Requirement: Agent input uses a contextual two-line prompt
In its ordinary single-line idle state, the inline prompt SHALL have one Agent
context/status line immediately above the editable input line. It SHALL show
the selected project/cwd, effective model or explicit unknown value, and current
Agent state. It SHALL follow transcript flow rather than occupy a fixed screen
position. Multiline drafts and temporary controls SHALL expand only as needed. Without a local project-picker receipt, an observed successful ordinary directory action SHALL provide the namespace cwd for context; the renderer MUST NOT infer a project label or grant access from that cwd. A Root Agent Process replacement SHALL discard the previous Root's observed cwd.

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


#### Scenario: External project authority and ordinary directory selection
- **WHEN** an existing public Host control grants a directory and the Root completes an ordinary directory action without a local project-picker receipt
- **THEN** the context line displays the observed namespace cwd subject to existing width priorities
- **AND** no project label or read-only access claim is invented from the path
- **AND** running, failed or rejected directory actions do not replace the last observed cwd

#### Scenario: Root replacement clears previous directory context
- **WHEN** the attached Root Agent Process changes
- **THEN** the previous Root's observed cwd is discarded before rendering the new Root
- **AND** retained project authority and pending-control fencing continue to use their existing owners
