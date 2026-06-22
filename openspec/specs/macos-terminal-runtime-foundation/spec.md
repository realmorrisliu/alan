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

### Requirement: Managed User PTY Provider Depends On Alan-Owned Runtime
Helper-backed Managed User terminal launch SHALL use Alan-owned PTY runtime
handles and MUST NOT launch managed-user terminals through renderer-owned
process state or `sudo` command fallback.

#### Scenario: Alan-owned PTY runtime is unavailable
- **WHEN** a `managed_user` Terminal Profile is selected before the Alan-owned
  PTY runtime and Ghostty external-PTY attachment are available
- **THEN** alan reports the managed-user terminal launch path as unavailable
- **AND** alan does not fall back to `sudo_user`, `osascript`, or a raw custom
  command to enter the Managed User

#### Scenario: Helper provides PTY endpoint
- **WHEN** Alan creates a terminal ContentInstance for a ready `managed_user`
  profile
- **THEN** the terminal runtime requests a helper-owned PTY endpoint for the
  account
- **AND** the resulting runtime handle remains keyed by terminal
  ContentInstance identity
- **AND** Ghostty attaches as renderer/protocol adapter over the Alan-provided
  endpoint

### Requirement: Managed User PTY Control Routes Through Helper Sessions
For helper-owned Managed User PTY sessions, the terminal runtime service SHALL
route resize, text delivery, EOF, interrupt, terminate, kill, and exit
observation through the Alan runtime handle and helper session boundary.

#### Scenario: Managed user terminal is resized
- **WHEN** a helper-backed Managed User terminal ContentInstance size changes
- **THEN** Alan applies the PTY window size through the helper session
- **AND** renderer resize follows the same dimensions without becoming the
  source of process truth

#### Scenario: Managed user terminal receives input
- **WHEN** terminal input is delivered to a helper-backed Managed User terminal
- **THEN** Alan writes the input through the Alan-owned PTY runtime handle
- **AND** the helper session remains responsible for the privileged PTY child
  and process-group lifecycle

#### Scenario: Managed user child exits
- **WHEN** the helper-owned child process exits
- **THEN** the helper reports exit status to Alan
- **AND** the terminal runtime projects final lifecycle metadata to the terminal
  ContentInstance

### Requirement: Terminal render coordination is window scoped
Each macOS shell window SHALL own a terminal render coordinator that coalesces
embedded Ghostty wakeups for terminal ContentInstance handles in that window and
drains render work by terminal runtime priority.

#### Scenario: Multiple terminal surfaces wake at once
- **WHEN** several terminal ContentInstances in one shell window request Ghostty
  tick or refresh work during the same scheduling interval
- **THEN** the window render coordinator coalesces those requests into bounded
  main-actor drain work
- **AND** foreground interactive surfaces are processed before visible
  background and hidden background surfaces

#### Scenario: Hidden surface requests repeated refreshes
- **WHEN** a hidden background terminal ContentInstance repeatedly requests
  refresh work because of high output
- **THEN** the coordinator avoids scheduling one immediate surface paint per
  wakeup
- **AND** the coordinator retains enough pending state to catch up when that
  terminal becomes visible

#### Scenario: Window closes with pending render work
- **WHEN** a shell window closes while the render coordinator has pending
  terminal wakeups
- **THEN** the coordinator cancels or drains only work that is safe for closing
  terminal ContentInstance handles
- **AND** no pending wakeup resurrects a closed surface handle

### Requirement: Ghostty app tick and surface refresh are separate scheduling concerns
Alan SHALL distinguish embedded Ghostty app tick processing from terminal
surface refresh painting so that required state and lifecycle events can be
processed without forcing hidden surfaces to paint on every wakeup.

#### Scenario: Hidden terminal has lifecycle events
- **WHEN** a hidden terminal ContentInstance has pending child-exit, close,
  error, title, cwd, or attention events
- **THEN** Alan drains the required Ghostty or runtime state needed to publish
  truthful lifecycle metadata
- **AND** Alan does not treat that drain as permission to repaint the hidden
  surface on every output wakeup

#### Scenario: Visible terminal needs repaint
- **WHEN** a visible terminal ContentInstance has pending rendered output
- **THEN** Alan schedules surface refresh according to foreground or visible
  background priority
- **AND** the refresh path uses the existing terminal ContentInstance surface
  handle

#### Scenario: Hidden terminal is promoted to foreground
- **WHEN** a hidden background terminal ContentInstance becomes foreground
  interactive
- **THEN** Alan performs catch-up tick processing and schedules a surface
  refresh for that same ContentInstance before treating the terminal as current
  for user interaction

### Requirement: Runtime update publication is priority aware
The terminal runtime service SHALL retain the latest runtime state for every
terminal ContentInstance while publishing SwiftUI-facing updates at a cadence
appropriate to foreground interactive, visible background, and hidden background
priority.

#### Scenario: Foreground terminal metadata changes
- **WHEN** a foreground interactive terminal ContentInstance reports scrollback,
  renderer phase, title, cwd, process, input readiness, or attention changes
- **THEN** Alan publishes the update immediately enough for active terminal
  interaction and visible controls to remain current

#### Scenario: Visible background terminal metadata changes
- **WHEN** a visible background terminal ContentInstance reports runtime state
  changes
- **THEN** Alan coalesces SwiftUI-facing publication to the display cadence
- **AND** the terminal runtime service retains the latest state even if several
  updates are merged into one UI publication

#### Scenario: Hidden background terminal produces high-frequency updates
- **WHEN** a hidden background terminal ContentInstance reports high-frequency
  scrollback metrics, renderer phase changes, or output-driven refresh state
- **THEN** Alan retains the latest runtime state without continuously
  invalidating the shell root view
- **AND** sidebar-relevant summaries such as title, cwd, child exit, bell,
  attention, and failure remain publishable on a bounded slower path

### Requirement: Hidden terminal surfaces are unfocused and occluded for Ghostty
Alan SHALL propagate terminal focus and visibility priority to embedded Ghostty
surfaces so hidden background terminals are treated as unfocused and occluded
for rendering while remaining live for terminal state and IO.

#### Scenario: Selected pane changes
- **WHEN** terminal focus moves from one visible terminal pane to another
- **THEN** Alan marks the newly focused terminal foreground interactive
- **AND** Alan marks the previously focused terminal visible background or
  hidden background according to its actual visibility

#### Scenario: Tab is no longer visible
- **WHEN** a terminal ContentInstance belongs to a tab that is no longer visible
- **THEN** Alan marks the embedded Ghostty surface unfocused and occluded for
  rendering coordination
- **AND** the terminal runtime handle remains live in the window runtime service

#### Scenario: Hidden terminal becomes visible
- **WHEN** a hidden terminal ContentInstance becomes visible again
- **THEN** Alan updates Ghostty focus and occlusion state before the terminal is
  treated as ready for foreground interaction
