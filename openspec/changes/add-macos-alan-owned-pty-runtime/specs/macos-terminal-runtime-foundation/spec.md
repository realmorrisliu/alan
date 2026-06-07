## ADDED Requirements

### Requirement: Terminal PTYs are Alan-owned
The macOS terminal runtime SHALL allocate and own the PTY, child process,
process group metadata, lifecycle phase, delivery state, and exit observation
for each terminal ContentInstance through Alan terminal runtime services.

#### Scenario: Terminal content starts with Alan-owned PTY
- **WHEN** a terminal ContentInstance is created through the Alan-owned runtime path
- **THEN** Alan allocates the PTY, launches the terminal child process, records the process group, and creates the runtime handle before attaching a renderer
- **AND** the runtime handle is keyed by terminal ContentInstance identity

#### Scenario: Runtime starts without renderer attachment
- **WHEN** Alan creates an Alan-owned PTY runtime before a visible terminal host view is mounted
- **THEN** the child process and PTY lifecycle remain owned by the runtime service
- **AND** renderer attachment can occur later without starting a second child process

### Requirement: Ghostty attaches to Alan-provided PTYs
The Ghostty integration SHALL act as a renderer and terminal-protocol adapter
over PTY endpoints supplied by Alan, and MUST NOT be the authoritative owner of
terminal child-process lifecycle in the Alan-owned PTY runtime path.

#### Scenario: Renderer attaches to live PTY
- **WHEN** a terminal host view mounts for terminal content with an Alan-owned PTY runtime
- **THEN** the Ghostty adapter attaches to the PTY endpoint supplied by Alan
- **AND** renderer readiness is reported separately from PTY and child-process readiness

#### Scenario: Renderer detaches
- **WHEN** SwiftUI or AppKit removes the visible terminal host view
- **THEN** the Ghostty renderer attachment may detach
- **AND** the Alan-owned PTY runtime, child process, process group, and delivery state remain alive unless the terminal content is closing

### Requirement: Runtime control uses Alan process handles
The terminal runtime service SHALL implement resize, text delivery, EOF,
interrupt, terminate, kill, and exit-status observation using Alan-owned PTY and
process handles rather than renderer-owned process state.

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

### Requirement: Legacy Ghostty-owned runtime can coexist during migration
The terminal runtime service SHALL allow the existing Ghostty-owned process path
and the Alan-owned PTY path to coexist behind an explicit runtime selection
boundary until Alan-owned PTY parity is proven.

#### Scenario: Existing runtime path is selected
- **WHEN** runtime selection chooses the legacy Ghostty-backed path
- **THEN** current terminal behavior continues to follow the existing Ghostty runtime service contracts

#### Scenario: Alan-owned path is selected
- **WHEN** runtime selection chooses the Alan-owned PTY path
- **THEN** terminal launch, delivery, resize, signal, exit, snapshot, and renderer attachment go through the Alan-owned runtime handles
- **AND** control-plane responses identify the selected runtime path in debug metadata
