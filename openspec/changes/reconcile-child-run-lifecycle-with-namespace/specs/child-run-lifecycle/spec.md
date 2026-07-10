## ADDED Requirements

### Requirement: Child-Run Records Are A Projection, Not A Second Process Table
Child-run records SHALL be a delegation-scoped projection over `/proc`
parentage plus launch and handoff metadata. For process state (existence,
parentage, liveness, terminal exit), `/proc` and the agent overlay
(`children/`, aggregate `events`) SHALL be authoritative; a child-run record
that disagrees with them SHALL be treated as stale and corrected from the
authoritative surfaces, never the reverse.

#### Scenario: Record disagrees with /proc
- **WHEN** a child-run record reports a child as running while `/proc` shows the
  process exited
- **THEN** consumers and reconciliation logic treat the process state from
  `/proc` as truth and update the record to terminal

#### Scenario: Delegation metadata has no /proc equivalent
- **WHEN** a consumer needs delegation-scoped metadata (launch metadata,
  classified requirements, handoff references, termination actor/reason)
- **THEN** the child-run record is the owner of that metadata, because `/proc`
  deliberately does not model delegation semantics

## REMOVED Requirements

### Requirement: Daemon Child-Run Control Plane
**Reason**: No consumer exists. The file-backed TUI reads agent files, and the
macOS client's daemon API client has no child-run calls; the endpoints'
only exerciser is the daemon's own payload contract test. The surface also
contradicts `agent-file-layout-contract`: child visibility belongs to the agent
overlay derived from `/proc` parentage, not a parallel DTO control plane.
**Migration**: List/inspect children via `/agent/<pid>/children/` and the
aggregate `events` stream; terminate via generic process control on
`/proc/<pid>/ctl` (or the governed parent termination tool for delegated
children). Remote child visibility, if needed, is namespace projection owned by
`define-remote-access-service`. The `/api/v1/sessions/{id}/child_runs*` routes,
their endpoint-registry entries, and their payload-contract test coverage are
deleted.

### Requirement: TUI Child-Agent Commands
**Reason**: The commands (`/agents`, `/agent <id>`, `/agent terminate|kill`)
were specified against the daemon-backed TUI, which was removed
(`remove-daemon-backed-tui-compat`, archived 2026-07-08). The file-backed TUI
never implemented them, so the requirement describes a surface that does not
exist.
**Migration**: None required (no behavior to migrate). A file-surface-backed
child rendering for the TUI can be proposed as its own change, reading
`children/` and `machine/ui` surfaces.
