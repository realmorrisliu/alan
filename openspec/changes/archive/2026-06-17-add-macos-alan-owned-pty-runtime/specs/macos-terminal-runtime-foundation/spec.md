## ADDED Requirements

### Requirement: Terminal PTYs are Alan-owned
The macOS terminal runtime SHALL allocate and own the PTY, child process,
process group metadata, lifecycle phase, delivery state, and exit observation
through Alan terminal runtime services for each terminal ContentInstance.

This requirement is scoped to the macOS Apple client runtime. It does not
require Linux, Windows, or cross-platform PTY ownership behavior.

#### Scenario: Terminal content starts with Alan-owned PTY
- **WHEN** a terminal ContentInstance is created
- **THEN** Alan allocates the PTY, launches the terminal child process, records the process group, and creates the runtime handle before attaching a renderer
- **AND** the runtime handle is keyed by terminal ContentInstance identity

#### Scenario: Runtime starts without renderer attachment
- **WHEN** Alan creates an Alan-owned PTY runtime before a visible terminal host view is mounted
- **THEN** the child process and PTY lifecycle remain owned by the runtime service
- **AND** renderer attachment can occur later without starting a second child process

#### Scenario: Runtime is verified without renderer attachment
- **WHEN** focused tests exercise Alan-owned PTY launch, input, resize, EOF,
  signals, exit observation, or snapshot capture through fake or non-UI runtime
  handles
- **THEN** the runtime service reports behavior from Alan-owned PTY/process
  handles
- **AND** the test does not require a live Ghostty renderer to prove process
  ownership semantics

### Requirement: Ghostty attaches to Alan-provided PTYs
The Ghostty integration SHALL act as a renderer and terminal-protocol adapter
over PTY endpoints supplied by Alan, and MUST NOT be the authoritative owner of
terminal child-process lifecycle.

#### Scenario: Renderer attaches to live PTY
- **WHEN** a terminal host view mounts for terminal content with an Alan-owned PTY runtime
- **THEN** the Ghostty adapter attaches to the PTY endpoint supplied by Alan
- **AND** renderer readiness is reported separately from PTY and child-process readiness

#### Scenario: Renderer detaches
- **WHEN** SwiftUI or AppKit removes the visible terminal host view
- **THEN** the Ghostty renderer attachment may detach
- **AND** the Alan-owned PTY runtime, child process, process group, and delivery state remain alive unless the terminal content is closing

#### Scenario: Renderer attachment seam is unavailable
- **WHEN** the prepared Ghostty artifacts do not expose the external-PTY
  attachment seam required by Alan
- **THEN** Alan-owned PTY renderer integration checks fail with an explicit
  unsupported-seam result
- **AND** Alan does not silently fall back to Ghostty-owned child-process launch

### Requirement: Runtime control uses Alan process handles
For Alan-owned PTY runtimes, the terminal runtime service SHALL implement
resize, text delivery, EOF, interrupt, terminate, kill, and exit-status
observation using Alan-owned PTY and process handles rather than renderer-owned
process state.

#### Scenario: Terminal is resized
- **WHEN** a terminal ContentInstance size changes
- **THEN** Alan applies the PTY window size to the Alan-owned PTY handle
- **AND** renderer resize follows the same dimensions without becoming the source of process truth

#### Scenario: Interrupt is requested
- **WHEN** the shell host or control plane requests an interrupt for active terminal work
- **THEN** Alan sends the configured interrupt signal to the terminal foreground process group when available
- **AND** the result reports whether signal delivery was attempted, unavailable, or failed

#### Scenario: Child process exits
- **WHEN** the Alan-owned child process exits
- **THEN** the runtime service records exit status, closes or drains PTY state according to policy, and projects final terminal metadata to shell state

### Requirement: Alan-owned PTY snapshots remain bounded
The Alan-owned PTY runtime SHALL maintain bounded transcript and activity state
needed for restore, close guards, diagnostics, and agent-readable terminal
metadata without persisting raw PTY handles, child-process handles, renderer
objects, or unbounded scrollback.

#### Scenario: Snapshot is captured
- **WHEN** workspace persistence requests a terminal snapshot from an Alan-owned PTY runtime
- **THEN** the runtime returns bounded transcript, dimensions, cwd, title, process summary, and capture metadata when available
- **AND** the snapshot excludes live file descriptors and process handles

#### Scenario: Renderer transcript is unavailable
- **WHEN** Ghostty renderer extraction is unavailable for an Alan-owned PTY runtime
- **THEN** Alan may use its bounded PTY transcript ring buffer for snapshot capture
- **AND** the snapshot truthfully records the capture source and fidelity

### Requirement: Alan-owned PTY runtime replaces Ghostty process ownership
The terminal runtime service SHALL replace the current Ghostty-owned terminal
process boundary with Alan-owned PTY runtime handles before the implementation
branch is merged to `main`.

#### Scenario: Implementation branch is ready to merge
- **WHEN** the Alan-owned PTY runtime implementation is marked ready for review
- **THEN** terminal launch, delivery, resize, signal, exit, snapshot, and renderer attachment go through Alan-owned runtime handles
- **AND** terminal ContentInstances do not rely on a selectable fallback process owner

#### Scenario: Obsolete process owner is removed
- **WHEN** the feature branch removes renderer-owned process lifecycle code
- **THEN** Ghostty remains responsible for renderer and terminal-protocol behavior
- **AND** Alan remains responsible for PTY and child-process lifecycle

## MODIFIED Requirements

### Requirement: Terminal runtime service supports bounded graceful shutdown requests
The terminal runtime service SHALL expose a service-owned graceful shutdown
request path backed by Alan-owned PTY and process handles for live terminal
ContentInstances so confirmed close can give foreground work a chance to exit
and print final transcript output before forced runtime finalization.

#### Scenario: Graceful shutdown is requested for active terminal work
- **WHEN** the shell host has received user confirmation for closing active
  terminal work
- **THEN** the runtime service requests graceful shutdown for the corresponding
  terminal ContentInstance through Alan-owned PTY/process runtime handles without
  exposing PTY handles, process handles, or Ghostty surface pointers to the shell
  host
- **AND** the shell host can observe whether the terminal returned to inactive
  foreground-work state or exited before the bounded wait expires

#### Scenario: Graceful shutdown cannot be requested
- **WHEN** the Alan-owned runtime is missing, failed, exited, or unable to
  receive the graceful shutdown request
- **THEN** the runtime service returns an explicit request result
- **AND** confirmed close may continue to capture the latest available transcript
  and force-finalize the runtime

#### Scenario: Alan-owned process signal delivery is available
- **WHEN** active terminal work has an Alan-owned foreground process group
  eligible for graceful signal delivery
- **THEN** the runtime service delivers the configured graceful termination
  signal through Alan-owned process handles according to policy
- **AND** the result records the attempted delivery without using Ghostty-owned
  process state

#### Scenario: Alan-owned process signal delivery is unavailable
- **WHEN** the Alan-owned runtime cannot identify an eligible foreground process
  group or the platform refuses signal delivery
- **THEN** the runtime service returns an explicit unavailable or failed result
  and may continue bounded observation before forced finalization
- **AND** Alan does not treat Ghostty terminal-level input or renderer-owned
  process state as a fallback process owner
