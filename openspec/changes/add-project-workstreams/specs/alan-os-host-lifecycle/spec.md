# Spec Delta

## ADDED Requirements

### Requirement: Explicitly bound projects auto-load a read-only Project Ledger
When bare `alan` starts in an explicitly bound project directory, it SHALL load that project's read-only Project Ledger and unread workflow inbox summary without replaying effects or restoring revoked grants. Foreign checkouts SHALL NOT auto-bind by Git remote, and `alan --ephemeral` or `/new` SHALL bypass the Ledger.

#### Scenario: Bound project directory loads read-only Ledger on startup
- **WHEN** a user runs bare `alan` in an explicitly bound project directory
- **THEN** Root Agent loads the project's read-only Ledger and unread `inbox/` summary
- **AND** current Host Mount grants and `task_budget` limits are revalidated rather than inherited as live capabilities
- **AND** no previously completed side effect is replayed automatically

#### Scenario: Ephemeral invocation bypasses Project Ledger
- **WHEN** a user runs `alan --ephemeral` or executes `/new` inside a bound project directory
- **THEN** the session starts with clean Root context
- **AND** the bound project's `ledger.json` is neither loaded into prompt context nor overwritten

#### Scenario: Foreign checkout does not auto-bind by Git remote
- **WHEN** a user starts `alan` in a separate checkout or clone sharing a Git remote with an already bound project
- **THEN** Alan does not implicitly bind or load the first project's Ledger or summaries without explicit user binding

### Requirement: Foreground exit checkpoints active workstreams without spawning a background daemon
Actual exit of a foreground `alan` process SHALL quiesce active in-instance workstreams, flush durable checkpoints, and shut down without launching a background replacement daemon. A terminal-host view detach that retains the native `alan` process MAY keep in-instance workstreams running while the foreground process remains alive.

#### Scenario: Normal foreground exit flushes checkpoints and stops children
- **WHEN** the user exits a foreground `alan` instance while workstreams are active or idle
- **THEN** Alan persists durable checkpoints for active `persistent` workstreams and stops all child Agent Processes
- **AND** no background Alan daemon continues running after process exit

#### Scenario: Terminal-host detach retains live foreground instance
- **WHEN** a terminal multiplexer such as tmux detaches a view while keeping the native `alan` process alive
- **THEN** active in-instance workstreams continue running under that foreground instance without spawning a separate daemon

### Requirement: Scheduled workflows support project inbox digests and guarded Stateful Cron Streams
Root SHALL draft scheduled `WorkflowManifest` definitions conversationally and install `launchd` or `crontab` triggers only after explicit confirmation. Stateless runs SHALL write redacted digests to project `inbox/`, while Stateful Cron Streams SHALL resume under a per-stream lease lock with compaction, budget caps, grant revalidation, and failure auto-pause.

#### Scenario: Conversational workflow creation requires explicit confirmation
- **WHEN** a user asks Root to create a recurring daily task
- **THEN** Root drafts the schedule, trust posture, grant scope, and budget cap for inspection
- **AND** `launchd` or `crontab` registration occurs only after explicit user confirmation

#### Scenario: Stateless scheduled workflow writes to project inbox
- **WHEN** a project-bound stateless `alan workflow` run finishes
- **THEN** it appends a bounded, secret-scrubbed digest entry to the project's `inbox/` directory in System Store

#### Scenario: Stateful Cron Stream enforces guardrails and lease lock
- **WHEN** a Stateful Cron Stream triggers on schedule
- **THEN** it acquires the per-stream lease lock, revalidates live grants and trust posture, applies compaction if over threshold, and enforces the per-run `task_budget` cap
- **AND** if consecutive failures reach `max_consecutive_failures`, it marks the stream `paused_error` and writes an alert entry to `inbox/`

#### Scenario: Stateful Cron Stream defers on interactive lock contention
- **WHEN** a Stateful Cron Stream triggers while an interactive `alan` session holds the target stream's lease lock
- **THEN** the scheduled run exits cleanly without mutating the checkpoint and records a contention notice in `inbox/`
