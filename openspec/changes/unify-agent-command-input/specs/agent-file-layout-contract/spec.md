## MODIFIED Requirements

### Requirement: Agent Machine persistence is Process-owned
Agent Runtime Service SHALL persist and restore Agent Machine tape, transition state, checkpoints,
requests, actions, and renderer projections through files owned by the Agent Process and its
durable backing stores. The Process path and durable record identifiers SHALL be sufficient to
locate and interpret that state.

Accepted submission identity, intent, queue ordering/pause and shared cwd
recovery evidence SHALL use these owners. Reliable pending work is restored
paused; unknown effects are reconciled and never automatically replayed.
Missing or untrustworthy records are reported rather than fabricated.

#### Scenario: An Agent Process resumes durable machine state
- **WHEN** Agent Runtime Service restores an Agent Machine from durable rollout or checkpoint files
- **THEN** the restored state is associated with the concrete Agent Process and AgentFS layout
- **AND** the rollout or checkpoint retains the durable provenance needed to interpret the state

### Requirement: AgentFS is the complete observable runtime-state boundary
AgentFS SHALL own Agent Process output, tape, requests, actions, machine
snapshots, renderer-safe UI state, and ordered update streams under `/agent`,
while `/proc` SHALL own generic Process state. Hosts and supervisors SHALL NOT
require an engine handle, callback, or broadcast receiver to observe equivalent
live state.

The complete boundary SHALL include input route, correlated submission outcome,
shared cwd and pending queue/pause state. Multiple clients observe the same
authority through files.

#### Scenario: Host attaches after a turn has started
- **WHEN** a host opens an already-running Agent Process
- **THEN** it hydrates snapshots and resumes streams from `/agent/<pid>` and
  reads generic lifecycle from `/proc/<pid>`
- **AND** attachment does not depend on having received earlier in-memory events

#### Scenario: Parallel live state API is added
- **WHEN** current engine code exposes output, request, action, machine, or UI
  state through a callback or publish/subscribe channel
- **THEN** repository verification fails because the owning file surface is the
  complete observable boundary

### Requirement: Tape and event streams are append-only and leased during generation
alan9 SHALL keep `machine/tape` and every `events` stream append-only. While an
agent is generating, `machine/tape` SHALL be held under an exclusive-write lease —
exactly one writer (the generating engine), while readers may still tail it — so
no second writer can interleave records into the tape mid-stream. The safe window
for an external actor to amend `machine/tape` or `context/` is the agent's
yielded/paused state.

The same exclusive writer exclusion SHALL cover deterministic command transitions
that update Tape. A paused ordinary queue does not itself grant any external
writer permission; the existing protocol-layer lease prerequisites still apply.

#### Scenario: A second writer attempts the tape during generation
- **WHEN** an agent is generating and another writer attempts to write
  `machine/tape`
- **THEN** the write is refused because the generating engine holds the
  exclusive-write lease
- **AND** the tape cannot be spliced mid-stream

#### Scenario: A reader tails the tape during generation
- **WHEN** a consumer tails `machine/tape` while the agent is generating
- **THEN** the read succeeds and resumes from the caller's offset
- **AND** the exclusive-write lease bars writers, not readers

#### Scenario: An external actor amends during a yield
- **WHEN** the agent is yielded/paused and an authorized actor amends
  `machine/tape` or `context/`
- **THEN** the amendment is accepted because no generation lease is held
- **AND** on resume the engine continues from the amended state

### Requirement: Agent input records declare routing explicitly
An input operation written to an Agent Process SHALL use the canonical
`type: "input"` record and SHALL include an explicit `mode`. Alan SHALL NOT
accept the retired `type: "steer"` alias or infer a mode when the field is
absent. Input scheduling mode and command/Agent intent SHALL remain distinct:
intent classification MUST NOT invent a missing protocol mode or reinterpret a
request response as ordinary input.

#### Scenario: Explicit input is submitted
- **WHEN** a caller submits `type: "input"` with a supported explicit mode and
  valid parts
- **THEN** the Agent Process routes the input according to that mode

#### Scenario: Input mode is absent
- **WHEN** a caller submits `type: "input"` without `mode`
- **THEN** the record is rejected as malformed
- **AND** Alan does not infer `steer` or any other routing behavior

#### Scenario: Retired steer operation is submitted
- **WHEN** a caller submits an operation with `type: "steer"`
- **THEN** the record is rejected as an unsupported operation shape
- **AND** the caller must resubmit canonical explicit input

## ADDED Requirements

### Requirement: Unified submissions and queue state are file-visible
AgentFS SHALL expose accepted submission identity, selected route, status,
ordered pending work, queue pause state and shared cwd through existing owning
Machine, UI and context surfaces. Explicit continue/discard controls SHALL use
the existing runtime control boundary. These records SHALL belong to the Agent
Process and MUST NOT introduce a global Session or renderer-owned queue.

#### Scenario: Another client attaches to paused work
- **WHEN** a client attaches after an interrupt paused queued submissions
- **THEN** it can inspect the paused queue and cwd through AgentFS
- **AND** it need not recover state from the original renderer

### Requirement: Command evidence feeds bounded shared Agent context
Direct commands and results SHALL use existing action, Process and durable
evidence owners and become available to subsequent Agent work. Model-input
projection SHALL include bounded output, exit status and an evidence reference,
with explicit truncation or retention loss. It MUST NOT require automatic model
summarization or a duplicated output database. Existing redaction, authorization
and retention rules SHALL apply to all evidence reads.

#### Scenario: A command produces large output
- **WHEN** output exceeds the model-input projection bound
- **THEN** later Agent input receives a bounded excerpt, status and evidence reference
- **AND** truncation is visible and further reads require the same access rights

#### Scenario: Output has expired under retention
- **WHEN** the Agent follows an output reference whose content is no longer retained
- **THEN** it receives an explicit unavailable or retention-gap result
- **AND** the interface does not claim complete history

#### Scenario: User refers to the prior command
- **WHEN** the user asks about the preceding direct command result
- **THEN** Agent reasoning can use its correlated command and evidence without asking the user to paste it
