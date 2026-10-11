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

## ADDED Requirements

### Requirement: A retained runtime error does not repeat as a live notice
The terminal SHALL retire a matching runtime-owned Error notice when the same
runtime error is retained in transcript history. It MUST preserve unrelated
runtime notices and local notices, including identical local text. Notice
retirement MUST NOT change input completion, Process lifecycle, original error
evidence or the draft/cursor. Hydrating the existing notice/error event pair SHALL
apply the same rule without deleting history or replaying work.

#### Scenario: Delegation fails but the parent completes its direct fallback
- **WHEN** the runtime publishes a matching Error notice and retained error, then completes the parent's task
- **THEN** the error appears once in history without remaining as a duplicate live footer
- **AND** the parent's actual Completed receipt and unchanged draft remain authoritative

#### Scenario: Local or unrelated notices must survive
- **WHEN** a retained runtime error has the same text as a local notice or a different text/severity from a runtime notice
- **THEN** that notice remains visible with its original ownership and severity
- **AND** text similarity alone does not authorize retirement

### Requirement: Delegated skill prelaunch rejection remains visible without changing authority
The runtime SHALL publish delegated-skill prelaunch rejection through existing
AgentFS notice and retained terminal-error surfaces when the invocation is invalid,
unavailable, cannot resolve its target or fails before child launch. Publication
MUST NOT invent an Action or child Process, broaden the child namespace, retry
effects or replace the original Tool result and parent continuation. A failed
presentation write SHALL preserve the recorded rejection and report the
publication failure through existing diagnostics. Successful launches and child
wait/cancel/exit states SHALL retain their existing lifecycle semantics.

#### Scenario: A child cwd is outside its explicit delegated namespace
- **WHEN** child launch rejects the delegated cwd before creating a child Process
- **THEN** the file-backed terminal receives the actual launch rejection independently of the model's commentary
- **AND** the parent retains the failed Tool result and may choose its existing direct Tool path without gaining authority
- **AND** turn settlement and a subsequent input do not erase the retained rejection or imply that delegation succeeded

#### Scenario: Presentation cannot be published
- **WHEN** the prelaunch rejection is recorded but its UI publication fails
- **THEN** parent advancement and the original failed Tool result remain unchanged
- **AND** diagnostics identify the unavailable presentation without creating an Action or retrying child launch

### Requirement: Tool loop guard stops remain visible in file-backed history
The runtime SHALL publish the actual stop reason through the existing AgentFS
notice and terminal-error event surfaces when repeated identical Tool calls or
the configured Tool batch limit stops a turn. The reason SHALL
remain in transcript history after the turn settles and after a subsequent
input clears the current notice. The stop MUST NOT execute the refused Tool,
replay earlier effects, imply successful task completion or advertise unsupported
environment overrides.

#### Scenario: Repeated call is refused before execution
- **WHEN** the repeated-call guard prevents the next identical Tool invocation
- **THEN** the file-backed terminal receives a retained error and current notice explaining the stop
- **AND** no Action or Process is created for the refused invocation

#### Scenario: Batch limit stops further advancement
- **WHEN** the configured Tool batch limit ends the current turn
- **THEN** the completed batch's original results remain and its stop reason is published through the same file-backed error surfaces
- **AND** settling the turn or starting another input does not erase its historical error or replay the completed batch

### Requirement: Explicit project revocation uses authoritative current-Root evidence
The terminal UI SHALL discover the current Root's active project grant for
`/project revoke` when no retained project-picker or recovery receipt exists, using bounded public
Host Mount grant/request files, matching requester identity and observed cwd by
namespace path components. It SHALL select the longest matching prefix and refuse
ambiguous, malformed, unavailable or stale evidence. Discovery MUST NOT create
authority, infer access from cwd, revoke another Root's grants or select arbitrary
grants at `/`. It SHALL reuse the existing confirmed leave-cwd-before-revoke
sequence, retain uncertain authority until acknowledgment and never resume work
implicitly. Read-only discovery SHALL preserve responsive input and exit.

#### Scenario: Externally granted current project is explicitly revoked
- **WHEN** the current Root has selected an SDK-granted project without a local picker receipt and the user invokes `/project revoke` at a settled boundary
- **THEN** matching active grant/request evidence identifies that Root's current project
- **AND** a correlated successful cwd control leaves the project before Host revocation
- **AND** only actual Host acknowledgment confirms revocation; paused work stays paused

#### Scenario: Discovery cannot establish a unique current project
- **WHEN** current cwd has no matching active grant, evidence is malformed or unavailable, or two equally specific grants match
- **THEN** the UI reports no matching authority or an explicit lookup failure without claiming all grants are absent
- **AND** no cwd control or grant mutation is sent

#### Scenario: Context changes during discovery
- **WHEN** Root identity, cwd or the settled input boundary changes before discovery completes
- **THEN** the stale result sends no directory control or revocation
- **AND** a subsequent explicit command must discover authority in its current context
