# Spec Delta

## ADDED Requirements

### Requirement: Delegated child and workstream supervision waits on kernel and agent event streams
`DelegatedChildRunSupervisor` and `async_workstreams` SHALL wait on `/proc/<pid>/wait` and `/agent/<pid>/events` streams rather than fixed `10–250ms` sleep polling, and parent-owned child launches SHALL set `link_parent: true`.

#### Scenario: Child run supervisor reacts to child exit or yield without sleep polling
- **WHEN** a delegated child agent pauses on a request or exits
- **THEN** `DelegatedChildRunSupervisor` wakes directly from `/agent/<child_pid>/events` or `/proc/<child_pid>/wait` within timeout/cancellation bounds

#### Scenario: Parent cancellation cascades to linked child agent
- **WHEN** a parent agent is aborted or exits while supervising a `link_parent: true` child agent
- **THEN** the kernel cancels the child process deterministically and workstream quiescence observes completion without fixed polling ticks
