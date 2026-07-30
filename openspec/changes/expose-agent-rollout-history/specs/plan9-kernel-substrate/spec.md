## MODIFIED Requirements

### Requirement: `/proc` renders the process table as files
Alan Kernel SHALL render the process table as files under `/proc`. Each process
SHALL appear as `/proc/<pid>` with files for identity, parentage, credentials,
namespace, status, exit state, its standard IO streams (`io/`), and a `ctl`
control file (the generic process layout every process exposes, so control
writes such as interrupt/cancel route through `/proc/<pid>/ctl`).

Before publishing any terminal Process transition, Alan Kernel SHALL invoke
the committed Process runner's terminal finalization hook exactly once with
the `ProcessInvocation` and intended numeric exit code. The hook SHALL default
to a no-op. Alan Kernel SHALL serialize runner completion, `/proc/<pid>/ctl`,
and Host `record_exit` through one per-Process terminal transition claim. It
SHALL await the claimed hook before publishing exit and, for control- or
Host-driven termination, before aborting the runner. The hook SHALL NOT change
the exit code, create another lifecycle state, or add executable-specific
semantics to Alan Kernel.

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
  finalizer with exit code `130`
- **AND** the runner is aborted and exit `130` is published only after that
  finalizer finishes

#### Scenario: Completion races with control
- **WHEN** runner completion and Process control race to terminate one Process
- **THEN** exactly one path claims terminal finalization
- **AND** the hook runs once and `/proc/<pid>` publishes one authoritative exit

#### Scenario: Process image needs no terminal finalization
- **WHEN** a Process runner uses the default terminal hook
- **THEN** finalization is a no-op
- **AND** generic Process completion retains its existing lifecycle behavior
