## MODIFIED Requirements

### Requirement: `/proc` renders the process table as files
Alan Kernel SHALL render the process table as files under `/proc`. Each process
SHALL appear as `/proc/<pid>` with files for identity, parentage, credentials,
namespace, status, exit state, its standard IO streams (`io/`), and a `ctl`
control file (the generic process layout every process exposes, so control
writes such as interrupt/cancel route through `/proc/<pid>/ctl`).

Before a committed Process becomes visible as running or accepts
`/proc/<pid>/ctl` control, Alan Kernel SHALL ask its Process runner to prepare
one per-Process terminal finalizer from the committed `ProcessInvocation`.
Preparation and finalization SHALL default to a no-op. Alan Kernel SHALL retain
the prepared finalizer and serialize runner completion, `/proc/<pid>/ctl`, and
Host `record_exit` through one per-Process terminal transition claim. Before
publishing any terminal transition, it SHALL invoke the claimed finalizer
exactly once with the winning claim source and intended numeric exit code. Only
a runner-completion winner SHALL carry its `ProcessOutcome`; control and Host
winners SHALL carry none. It SHALL await finalization
before publishing exit and, for control- or Host-driven termination, before
aborting the runner. The preparation and finalizer SHALL NOT change the exit
code or add executable-specific semantics to Alan Kernel. Claim source SHALL
remain transition-local and SHALL NOT become another lifecycle state.

#### Scenario: A process is inspected
- **WHEN** a consumer opens `/proc/<pid>`
- **THEN** it finds files describing identity, parent, credentials, namespace,
  status, exit state, `io/` streams, and a `ctl` control file subject to access
  rights
- **AND** `/proc/<pid>` is the single source of truth for that process; any
  `/agent`-style view is derived from it

#### Scenario: Control terminates a Process with a finalizer
- **WHEN** a consumer writes `cancel` or `interrupt` to `/proc/<pid>/ctl`
- **THEN** Alan Kernel claims the terminal transition and invokes the runner
  finalizer prepared before control became reachable with exit code `130`
- **AND** the runner is aborted and exit `130` is published only after that
  finalizer finishes

#### Scenario: Control arrives immediately after Process commit
- **WHEN** a consumer opens `/proc/<pid>/ctl` as soon as the Process becomes
  visible
- **THEN** its per-Process finalizer has already been prepared
- **AND** control cannot race ahead of runner-owned terminal registration

#### Scenario: Completion races with control
- **WHEN** runner completion and Process control race to terminate one Process
- **THEN** exactly one path claims terminal finalization
- **AND** the hook runs once and `/proc/<pid>` publishes one authoritative exit
- **AND** runner outcome reaches the hook only when runner completion wins
- **AND** a control winner supplies no losing runner outcome

#### Scenario: Process image needs no terminal finalization
- **WHEN** a Process runner uses the default terminal preparation and hook
- **THEN** both are no-ops
- **AND** generic Process completion retains its existing lifecycle behavior
