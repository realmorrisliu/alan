## ADDED Requirements

### Requirement: Explicit Run State Transitions
The daemon and runtime SHALL expose coherent run states for active work, waiting for user input, approval resume, delegated progress, and terminal completion.

#### Scenario: Tool approval pauses a run
- **WHEN** a tool call requires user approval
- **THEN** the run state becomes `awaiting_approval` or an equivalent yielded state whose checkpoint identifies the pending approval

#### Scenario: Approval resumes execution
- **WHEN** the user approves a pending runtime confirmation and the runtime begins replaying or continuing the approved work
- **THEN** the run state transitions back to `running` before the resumed tool execution or next model step is observed by clients

#### Scenario: Turn completes after resume
- **WHEN** the resumed turn finishes
- **THEN** the run state transitions from `running` to a terminal state instead of remaining yielded until completion is inferred indirectly

### Requirement: Parent Timeline Shows Delegated Work
Parent session event streams SHALL expose delegated child start, progress, and terminal lifecycle events.

#### Scenario: Child starts
- **WHEN** a parent runtime launches a delegated child runtime
- **THEN** parent clients receive or can read a lifecycle event containing child id, target, workspace scope, and initial status before the child completes

#### Scenario: Child remains active
- **WHEN** a child runtime is running longer than a short UI-visible interval
- **THEN** parent clients can observe heartbeat, progress, or current-status metadata that distinguishes active delegated work from an idle parent

#### Scenario: Child completes
- **WHEN** a child runtime completes, fails, times out, or is terminated
- **THEN** parent clients receive or can read a terminal lifecycle event that links to the child-run record and delegated result handoff

### Requirement: Human-Readable Status Semantics
Client-facing run and child statuses SHALL be understandable without requiring users to inspect raw rollout files.

#### Scenario: Work waits on approval
- **WHEN** a run is waiting for an approval checkpoint
- **THEN** the client-visible status names the pending action and why approval is required

#### Scenario: Work is delegated
- **WHEN** a run is blocked on a delegated child
- **THEN** the client-visible status identifies the child target and current phase instead of showing only generic running or idle text

#### Scenario: Work has evidence limitations
- **WHEN** an answer or child result is based on partial, failed, or truncated evidence
- **THEN** debug or inspection surfaces expose that limitation through evidence metadata rather than requiring the user to infer it from missing text
