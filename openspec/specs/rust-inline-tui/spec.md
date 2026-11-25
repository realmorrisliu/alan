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
The renderer SHALL restore its terminal and release file attachments on exit.
The owning foreground application SHALL then shut down its instance. Detaching a
terminal-host view while retaining the native process is not an application exit.

#### Scenario: Mouse capture is disabled
- **WHEN** the TUI is running
- **THEN** the terminal's native mouse selection and copy behavior is available
- **AND** the TUI does not enable mouse capture

#### Scenario: The foreground application exits while a task is active
- **WHEN** the user quits Alan or the terminal input ends during a running task
- **THEN** the renderer restores terminal modes and the application shuts down owned work
- **AND** completed effects are retained and an unknown outcome does not trigger automatic replay

#### Scenario: Ctrl-D exits from an empty prompt
- **WHEN** the user presses Ctrl-D with an empty prompt and no pending Agent input
- **THEN** the renderer exits and restores terminal modes
- **AND** the foreground application shuts down its owned Agent and services

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

The TUI SHALL classify each renderer-visible file update as permanent transcript
content, ephemeral live-region status, or suppressed lifecycle detail. A plan
change SHALL retain a compact permanent record and access to its corresponding
full snapshot rather than repeatedly printing the complete snapshot inline.

#### Scenario: Machine hydration is suppressed

- **WHEN** the renderer hydrates Agent Machine state or observes Process attachment lifecycle metadata
- **THEN** it does not print that lifecycle detail into the transcript
- **AND** it MAY retain the detail in tracing output

#### Scenario: Conversational substance is permanent

- **WHEN** AgentFS surfaces user input, assistant output, a completed Tool result, a plan snapshot, or a fatal error
- **THEN** the TUI renders it as permanent transcript content
- **AND** a plan snapshot is represented by a compact change record with available progress and current-step information
- **AND** the corresponding full plan remains inspectable subject to explicit retention limits

#### Scenario: A historical plan is inspected after a newer update
- **WHEN** the user opens the detail of an earlier plan record
- **THEN** Alan shows that record's full snapshot or its explicit unavailability
- **AND** it does not substitute the newest plan or infer missing steps
- **AND** closing detail restores the same draft and transcript position

#### Scenario: A plan observation repeats without a change
- **WHEN** an already represented plan snapshot is observed again
- **THEN** the renderer does not append a duplicate plan change record
- **AND** distinct later snapshots remain separately represented

### Requirement: Composer history persists across launches

The TUI composer SHALL support standard readline editing and SHALL persist prior submissions in the product System Store shell-ui subtree for recall across launches. Explicitly isolated store bindings SHALL retain independent history; the retired installation channel SHALL NOT select a history file.

#### Scenario: A later launch recalls history

- **WHEN** a user submits text, exits the TUI, and launches it again with the same product store binding
- **THEN** history-previous recalls the earlier submission
- **AND** a launch bound to a different isolated product store does not recall those submissions

### Requirement: Agent input uses a contextual two-line prompt
In its ordinary single-line idle state, the inline prompt SHALL have one Agent
context/status line immediately above the editable input line. It SHALL show
the selected project/cwd and effective model or explicit unknown value. Ordinary idle state and zero queue counts SHALL remain implicit; actionable state SHALL appear in a separate conditional line. It SHALL follow transcript flow rather than occupy a fixed screen
position. Multiline drafts and temporary controls SHALL expand only as needed. Without a local project-picker receipt, an observed successful ordinary directory action SHALL provide the namespace cwd for context; the renderer MUST NOT infer a project label or grant access from that cwd. A Root Agent Process replacement SHALL discard the previous Root's observed cwd.

#### Scenario: Idle Agent accepts input
- **WHEN** the Agent is ready for a new single-line input
- **THEN** the first prompt line shows actual project context and model/effort without ready, active or next qualifiers
- **AND** the second line accepts input with an explicit Agent or command route marker

#### Scenario: Cursor follows the inline draft after earlier output
- **WHEN** the inline viewport starts below prior terminal output or moves after scrollback insertion
- **THEN** the terminal cursor identifies the actual insertion position in the rendered draft using terminal coordinates
- **AND** wrapped lines, Chinese text and emoji retain correct cursor placement without assuming a zero-origin viewport

#### Scenario: Attention is required
- **WHEN** work is queued, paused, failed or waiting for approval
- **THEN** a conditional status line distinguishes that state from working and ordinary idle input
- **AND** the applicable action is visible without consulting logs
- **AND** elapsed activity and queue indicators do not masquerade as completion

#### Scenario: Settled queue pause stays calm
- **WHEN** activity is paused with no waiting submission, no pending user request, and no active work
- **THEN** the conditional status line retains truthful paused queue state without repeating the same paused label
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

#### Scenario: User requests complete context
- **WHEN** the user invokes /status
- **THEN** actual observed cwd, retained project access, model bindings and queue observation remain inspectable
- **AND** unknown state is explicit and does not invent authority or successful completion

#### Scenario: Quiet input follows completed output
- **WHEN** output settles and no request, failure or queue attention is pending
- **THEN** one blank row and a subtle terminal-width rule separate output from the project/model line and editable input
- **AND** zero queue counts and idle labels are omitted without adding permanent transcript decorations
- **AND** filtering completion candidates preserves the input anchor

#### Scenario: Model bindings differ
- **WHEN** active work retains an earlier binding while the selected next model differs
- **THEN** during running, paused or response-pending work the ordinary model label identifies the active binding and a conditional next-model cue explains the difference
- **AND** /status retains both complete binding observations

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

### Requirement: Terminal presentation has no renderer-generated role prefixes
The TUI SHALL present semantic actions, content and authoritative states without
generating role labels of the form `xxx>`. This requirement SHALL cover transcript,
live status, pending requests, errors, expanded details and re-rendered retained
history. It MUST NOT remove or rewrite identical literal text from user input,
Agent answers, code, Markdown quotations or Tool output. Input route markers and
request-response semantics SHALL retain their existing behavior.

#### Scenario: Every semantic output surface uses the new presentation
- **WHEN** Alan renders a Tool, plan, pending input, error, thinking detail or another role-bearing UI element
- **THEN** it does not add a role name followed by `>`
- **AND** meaningful operations, statuses, request choices and error information remain distinguishable

#### Scenario: Literal content resembles a former prefix
- **WHEN** input, an answer, code or Tool content contains `tool>`, `server> ready`, `a > b` or a Markdown quotation
- **THEN** the renderer preserves that source content under its ordinary content-rendering rules
- **AND** it does not apply a global prefix-removal pass

#### Scenario: Older history is rendered
- **WHEN** retained structured history can be rendered again
- **THEN** newly generated UI uses the new presentation without rewriting durable evidence
- **AND** opaque historical text and already committed native scrollback are not guessed at or rewritten

### Requirement: Output hierarchy communicates actions and states
The TUI SHALL distinguish answer content, routine activity, requests and failures
through concrete wording and restrained visual hierarchy. It SHALL keep normal
activity compact and preserve authoritative failed, rejected, cancelled, waiting
and unknown states. Color or icons alone MUST NOT convey essential outcomes.
Detail discovery SHALL remain visible in the applicable context without
repeating an identical hint on every routine Action.

#### Scenario: Routine work precedes an answer
- **WHEN** several Tools complete before an Agent answer
- **THEN** their summaries omit redundant titles, paths and repeated detail-hint rows
- **AND** the answer remains visually distinct from routine activity

#### Scenario: Attention is required
- **WHEN** a request, failure or unknown outcome is displayed
- **THEN** its actual state and available user action are explicit
- **AND** it is not hidden inside a successful activity summary or converted into a success

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

### Requirement: Assistant output and thinking render incrementally
The terminal UI SHALL render assistant text and thinking as they arrive from the
Agent Process output files and SHALL show elapsed thinking time while thinking is
streaming. The final message SHALL reflect the reconciled tape content.

#### Scenario: A long answer streams
- **WHEN** the model emits text over several seconds
- **THEN** the visible transcript grows before the response completes
- **AND** the final transcript matches the reconciled message

### Requirement: Leaving a modal never stops input
The terminal UI SHALL close only the active modal, chooser, form or project
selection when it is quit or dismissed with Ctrl+Q or Esc. The terminal input
reader SHALL keep running until the application itself exits.

#### Scenario: Ctrl+Q inside the model chooser
- **WHEN** the model chooser is open and the user presses Ctrl+Q
- **THEN** the chooser closes and the composer accepts the next key

### Requirement: Idle Esc and modified characters are inert
Esc at an idle prompt SHALL dismiss completion or do nothing and SHALL NOT send an
interrupt. A character key pressed with Control or Alt that has no binding SHALL
NOT be inserted into the composer.

#### Scenario: Esc at an idle prompt
- **WHEN** no turn, request or modal is active and the user presses Esc
- **THEN** no interrupt is sent

### Requirement: Secret entry does not echo
Interactive secret entry SHALL disable terminal echo while reading from a terminal
and SHALL restore it on every exit path, while piped input remains supported.

#### Scenario: Setting a secret in a terminal
- **WHEN** a user enters a secret at a terminal prompt
- **THEN** the typed characters are not displayed

### Requirement: Terminal behavior is verified on a real PTY
Terminal acceptance SHALL drive the built binary through a Unix pseudo-terminal and
a terminal-state model covering startup frame, resize reflow, wide text, Ctrl+C,
Esc and modal exit. No terminal-host harness SHALL be required.

#### Scenario: Acceptance runs without a terminal host
- **WHEN** the PTY acceptance suite runs in CI
- **THEN** it passes using only the built binary and the terminal-state model

### Requirement: Approval posture and execution isolation are distinguishable
The terminal SHALL distinguish governed versus fully trusted posture and actual confined versus Host-account execution. Activation SHALL show both choices through a trusted-user control; compact presentation MAY use a short indicator with details in status, but MUST NOT hide a broad Host-account execution state or imply unavailable confinement.

#### Scenario: Narrow trusted terminal
- **WHEN** the user runs fully trusted Host-account execution in a narrow pane
- **THEN** a visible indicator and accessible details identify broad native execution without displacing input

#### Scenario: Confinement unavailable
- **WHEN** the selected backend cannot provide its promised boundary
- **THEN** the UI reports the limitation rather than displaying an ordinary protected state

### Requirement: Terminal continuity controls preserve explicit user selection
The terminal SHALL offer authorized history inspection, explicit recovery selection
and fresh start, plus inspect/correct/delete controls for derived project summaries.
It SHALL show source provenance and authoritative pending, unavailable and failure
states. Reading a summary MUST NOT execute work. Deletion controls SHALL distinguish
derived summary recall from retained execution evidence.

#### Scenario: User chooses fresh start
- **WHEN** history exists but the user selects fresh work
- **THEN** no prior pending operation is launched or old authority inferred

#### Scenario: User deletes a summary
- **WHEN** a user chooses the summary deletion control
- **THEN** the UI explains which derived context is removed and that source execution evidence is retained separately

#### Scenario: History cannot be read
- **WHEN** an authorized history reference is unavailable or malformed
- **THEN** the terminal presents that failure without pretending recovery or execution succeeded

### Requirement: Sanitized GitHub issue intake slash command
The file-backed TUI SHALL provide `/issue` (with alias `/bug`) to draft a bug
report or enhancement request from strictly redacted session diagnostics,
require explicit user review and confirmation before any outbound action, and
submit through mounted `/mnt/github`, authenticated `gh` CLI, or a prefilled
GitHub web URL fallback.

#### Scenario: Drafting an issue includes redacted diagnostics and waits for confirmation
- **WHEN** the operator invokes `/issue` or `/bug` in the TUI
- **THEN** the TUI constructs an editable draft containing only redacted runtime metadata (Alan version, OS/architecture, active provider/model label, permission posture, and latest error or notice summary)
- **AND** it displays the draft for confirmation without sending any network request automatically

#### Scenario: Confirmed submission uses mounted githubfs, gh CLI, or web URL fallback
- **WHEN** the operator confirms the drafted issue
- **THEN** the TUI writes to `/mnt/github/repos/<owner>/<repo>/issues/new` if writable in the session namespace
- **AND** otherwise invokes authenticated `gh issue create` when available or presents a bounded prefilled GitHub `issues/new` web URL when neither `/mnt/github` nor authenticated `gh` is available

#### Scenario: Cancelling issue intake sends nothing
- **WHEN** the operator cancels the `/issue` confirmation prompt
- **THEN** no file write, CLI subprocess, or outbound network action occurs
- **AND** the active session returns cleanly to normal input mode

#### Scenario: Secrets and unapproved transcript content are excluded
- **WHEN** the TUI assembles the default `/issue` diagnostic payload during a session containing project files, environment secrets, or prior conversation messages
- **THEN** full conversation transcripts, project source contents, environment variables, and credentials MUST NOT be included automatically in the draft

### Requirement: Single-viewport progressive disclosure for workstreams and focus switching
The Rust inline TUI SHALL preserve its single-viewport, scrollback-safe POSIX/VT layout without adding a permanent sidebar or workspace manager. It SHALL display calm inline receipts whenever Root routes or hands off work, show compact active/idle/inbox counts in the status bar, provide `/streams` for bounded on-demand workstream inspection and management, and provide `/focus <name>` and `/root` to switch the non-owning TUI file client between `/agent/<child_pid>` and `/agent/root`.

#### Scenario: Inline routing receipt and compact status badge appear without sidebar
- **WHEN** Root dispatches a request to a named workstream or processes a `routefs` handoff
- **THEN** the transcript renders a concise inline routing receipt and the status bar updates compact stream and inbox counts
- **AND** no permanent sidebar or full-screen panel is added

#### Scenario: User switches focus between Root and a child workstream
- **WHEN** the user runs `/focus <name>` and later `/root`
- **THEN** the non-owning TUI switches its mounted target between `/agent/<child_pid>` and `/agent/root`
- **AND** terminal scrollback and narrow-pane resize reflow remain intact

