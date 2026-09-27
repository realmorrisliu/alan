## RENAMED Requirements

- FROM: `### Requirement: The terminal CLI attaches to the existing Root Agent`
- TO: `### Requirement: The terminal renderer uses its foreground instance`

## MODIFIED Requirements

### Requirement: The terminal renderer uses its foreground instance
A local terminal renderer SHALL receive its foreground invocation's mounted
alan9 namespace and the concrete instance-local `/agent/root` Agent Process path. It MUST NOT spawn, restore, or
supervise an Agent Process. The CLI composition, outside the renderer, owns
the foreground application lifetime. A renderer MUST NOT connect to another
invocation as an implicit fallback. Root identity SHALL be pinned for this
invocation: an unexpected PID change or disappearance SHALL stop observation and
report failure, never reopen another Root or recover history automatically.
AgentFS remains the authority for input, streamed
output, status, and Agent UI state.

#### Scenario: Bare Alan opens the terminal renderer
- **WHEN** bare `alan` runs with interactive stdin and stdout after
  its own alan9 instance has become ready
- **THEN** it opens the file-backed renderer on `/agent/root`
- **AND** it does not create a second Agent or Shell Process

#### Scenario: A user submits an ordinary Agent task
- **WHEN** the user submits an ordinary task entry through the composer, not a
  renderer-local slash command or a response to a pending form or Agent yield
- **THEN** one complete framed input with correlated acceptance and route is written to
  `/agent/root/io/input` and new Agent output is observed from
  `/agent/root/io/output`
- **AND** the renderer does not privately call a provider or Tool

#### Scenario: Root Agent Process changes between submissions
- **WHEN** the pinned Root Agent PID changes or disappears between submissions
- **THEN** the renderer preserves its transcript and reports that its Agent is unavailable
- **AND** it stops admitting input to that attachment rather than selecting the new PID
- **AND** prior work can be recovered only by an explicitly selected fresh invocation

#### Scenario: Recovered Tape reconciles an existing assistant preview
- **WHEN** an explicitly selected fresh recovery loads durable Tape from work that previously streamed an assistant preview
- **THEN** it renders the recovered evidence once without importing another invocation's transient preview
- **AND** partial text alone is not presented as proof that the prior submission completed

#### Scenario: Queued events from a superseded attachment are discarded
- **WHEN** the pinned Root Agent identity changes while old watcher events remain queued
- **THEN** the renderer stops its watchers and discards their queued output, Tape, UI, action, request and watcher-error events
- **AND** it reports the identity failure and preserves visible transcript and unsent input
- **AND** it does not open replacement tails or submit preserved input to a new Process

#### Scenario: Root Agent Process changes while this renderer is idle
- **WHEN** `/agent/root` changes while this renderer has no submitted turn
- **THEN** the renderer reports the identity change without hydrating replacement history
- **AND** its existing transcript remains intact and work is not replayed

#### Scenario: Idle reattachment follows partially pruned scrollback
- **WHEN** the pinned Agent becomes unavailable after the renderer pruned old scrollback
- **THEN** the renderer retains the visible transcript and reports the unavailable Agent
- **AND** it does not automatically reattach or reconstruct pruned text from another Process

#### Scenario: Root Agent identity changes while a tail is opening
- **WHEN** Root identity changes between the renderer's history snapshot and tail open
- **THEN** the renderer rejects the inconsistent attachment and closes partial tails
- **AND** it reports failure instead of retrying hydration against another Process

#### Scenario: Renderer starts during stale Root Agent PID publication
- **WHEN** startup finds a stale or unavailable Root Agent publication
- **THEN** it reports startup failure after the existing bounded readiness window
- **AND** it does not attach to a replacement or another invocation as a fallback

#### Scenario: The replacement Root Agent fails before persisting the user turn
- **WHEN** a Root Agent created by explicit recovery emits correlated Running, Error and Idle events for new input without persisting that input to Tape
- **THEN** the renderer displays the correlated error and stops waiting for that submission
- **AND** it does not replay the input or start another recovery

#### Scenario: Hydration omits completed actions with unknown turn position
- **WHEN** Tape contains multiple completed turns and action snapshots contain
  completed Tools without a shared turn identifier
- **THEN** hydration preserves Tape order and does not append those Tool cells
  after the latest turn
- **AND** pending/running Tools remain visible in the live-tool region

#### Scenario: A live action status event changes only its own tool cell
- **WHEN** an action creation or status event is observed after attachment
- **THEN** the renderer inserts or updates only that action's Tool cell
- **AND** it does not append unrelated historical action snapshots or duplicate
  the changed action's result

#### Scenario: Hydration does not replay older errors after later turns
- **WHEN** UI history contains an error followed by a newer `Running` event and
  later completed tape messages
- **THEN** hydration does not append that older error after the later messages
- **AND** errors after the latest `Running` event remain visible as the current
  or most recent turn outcome

#### Scenario: An explicit shell request is submitted
- **WHEN** the user submits input beginning with `!`
- **THEN** it remains a framed AgentFS submission whose exact body selects the
  Machine's governed native shell command operation
- **AND** the renderer does not privately execute a host command or bypass Agent Tool
  governance

#### Scenario: No callable Connection is configured
- **WHEN** the Root Agent has no callable Connection and the user submits work requiring generation
- **THEN** the renderer displays a clear unavailable-Connection error
- **AND** the Agent does not report a model-control incompatibility or start a
  provider request, including automatic pre-turn compaction
- **AND** the Host and Root Agent remain available

### Requirement: Root Agent interruption is turn-scoped
The renderer SHALL interrupt a running Root Agent turn through the Agent Runtime
control file `/agent/root/machine/ctl`. It MUST NOT send turn interruption to
`/proc/<pid>/ctl`, which owns Process lifecycle. The Root Agent SHALL remain
usable after interruption. Accepted interrupt SHALL pause queued ordinary work
for explicit continuation or discard; no next action SHALL start automatically.

#### Scenario: Ctrl-C interrupts a running turn
- **WHEN** the user presses Ctrl-C while a Root Agent turn is running
- **THEN** the renderer writes the Agent Runtime interrupt control
- **AND** the Root Agent Process remains running and accepts a subsequent task

#### Scenario: Interrupt arrives before a submitted turn starts
- **WHEN** Ctrl-C or Escape targets an accepted submission that has not started
- **THEN** runtime control cancels that identified pending submission and pauses ordinary queued work
- **AND** it does not wait for generation-specific Running evidence or interrupt an unrelated submission
- **AND** if that submission already settled the control does not cancel later work

#### Scenario: Renderer exits
- **WHEN** the user quits or closes the local renderer in the owning Alan invocation
- **THEN** it closes its file streams and restores the terminal
- **AND** the application shuts down its owned instance through existing lifecycle boundaries
- **AND** terminal-host view detach while retaining the process does not count as Alan exit

## ADDED Requirements

### Requirement: Unified input presentation reflects Agent-owned state
Interactive clients SHALL display each submission's selected command or Agent
route and the shared Agent cwd from its file surfaces. Route display SHALL NOT
add an unconditional approval gate. Direct command completion SHALL present its
result without automatically generating commentary. Renderers MUST NOT own a
private cwd, execution queue, command executor or durable result database.

#### Scenario: Two clients observe a directory change
- **WHEN** an explicit `cd` completes in their shared Agent Process
- **THEN** both clients observe the same updated cwd from runtime-owned files

#### Scenario: Command completes without generation
- **WHEN** a direct command finishes
- **THEN** the interface shows its result and status without starting an explanatory model call

### Requirement: Terminal EOF and redirected EOF have distinct transport meanings
Interactive Ctrl-D with empty input and no pending Agent input SHALL exit the
foreground Alan invocation, as SHALL explicit quit. With a confirmation or structured-input request
pending, Ctrl-D MUST leave the client attached and the request available for a
response. The application SHALL shut down its owned work on actual exit.
A terminal host MAY keep the process alive when only its view detaches; Alan
MUST NOT start a background replacement to preserve execution.
Redirected EOF SHALL finish collection of one submission rather than cancel
execution.

#### Scenario: Empty terminal input receives Ctrl-D
- **WHEN** Ctrl-D is pressed with an empty composer and no pending Agent input
- **THEN** the renderer exits and the application shuts down its owned instance
- **AND** completed effects are not rolled back and uncertain work is not replayed

#### Scenario: Pending Agent input receives Ctrl-D
- **WHEN** Ctrl-D is pressed while a confirmation or structured-input request is pending
- **THEN** the client remains attached and the request stays available for a response

#### Scenario: Pipe reaches EOF
- **WHEN** redirected input reaches EOF
- **THEN** Alan submits the complete collected input once and follows its outcome

### Requirement: Composer prompts communicate one-shot input intent
The ordinary composer SHALL default to `alan: ` for Agent input. A leading `!`
in an empty entry SHALL be represented by `alan! `, with the command body after
the prompt. The renderer SHALL preserve canonical submission intent while folding
its prefix into presentation. Backspace on an empty command body SHALL restore
`alan: `. After accepted submission a fresh composer SHALL default to `alan: `;
rejection SHALL preserve the draft and intent. Explicit `:` SHALL force Agent
intent without a duplicate displayed delimiter. No persistent command mode or
renderer-owned execution path SHALL be introduced.

#### Scenario: User selects a command
- **WHEN** the user types `!` into an empty ordinary composer
- **THEN** the prompt becomes `alan! ` with an empty command body
- **AND** typing `git status` submits canonical command intent with exactly that body

#### Scenario: User leaves an empty command entry
- **WHEN** the command body is empty and the user presses Backspace
- **THEN** the prompt returns to `alan: ` with an empty Agent entry
- **AND** no command is submitted

#### Scenario: Accepted command does not leave a persistent mode
- **WHEN** a command submission is accepted
- **THEN** the next fresh entry uses `alan: ` even if accepted work is still running
- **AND** an empty override rejected before acceptance retains its draft and route

#### Scenario: Paste follows the same prefix rules
- **WHEN** a paste beginning with `!git status` is inserted into an empty entry
- **THEN** the prompt shows `alan! ` and the remaining text is the command body
- **AND** `!` inserted within an existing body is literal content rather than a mode switch

#### Scenario: Explicit Agent prefix protects literal command-looking text
- **WHEN** the user enters `:!explain this text`
- **THEN** the prompt is `alan: ` and the visible body is `!explain this text`
- **AND** the canonical submission retains forced-Agent intent without reparsing the body

#### Scenario: History restores command intent
- **WHEN** a previously submitted command is recalled as an editable draft
- **THEN** the prompt and canonical intent agree and the original body is preserved
- **AND** multiline cursor positioning and resizing account for the visible prompt width

#### Scenario: Pending request receives a prefix character
- **WHEN** input answers an existing form or confirmation
- **THEN** `!` and `:` remain response data under the existing request handler
- **AND** the ordinary composer route-switch behavior does not intercept them
