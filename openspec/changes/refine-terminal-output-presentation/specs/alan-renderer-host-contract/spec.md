## MODIFIED Requirements

### Requirement: The terminal CLI renders its invocation's Root Agent
A local terminal renderer SHALL receive its foreground invocation's mounted
alan9 namespace and the concrete instance-local `/agent/root` Agent Process
path. The CLI composition owns the foreground application lifetime; the
renderer MUST NOT create a Shell Process or become a separate execution
manager. AgentFS remains the authority for input, streamed output, status, and
Agent UI state. A renderer MUST NOT attach to another invocation as a fallback.

#### Scenario: Bare Alan opens its instance's terminal renderer
- **WHEN** bare `alan` runs with interactive stdin and stdout after its own
  alan9 instance has become ready
- **THEN** it opens the file-backed renderer on `/agent/root`
- **AND** it does not create a second Agent or Shell Process

#### Scenario: A user submits an ordinary Agent task
- **WHEN** the user submits an ordinary task entry through the composer, not a
  renderer-local slash command or a response to a pending form or Agent yield
- **THEN** one complete framed input is written to
  `/agent/root/io/input` and new Agent output is observed from
  `/agent/root/io/output`
- **AND** the renderer does not privately call a provider or Tool

#### Scenario: Root Agent Process changes between submissions
- **WHEN** the renderer submits a task and the Service Manager reports a new
  Root Agent PID
- **THEN** the renderer reopens its AgentFS tails against the current
  `/agent/root` and preserves the already-rendered transcript
- **AND** it merges a recovered turn only when current UI activity and its tape
  boundary can be correlated to that submission
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
- **THEN** the renderer inserts or updates only that Action's presentation member
- **AND** if that member belongs to an eligible read-only group, it recomputes only
  the affected uncommitted group projection without changing other members' states
- **AND** it does not append unrelated historical action snapshots or duplicate
  the changed Action's result

#### Scenario: Hydration does not replay older errors after later turns
- **WHEN** UI history contains an error followed by a newer `Running` event and
  later completed tape messages
- **THEN** hydration does not append that older error after the later messages
- **AND** errors after the latest `Running` event remain visible as the current
  or most recent turn outcome

#### Scenario: An explicit shell request is submitted
- **WHEN** the user submits ordinary Agent task text beginning with `!`
- **THEN** the input remains a framed AgentFS task and requests the exact
  remainder through the existing `bash` Tool
- **AND** the renderer does not execute a host command or bypass Agent Tool
  governance

#### Scenario: No callable Connection is configured
- **WHEN** the Root Agent has no callable Connection and the user submits a task
- **THEN** the renderer displays a clear unavailable-Connection error
- **AND** the Agent does not report a model-control incompatibility or start a
  provider request, including automatic pre-turn compaction
- **AND** the Host and Root Agent remain available


## ADDED Requirements

### Requirement: Grouped presentation preserves individual Action identity
A read-only activity group SHALL remain a presentation over ordered individual
Action references, not a new execution record or lifecycle owner. Detail selection
and asynchronous reads SHALL retain the selected member's identity and attachment
generation. Hydration and recovery SHALL preserve chronology and no-replay
semantics whether a result was displayed alone or in a group.

#### Scenario: A grouped Action is selected for detail
- **WHEN** the user selects a particular member of a read-only group
- **THEN** the renderer reads only that member's retained detail under the current attachment
- **AND** unavailable or truncated evidence is reported for that member without substituting another result
- **AND** closing detail restores the draft and does not execute any Action

#### Scenario: A late detail response belongs to an old selection
- **WHEN** a detail response arrives after its selected member or Process attachment changed
- **THEN** it does not replace the current selection's content
- **AND** identity is not inferred from matching titles or displayed text

#### Scenario: A grouped history prefix was partially committed before reconnect
- **WHEN** attachment recovery follows a partial history drain or terminal resize
- **THEN** matching preserves member identities and existing semantic history boundaries
- **AND** it neither duplicates represented members nor drops later distinct results with identical text
- **AND** it never resubmits input or re-executes an Action

#### Scenario: Historical ordering cannot be correlated
- **WHEN** completed Action records lack sufficient turn or order evidence
- **THEN** grouping does not guess their location or append them after an unrelated turn
- **AND** existing unknown-position hydration behavior remains in force
