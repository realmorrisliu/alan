## ADDED Requirements

### Requirement: Terminal runtime service captures bounded transcript snapshots
The terminal runtime service SHALL expose a service-owned snapshot capture path
for live terminal ContentInstances that returns bounded terminal transcript
state without exposing durable manifests to Ghostty renderer internals.

#### Scenario: Live terminal snapshot is requested
- **WHEN** the shell close guard or workspace manifest sync requests a snapshot for a live terminal ContentInstance
- **THEN** the runtime service returns a bounded transcript snapshot containing restorable text history, dimensions, viewport, cwd, title, process summary, and capture metadata when available
- **AND** the snapshot is keyed by terminal ContentInstance identity

#### Scenario: Surface extraction is unavailable
- **WHEN** a live Ghostty surface cannot provide a text or scrollback extraction range
- **THEN** the runtime service may use a bounded transcript ring buffer maintained by the terminal handle
- **AND** the absence of high-fidelity renderer state does not cause Alan to persist Ghostty renderer objects

#### Scenario: Snapshot excludes non-restorable runtime objects
- **WHEN** a terminal transcript snapshot is produced
- **THEN** it does not include PTY file descriptors, child process handles, Ghostty surface pointers, renderer objects, delivery queues, or unbounded scrollback

### Requirement: Terminal runtime service supports bounded graceful shutdown requests
The terminal runtime service SHALL expose a service-owned graceful shutdown
request path for live terminal ContentInstances so confirmed close can give
foreground work a chance to exit and print final transcript output before forced
runtime finalization.

#### Scenario: Graceful shutdown is requested for active terminal work
- **WHEN** the shell host has received user confirmation for closing active terminal work
- **THEN** the runtime service requests a graceful shutdown for the corresponding terminal ContentInstance without exposing PTY handles or Ghostty surface pointers to the shell host
- **AND** the shell host can observe whether the terminal returned to inactive foreground-work state or exited before the bounded wait expires

#### Scenario: Graceful shutdown cannot be requested
- **WHEN** the runtime surface is missing, failed, exited, or unable to receive the graceful shutdown request
- **THEN** the runtime service returns an explicit request result
- **AND** confirmed close may continue to capture the latest available transcript and force-finalize the runtime

#### Scenario: Process-group signal delivery is unavailable
- **WHEN** the current Ghostty-backed runtime does not expose a foreground process-group signal seam
- **THEN** Alan treats graceful shutdown as best-effort terminal-level shutdown input and bounded observation
- **AND** Alan does not claim true PTY/process survival or guaranteed `SIGTERM` delivery

### Requirement: Restored transcript history seeds newly created terminal runtimes
The terminal runtime service SHALL seed newly created terminal runtimes with
restored transcript history when a terminal ContentInstance is materialized from
a workspace manifest with a terminal transcript snapshot and before treating the
pane as ready for normal user input.

#### Scenario: Runtime starts with restored transcript
- **WHEN** a restored terminal ContentInstance has a terminal transcript snapshot
- **THEN** the runtime service presents the transcript as initial terminal history for the new runtime
- **AND** the new shell starts in the restored cwd and can accept subsequent input

#### Scenario: Restored alternate-screen snapshot
- **WHEN** a transcript snapshot was captured from alternate-screen terminal mode
- **THEN** Alan records the captured mode metadata
- **AND** the restored runtime may present the captured text as transcript history without claiming that the prior alternate-screen application is still running

#### Scenario: Snapshot metadata is debug-only
- **WHEN** a runtime is seeded from a restored transcript snapshot
- **THEN** debug or control-plane metadata may indicate that the runtime was restored from a snapshot
- **AND** the normal terminal UI is not required to show additional restored-session chrome
