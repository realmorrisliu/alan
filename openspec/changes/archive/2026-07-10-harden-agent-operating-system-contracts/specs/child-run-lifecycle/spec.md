## ADDED Requirements

### Requirement: Parent-Visible Child Lifecycle Events
The system SHALL surface child-run lifecycle transitions to the parent session timeline in addition to updating the child-run registry.

#### Scenario: Child launch is recorded for parent clients
- **WHEN** a parent runtime launches a delegated child runtime
- **THEN** the parent session emits or persists a child lifecycle event with child run id, child session id when available, target, workspace scope, and initial status

#### Scenario: Child progress updates parent timeline
- **WHEN** a child runtime emits progress, heartbeat, tool, plan, or status metadata
- **THEN** the parent session emits or persists a bounded progress event linked to the child run

#### Scenario: Child terminal state updates parent timeline
- **WHEN** a child runtime completes, fails, yields, times out, is cancelled, or is terminated
- **THEN** the parent session emits or persists a terminal child lifecycle event with terminal status and handoff/evidence references when available

### Requirement: Delegated Tool Calls Do Not Hide Long-Running Children
The parent runtime SHALL expose child activity while an `invoke_delegated_skill` call is still pending.

#### Scenario: Delegated call has not returned
- **WHEN** a delegated child has started but the parent tool call has not yet returned its result
- **THEN** parent-visible lifecycle surfaces indicate that delegated work is active rather than leaving the parent turn with no observable progress
