# alan-renderer-host-contract Specification

## Purpose
Define what an Alan renderer host is in the Plan 9 model: a client that renders
from alan9 file surfaces and expresses user input as file writes and `ctl`
commands. This replaces the retired "semantic view snapshot" pull model
(ADR-0024); renderer hosts own presentation, never runtime truth.

## Requirements

### Requirement: Renderer hosts project mounted alan9 file state

Alan renderer hosts SHALL derive durable presentation state from files under `/proc`, `/agent`, and mounted service trees, and SHALL translate user actions into file or `ctl` writes.

#### Scenario: Renderer host boundary is reviewed

- **WHEN** an Alan renderer host is reviewed
- **THEN** its durable truth source is the mounted alan9 namespace
- **AND** it owns presentation only, not Process, Agent Machine, or service truth

### Requirement: A mounted namespace is sufficient for local renderer launch

A local renderer host SHALL start from a mounted alan9 root plus a concrete Agent Process path.

#### Scenario: Renderer opens a root Agent Process

- **WHEN** the renderer receives a namespace root and `/agent/root`
- **THEN** it reads and tails AgentFS output and state files
- **AND** it writes input and Process control through the corresponding files

### Requirement: Renderer attachments use Process Reference and offsets
An Alan renderer host SHALL identify a Process by boot ID and PID, verify it
through `/proc`, and own the byte offsets of each stream it reads. Recreating or
duplicating a renderer MUST NOT create, restore, or become authority for the
Process.

#### Scenario: Second renderer attaches
- **WHEN** another renderer opens the same Agent Process
- **THEN** it maintains independent fids and offsets
- **AND** both observe the same alan9 file authority

### Requirement: Retention gaps are visible
When a saved stream offset can no longer be served, the renderer SHALL surface
the gap and MUST NOT silently jump forward or claim complete continuity.

#### Scenario: Saved offset predates retained history
- **WHEN** a renderer reattaches after retention has truncated earlier bytes
- **THEN** it reports the missing range before continuing

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

### Requirement: Root Agent interruption is turn-scoped
The renderer SHALL interrupt a running Root Agent turn through the Agent Runtime
control file `/agent/root/machine/ctl`. It MUST NOT send turn interruption to
`/proc/<pid>/ctl`, which owns Process lifecycle. The Root Agent SHALL remain
usable for subsequent input after a turn is interrupted.

#### Scenario: Ctrl-C interrupts a running turn
- **WHEN** the user presses Ctrl-C while a Root Agent turn is running
- **THEN** the renderer writes the Agent Runtime interrupt control
- **AND** the Root Agent Process remains running and accepts a subsequent task

#### Scenario: Interrupt arrives before a submitted turn starts
- **WHEN** the user presses Ctrl-C or Escape after submitting a Root Agent task
  but before observing that task as active
- **THEN** the renderer keeps the turn interrupt pending until the submitted
  task is observed as running or paused
- **AND** it discards the pending interrupt if the task settles before becoming
  active

#### Scenario: Renderer exits
- **WHEN** the user quits or closes the local renderer
- **THEN** it closes its own file streams and restores the terminal
- **AND** it does not stop the shared alan9 Host or Root Agent Process
