# alan-os-host-lifecycle Specification

## Purpose
Defines foreground invocation ownership of an alan9 instance, file-proven
readiness, per-invocation boot identity, and production composition.
## Requirements
### Requirement: Each foreground invocation owns its alan9 instance
Each bare or redirected Agent-execution invocation of `alan` SHALL own an
independent alan9 instance when `ALAN_INSTANCE_RUNTIME_DIR` is unset or unique
to that invocation. It also owns its Kernel and native application lifetime.
An explicit shared runtime directory selects one exact endpoint and permits
only one owner.
In this contract, an invocation means that execution path; metadata,
configuration and explicitly targeted management subcommands SHALL NOT boot an
unrelated Root Agent.
Explicit Host bindings SHALL select the product or isolated test store pair,
not a singleton live runtime. Service Manager
SHALL retain ownership of services and the instance-local Root Agent Process.
The renderer SHALL remain a file client of that instance. Bare `alan` MUST NOT
launch or attach to a separate background Host, including through launchd or
systemd. No terminal host SHALL be required for ordinary terminal operation.

#### Scenario: Two terminal sessions start Alan
- **WHEN** two terminal sessions start `alan` for the same user without `ALAN_INSTANCE_RUNTIME_DIR`, or with distinct runtime
  directories
- **THEN** each owns an independent Root Agent, Process table, input queue, cwd,
  and runtime endpoint
- **AND** exiting one invocation does not stop or submit work to the other
- **AND** concurrent use of package, connection, and credential stores preserves
  their existing commit and authorization contracts

#### Scenario: Two invocations select the same runtime directory
- **WHEN** two terminal sessions select the same explicit
  `ALAN_INSTANCE_RUNTIME_DIR`
- **THEN** only one invocation owns that runtime directory
- **AND** the other fails to acquire it instead of attaching to or borrowing its Root Agent

#### Scenario: Terminal host retains a process
- **WHEN** a terminal host such as tmux detaches a view while retaining the Alan process
- **THEN** that instance may keep executing while its process is alive
- **AND** Alan does not spawn a background replacement to provide that lifetime
- **AND** terminal-host identifiers grant neither Alan identity nor authority

### Requirement: Host readiness is file-proven
An invocation SHALL expose its renderer or explicitly addressed local attachments
only after the Standard Namespace, required services and `/agent/root` are
readable. Required failure SHALL fail startup, clean up the failed instance and
report an error rather than connect to an unrelated running instance.

#### Scenario: Root Agent fails to start
- **WHEN** `/agent/root` cannot be read during boot
- **THEN** the invocation reports startup failure
- **AND** it does not present a partial system or borrow another terminal's Root Agent

### Requirement: Host restart creates a new boot identity
Every invocation SHALL create a fresh boot identity, Process table and Root Agent
Process. It MUST NOT deserialize live Process state or implicitly select a
previous product-wide rollout. Recovery SHALL require an explicitly selected
durable record. Recovery failure SHALL be reported without silently starting a
fresh task. Current authority SHALL be revalidated; recorded paths or grants are
not live capabilities. Reliable pending work SHALL remain paused, and unknown
effects MUST NOT be replayed automatically.

#### Scenario: A Process reference belongs to another boot
- **WHEN** a client presents a reference with a different instance boot identity
- **THEN** Alan rejects it even if the PID has been reused

#### Scenario: A new invocation starts
- **WHEN** the user starts a new invocation without explicitly selecting recovery
- **THEN** it creates fresh execution in its own instance
- **AND** it does not implicitly attach to or resume another invocation's Root

#### Scenario: User selects durable recovery
- **WHEN** the user explicitly selects valid durable execution evidence
- **THEN** it is interpreted by the existing recovery owners in a fresh instance
- **AND** reliable pending work is exposed paused with current access revalidated

#### Scenario: User selects missing or invalid recovery evidence
- **WHEN** the user explicitly selects a missing or invalid durable recovery record
- **THEN** recovery fails with a diagnostic rather than starting fresh execution

### Requirement: Product composition preserves production adapters
Mock providers and ephemeral test stores SHALL require explicit development/test
selection. Foreground product composition SHALL use the existing product adapters,
product stores and governance; running in-process SHALL NOT itself select a test
Host or weaken authorization.

#### Scenario: Product composition fails
- **WHEN** product composition fails to boot its independent foreground instance
- **THEN** startup fails with a diagnostic
- **AND** it does not substitute mock providers, test-only authority, or an
  ambient background Host

### Requirement: Foreground application exit ends its owned runtime
Actual Alan exit SHALL shut down its owned services and active execution through
the existing lifecycle and native descendant cancellation boundaries. Completed
effects SHALL remain completed. Forced termination or incomplete durable evidence
SHALL NOT be reported as successful cancellation or successful work. Terminal-host
view detach SHALL be distinguished from actual process exit.

#### Scenario: User exits Alan during work
- **WHEN** the user explicitly exits the foreground application
- **THEN** the instance stops admitting work and shuts down its owned execution
- **AND** it does not keep a detached Host running or roll back completed effects

#### Scenario: Native process is forcibly terminated
- **WHEN** Alan cannot complete orderly shutdown or persist a terminal outcome
- **THEN** later explicit recovery reports incomplete or unknown work from available evidence
- **AND** absence of completion does not authorize automatic replay

### Requirement: Ephemeral invocations leave the root rollout pointer untouched
An invocation started as ephemeral SHALL NOT read or overwrite the product root
rollout pointer and SHALL be rejected when combined with explicit recovery.

#### Scenario: An ephemeral invocation ends
- **WHEN** an ephemeral invocation runs and exits
- **THEN** the next explicit recovery selects the same durable work it would have before
- **AND** the ephemeral invocation never became the recorded root

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

