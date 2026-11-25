# Spec Delta

## ADDED Requirements

### Requirement: Named Workstreams support persistent resumption and one-off auto-collapse
Root SHALL manage named child workstreams recorded in the project's System Store metadata. A `persistent` workstream SHALL save its latest checkpoint reference on turn completion and transition to `idle` for later resumption into a fresh child Process, whereas a `one_off` workstream SHALL automatically transition to `archived` upon completion and collapse out of the active roster while retaining inspectable durable rollout evidence.

#### Scenario: Root resumes a persistent workstream
- **WHEN** Root dispatches new work matching an existing `idle` `persistent` workstream
- **THEN** Alan spawns a new child Agent Process from that workstream's latest checkpoint after revalidating current grants and budgets

#### Scenario: One-off workstream collapses automatically after completion
- **WHEN** a `one_off` child workstream completes its task
- **THEN** its record transitions to `archived` and collapses from the active workstream list
- **AND** its durable rollout evidence remains inspectable through existing evidence paths

### Requirement: Foreground Root supports in-instance asynchronous workstream concurrency with tiered write isolation
Foreground Root SHALL run multiple child workstreams asynchronously while staying responsive. Read-only streams SHALL run concurrently; routine writes on the same Host Mount SHALL serialize via a single-writer queue; and concurrent long-running writes SHALL isolate in an on-demand `git worktree` with narrowed write grants and explicit merge handoff.

#### Scenario: Read-only workstreams run concurrently while Root stays interactive
- **WHEN** Root dispatches two `read_only` research workstreams asynchronously
- **THEN** both child Agent Processes execute concurrently inside the foreground instance
- **AND** Root continues accepting and responding to subsequent user prompts

#### Scenario: Routine write workstreams serialize on the shared working tree
- **WHEN** a second routine write workstream targets a Host Mount currently held by an active `shared_queue` writer
- **THEN** it waits in the per-grant FIFO writer queue until the first writer completes and releases the lock

#### Scenario: Concurrent long-running write workstreams isolate via git worktree
- **WHEN** concurrent long-running write execution is requested on a repository with an active writer
- **THEN** Alan allocates an isolated `git worktree` and narrows the new child Process's Host Mount write grant to that worktree path
- **AND** on completion the child returns a diff and merge handoff rather than silently mutating the primary working tree

### Requirement: Per-stream lease locks prevent concurrent checkpoint corruption
Every interactive resumption or scheduled Stateful Cron Stream execution targeting a named workstream SHALL acquire that stream's System Store lease lock before mutating its checkpoint or rollout pointer.

#### Scenario: Concurrent stream resume is prevented by per-stream lease lock
- **WHEN** an interactive session holds the lease lock for a named stream and a second caller attempts to resume that same stream
- **THEN** the second caller observes lock contention and aborts or queues without corrupting the active checkpoint
