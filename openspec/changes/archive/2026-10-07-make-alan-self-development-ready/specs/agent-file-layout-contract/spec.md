## ADDED Requirements

### Requirement: Request cancellation belongs to the request owner
AgentFS SHALL expose a writable `ctl` on each request object at
`/agent/<pid>/requests/<request-id>/ctl`. An authorized descriptor write of
`cancel` SHALL atomically transition only that pending request to `cancelled`
under the same owner lock used to settle responses. Cancellation SHALL leave
its response unchanged and SHALL publish request/status invalidation through
the existing event surfaces. Request `status` SHALL remain read-only.

Cancellation of an already cancelled request SHALL be idempotent. An answered
or closed request SHALL retain its actual terminal state and response when
cancellation races with settlement; cancellation SHALL NOT overwrite either.
Malformed control, unavailable requests, and missing write rights SHALL fail
without mutating any request or granting execution authority.

#### Scenario: A pending request is cancelled
- **WHEN** the owning Agent Process cancels its exact pending request through its request control descriptor
- **THEN** AgentFS publishes `cancelled` for that request with its response unchanged
- **AND** request watchers invalidate the outstanding question or confirmation
- **AND** unrelated requests and their responses remain unchanged

#### Scenario: A response descriptor predates cancellation
- **WHEN** a response descriptor opened while a request was pending is closed after cancellation wins
- **THEN** response settlement is rejected against the current terminal status
- **AND** the response and cancelled state are not overwritten and the Process is not resumed

#### Scenario: An actual answer wins a cancellation race
- **WHEN** an authorized response has already settled the request before its cancel operation
- **THEN** the existing answered state and actual response remain authoritative
- **AND** cancellation does not manufacture a rejection, approval, or second terminal event

### Requirement: Machine cancellation settles its owned requests before reset
The Agent Execution Engine SHALL settle only the confirmation and structured
input requests associated with the current Machine logical waits before
clearing those waits at the shared turn reset or cancellation boundary.
It SHALL address the service-assigned request IDs within the pinned actual
Agent Process path through ordinary namespace descriptors. A checkpoint ID
SHALL NOT be substituted for a request directory ID. Host Mount requests SHALL
continue to settle through their owning service and existing durable evidence.

Request cancellation or terminal-evidence failure SHALL preserve any unsettled
logical association and SHALL NOT be treated as successful reset. Engine
cancellation SHALL NOT synthesize a user response, scan and cancel unrelated
pending requests, make status writable, or introduce a direct Engine dependency
on AgentFS. Existing Process lifecycle and input cancellation evidence SHALL
remain authoritative.

#### Scenario: A confirmation is interrupted
- **WHEN** cancellation interrupts a Tool while its confirmation is pending
- **THEN** the Machine-associated AgentFS request reaches truthful terminal state before its logical wait is cleared
- **AND** existing watchers remove the approval prompt so ordinary shell commands and natural exit are usable
- **AND** a cancellation that wins before approval does not spawn the Tool, grant authority, or replay queued work

#### Scenario: Cancellation cannot settle its request
- **WHEN** the owner operation or required terminal evidence fails
- **THEN** the corresponding Machine wait association remains recoverable
- **AND** the Engine does not claim successful cancellation or reset for that unsettled association
