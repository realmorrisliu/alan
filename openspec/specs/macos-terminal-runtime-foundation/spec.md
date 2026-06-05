# macos-terminal-runtime-foundation Specification

## Purpose
Defines the macOS terminal runtime foundation, including process-scoped Ghostty
bootstrap, window-scoped runtime services, stable pane handles, host-view
adapter boundaries, metadata projection, and deterministic cleanup.
## Requirements
### Requirement: Ghostty initialization is process scoped
The macOS app SHALL initialize libghostty, Ghostty resources, terminfo, logging,
and global terminal configuration through a single process-scoped bootstrap
before any pane surface is created.

#### Scenario: First terminal window opens
- **WHEN** the first shell window requests a terminal runtime
- **THEN** the process bootstrap initializes Ghostty exactly once and returns a ready bootstrap state to the window runtime service

#### Scenario: Additional terminal window opens
- **WHEN** another shell window requests a terminal runtime after bootstrap has completed
- **THEN** the app reuses the existing process bootstrap instead of repeating libghostty initialization

#### Scenario: Bootstrap fails
- **WHEN** Ghostty resources, terminfo, or dynamic libraries cannot be prepared
- **THEN** the bootstrap records a stable failure state and pane creation reports non-ready runtime status without pretending terminal input succeeded

### Requirement: Runtime services are window scoped
Each macOS shell window SHALL own a terminal runtime service that maps stable
terminal ContentInstance IDs to terminal surface handles for that window only.
PaneSlot IDs MAY be accepted as convenience targets, but runtime lookup SHALL
resolve them to mounted terminal ContentInstances before touching terminal state.

#### Scenario: Terminal content lookup in one window
- **WHEN** a control-plane command targets a terminal ContentInstance in a shell window
- **THEN** the command resolves that terminal content through the terminal runtime service for the same window
- **AND** the runtime service uses `content_id` as the terminal runtime identity

#### Scenario: PaneSlot convenience target resolves to terminal content
- **WHEN** `terminal.send_text` targets a PaneSlot that mounts terminal content
- **THEN** alan resolves the PaneSlot to the mounted terminal ContentInstance before invoking the runtime service
- **AND** the terminal runtime service does not key delivery by PaneSlot identity

#### Scenario: Content ID collision across windows
- **WHEN** two windows contain terminal ContentInstances with identical local IDs or restored IDs
- **THEN** each window runtime service resolves and mutates only its own terminal content handle

### Requirement: Pane surfaces have stable handles
A terminal ContentInstance SHALL be represented by a stable surface handle that
outlives SwiftUI/AppKit view creation and stores lifecycle phase, text delivery
state, metadata, and teardown state.

#### Scenario: View is recreated
- **WHEN** SwiftUI recreates the terminal host view for an existing terminal ContentInstance
- **THEN** the new view attaches to the existing surface handle without starting a new shell process

#### Scenario: Background terminal content receives text
- **WHEN** a background terminal ContentInstance has a live surface handle and receives `terminal.send_text`
- **THEN** the runtime service delivers text through that handle without requiring the PaneSlot or tab to become visible

#### Scenario: Surface handle is closing
- **WHEN** text delivery targets terminal content whose surface handle is closing or closed
- **THEN** the runtime service rejects or queues the command according to the terminal content's delivery policy and reports that state explicitly

### Requirement: Host views are runtime adapters
`AlanTerminalHostNSView` and related SwiftUI wrappers SHALL act as adapters for
focus, display metrics, occlusion, frame changes, and input forwarding, and MUST
NOT own Ghostty app lifetime or terminal ContentInstance runtime truth.

#### Scenario: Host view attaches
- **WHEN** a terminal host view is mounted for a terminal ContentInstance
- **THEN** it receives an existing surface handle from the runtime service and reports view metrics to that handle

#### Scenario: Host view detaches
- **WHEN** a terminal host view is removed because selection or layout changed
- **THEN** the terminal content surface handle remains alive unless the content, PaneSlot, tab, window, or app is closing

### Requirement: Runtime metadata is projected by pane identity
The runtime service SHALL project terminal title, cwd, process status,
attention, renderer phase, readiness, and delivery diagnostics into alan shell
state using stable terminal ContentInstance IDs; PaneSlot projection SHALL be
derived from the content currently mounted in that slot.

#### Scenario: Metadata event from background terminal content
- **WHEN** background terminal content emits a title, cwd, process, attention, or renderer-state event
- **THEN** shell state updates the matching ContentInstance record without changing user focus
- **AND** any PaneSlot currently mounting that content reflects the updated terminal projection

#### Scenario: Metadata event after content close
- **WHEN** a terminal callback arrives after its ContentInstance has reached closed state
- **THEN** the runtime service ignores or records it as late diagnostics without resurrecting the content or its former PaneSlot

### Requirement: Runtime cleanup is deterministic
Content, PaneSlot, tab, window, and app close paths SHALL transition terminal
ContentInstance surface handles through closing and closed states and release
Ghostty resources exactly once.

#### Scenario: Closing one terminal pane
- **WHEN** a user closes a split PaneSlot that mounts terminal content
- **THEN** the runtime service tears down that terminal ContentInstance surface exactly once and preserves other terminal content handles in the same tab

#### Scenario: Closing a tab
- **WHEN** a user closes a tab with multiple terminal ContentInstances
- **THEN** the runtime service tears down every terminal ContentInstance surface in that tab exactly once and publishes final closed state

#### Scenario: App terminates
- **WHEN** the app terminates while terminal ContentInstances are live
- **THEN** the runtime service performs best-effort teardown and records closed or interrupted terminal state for persisted diagnostics

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
