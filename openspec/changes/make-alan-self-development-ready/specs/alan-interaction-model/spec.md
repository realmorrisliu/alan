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

### Requirement: Accepted inputs have visible disposition
Alan SHALL acknowledge authoritative admission with a visible submitted input
and its running, queued or paused disposition, correlated with the existing
submission identity. It SHALL distinguish admission from completion and SHALL
NOT replay an input to repair missing display state.

#### Scenario: Input is admitted behind paused work
- **WHEN** the user submits a follow-up to a paused queue and admission succeeds
- **THEN** its text appears once with a queued disposition and visible pending count
- **AND** the UI explains the existing continue/discard actions

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
