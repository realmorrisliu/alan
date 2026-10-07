## MODIFIED Requirements

### Requirement: Interactive Alan uses a shell-like inline transcript
When `alan` runs in an interactive terminal, the renderer SHALL present one
continuous terminal transcript: accepted user input SHALL appear as a `: ` (Agent) or `! ` (command)
line, Agent output SHALL follow it in terminal order, and the next contextual two-line
prompt SHALL appear immediately after the output. The prompt SHALL NOT be pinned
to a full-height bottom composer. Committed transcript SHALL remain available
in the host terminal's scrollback.

#### Scenario: A task completes in the inline REPL
- **WHEN** a user submits a task to the attached Root Agent Process
- **THEN** the submitted line is shown with its `: ` or `! ` route marker
- **AND** Agent output follows in the same terminal flow
- **AND** the next editable `: ` prompt follows the output

#### Scenario: Completion candidates are opened
- **WHEN** a user enters a slash command or a `$` skill or `@` file reference
- **THEN** matching candidates appear in a temporary inline list adjacent to
  the current prompt
- **AND** arrow keys, Tab, Enter, and Escape retain their documented selection
  and dismissal behavior
- **AND** closing the list returns to the compact transcript-and-prompt layout

## ADDED Requirements

### Requirement: Foreground project authorization is discoverable
Interactive Alan SHALL offer a host-local project selection and authorization
flow through the existing Host Mount authority. Host cwd SHALL be only a
candidate until explicitly approved. The user SHALL see the requested access,
approve/change/cancel choices, and the resulting project-relative cwd. Raw Host
paths SHALL remain outside Agent-visible files.

#### Scenario: Fresh invocation has no project grant
- **WHEN** Alan starts inside a Host project without an active grant
- **THEN** it explains that project access is not authorized and offers `/project`
- **AND** neither rendering the suggestion nor pressing a default Enter grants access

#### Scenario: Agent requests a directory
- **WHEN** an Agent Process requests a Host Mount
- **THEN** the foreground CLI presents the label, reason and access scope with a native approval flow
- **AND** the user can complete or cancel it without discovering instance sockets or PIDs
- **AND** approval is recorded only by the existing Host Mount authority

#### Scenario: Project authority is revoked
- **WHEN** a selected grant is revoked or unavailable
- **THEN** Alan shows that the project is unavailable and offers a new explicit selection
- **AND** stale completion entries and execution do not retain that authority

#### Scenario: Project authorization reply is lost
- **WHEN** the Host approves a selected project but the client loses the reply
- **THEN** the client retains the operation identity allocated before submission and reports the outcome as unconfirmed
- **AND** explicit reconciliation against the same Host invocation returns the original grant outcome without granting again
- **AND** a different directory or access mode cannot reuse that operation identity
- **AND** another project selection remains unavailable until the outcome is resolved, while help and quit remain available
- **AND** an already-revoked grant is never recreated by retrying its original operation

#### Scenario: Completed directory selection is delivered again
- **WHEN** a completed project directory control is delivered again after later navigation
- **THEN** the existing completed Action identity prevents another cwd effect or duplicate Action
- **AND** restoring that Action evidence does not replay the original directory change

### Requirement: Accepted inputs have visible disposition
Alan SHALL acknowledge authoritative admission with a visible submitted input
and its running, queued or paused disposition, correlated with the existing
submission identity. It SHALL distinguish admission from completion and SHALL
NOT replay an input to repair missing display state.

Successful delivery to `io/input` alone SHALL NOT be treated as admission. The
Agent Machine SHALL publish the existing submission identity and its current
admitted disposition through AgentFS-owned renderer files only after the
durable admission boundary succeeds. Reattachment SHALL read an authoritative
snapshot to distinguish accepted paused work from unacknowledged delivery;
renderer timers, pending local entries and optimistic writes SHALL NOT supply
that authority. Observation failure SHALL remain explicit and SHALL NOT
implicitly continue, discard or resubmit work.

#### Scenario: Input is admitted behind paused work
- **WHEN** the user submits a follow-up to a paused queue and admission succeeds
- **THEN** its text appears once with a queued disposition and visible pending count
- **AND** the UI explains the existing continue/discard actions

#### Scenario: Delivery awaits authoritative admission
- **WHEN** the input stream accepts bytes but no matching Machine admission receipt has been observed
- **THEN** the UI shows an unconfirmed submission and preserves its exact identity
- **AND** it does not claim an accepted queue entry, settled cancellation or safe project-control boundary from that delivery alone

#### Scenario: A client reattaches to accepted paused work
- **WHEN** the same Agent Process publishes a reliable paused queue snapshot containing previously admitted input identities
- **THEN** the UI distinguishes those accepted pending entries from locally delivered but unconfirmed entries
- **AND** accepted paused work does not alone block explicit project reauthorization at the existing safe Machine boundary
- **AND** the control operation neither consumes the queue nor grants access without explicit Host Mount approval

#### Scenario: Admission fails
- **WHEN** the authoritative input write is rejected
- **THEN** the draft remains editable and the rejection is visible
- **AND** it is not displayed as accepted or completed

#### Scenario: Cancellation has not settled
- **WHEN** a cancellation request is sent but its result is not confirmed
- **THEN** the UI distinguishes the pending request from a confirmed cancellation
- **AND** local editing and help remain usable while the control operation waits
- **AND** unknown effects are never described as rolled back

### Requirement: Effective model selection is truthful
Alan SHALL expose the effective Connection profile and model, or explicitly
state that a value is unknown. Model selection SHALL use the active Connection's
catalog and authoritative binding; renderer-only labels SHALL NOT claim success.

#### Scenario: Model selection succeeds at an input boundary
- **WHEN** the user selects an available model and the owning Connection confirms the change
- **THEN** subsequently admitted inputs use that model
- **AND** already-admitted work retains its previous binding
- **AND** the displayed effective model reflects the confirmed selection

#### Scenario: Catalog or selection is unavailable
- **WHEN** the catalog cannot be read or the selection cannot be applied
- **THEN** Alan explains the failure without inventing model entries
- **AND** the last confirmed binding remains effective

### Requirement: Self-development readiness is demonstrated by execution
A claim that Alan supports supervised self-development SHALL be backed by a
real foreground CLI run in which Alan inspects its own instructions and source,
edits a bounded change in an explicitly authorized isolated checkout, runs
relevant checks and reports the actual diff. Independent review and a fresh
candidate launch SHALL verify the result. Approval and review do not substitute
for Alan performing the development work.

#### Scenario: A candidate completes a self-development task
- **WHEN** Alan produces a scoped source change and its focused check through governed Tools
- **THEN** acceptance records the source revision, diff, commands, exit statuses and candidate build identity
- **AND** an independent reviewer verifies the patch and the freshly launched behavior
- **AND** a fixture-only or operator-authored result is not counted as self-development

#### Scenario: A required operation is unavailable
- **WHEN** authorization, editing, build/test execution or recovery cannot be completed through the product flow
- **THEN** readiness remains unqualified and the specific failed gate is recorded
- **AND** hidden operator wiring or disabled sandbox protection does not count as passing
