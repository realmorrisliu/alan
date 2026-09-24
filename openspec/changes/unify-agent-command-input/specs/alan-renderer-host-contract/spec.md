## MODIFIED Requirements

### Requirement: The terminal CLI attaches to the existing Root Agent
A local terminal renderer SHALL receive a mounted alan9 namespace and the
concrete `/agent/root` Agent Process path. It MUST NOT spawn, restore, or
supervise an Agent Process. AgentFS remains the authority for input, streamed
output, status, and Agent UI state.

#### Scenario: Bare Alan opens the terminal renderer
- **WHEN** bare `alan` runs with interactive stdin and stdout after attaching to
  the dedicated Host
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
- **WHEN** the renderer submits a task and the Service Manager reports a new
  Root Agent PID
- **THEN** the renderer reopens its AgentFS tails against the current
  `/agent/root` and preserves the already-rendered transcript
- **AND** it merges a recovered turn only when current UI activity and its execution evidence
  can be correlated to that submission
- **AND** if the replacement is idle without correlated turn evidence, it
  renders an outcome-unknown error instead of reusing an older identical
  prompt or stale UI error
- **AND** it never resubmits the input

#### Scenario: Recovered Tape reconciles an existing assistant preview
- **WHEN** the old Process streamed the submitted turn's assistant preview
  before reattachment and the two answers are equal or one is a prefix of the
  other
- **THEN** the renderer keeps the longer compatible answer, using Tape when it
  extends the preview
- **AND** it does not display the current-turn answer twice

#### Scenario: Queued events from a superseded attachment are discarded
- **WHEN** the Root Agent PID changes while old watcher events remain queued
- **THEN** the renderer discards queued output, Tape, UI, action, request, and
  watcher-error events before hydrating the replacement Process
- **AND** it preserves queued terminal input and terminal-reader errors
- **AND** events from the replacement tails update only the replacement view

#### Scenario: Root Agent Process changes while this renderer is idle
- **WHEN** `/agent/root` is rebound while this renderer has no submitted turn
  and the replacement Process already has completed AgentFS history
- **THEN** the renderer preserves its existing transcript and appends the
  replacement history not already represented there
- **AND** it does not duplicate an unambiguous shared suffix or resubmit work
- **AND** when identical retained history appears more than once, it keeps the
  replacement turns after the earliest matching window rather than dropping
  intervening turns

#### Scenario: Idle reattachment follows partially pruned scrollback
- **WHEN** the renderer's first retained history cell was partially pruned to
  bound scrollback before the Root Agent changed
- **THEN** it matches the retained rendered-text suffix against replacement
  history and appends only the replacement history after that match
- **AND** it does not replay the full pruned cell

#### Scenario: Root Agent identity changes while a tail is opening
- **WHEN** the Service Manager changes the Root Agent PID between the renderer's
  history snapshot and tail open
- **THEN** the renderer opens all history snapshots and watcher tails against
  one concrete `/agent/<pid>` path
- **AND** it discards that attachment and retries if the reported PID changed
  before hydration is complete

#### Scenario: Renderer starts during stale Root Agent PID publication
- **WHEN** the supervisor has detached the Root Agent but the Service Manager
  still publishes its old PID
- **THEN** the renderer retries hydration within a bounded startup window
- **AND** it attaches to the replacement if the published PID changes during
  that window
- **AND** it surfaces the attachment error if the bounded retries are exhausted

#### Scenario: The replacement Root Agent fails before persisting the user turn
- **WHEN** a replacement Root Agent emits a post-submission `Running`, an
  `Error`, and then `Idle` without writing the user message to tape
- **THEN** the renderer preserves that correlated error in its transcript
- **AND** it stops polling for this turn after rendering the terminal outcome

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
- **WHEN** the user quits or closes the local renderer
- **THEN** it closes its own file streams and restores the terminal
- **AND** it does not stop the shared alan9 Host or Root Agent Process

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
Interactive Ctrl-D with empty input and no pending Agent input SHALL detach, as
shall an explicit client exit. With a confirmation or structured-input request
pending, Ctrl-D MUST leave the client attached and the request available for a
response. Detach MUST NOT stop accepted work, the Agent Process or the Host.
Redirected EOF SHALL finish collection of one submission rather than cancel
execution.

#### Scenario: Empty terminal input receives Ctrl-D
- **WHEN** Ctrl-D is pressed with an empty composer and no pending Agent input
- **THEN** the client detaches and already accepted work continues

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

#### Scenario: Automatic command routing is considered for activation
- **WHEN** a future classifier may route unprefixed text to direct execution
- **THEN** its qualified presentation contract must visibly distinguish that command route
- **AND** it must not silently execute a direct command represented as conversation under `alan: `
