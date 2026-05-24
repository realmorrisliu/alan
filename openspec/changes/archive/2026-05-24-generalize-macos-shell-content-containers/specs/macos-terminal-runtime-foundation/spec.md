## MODIFIED Requirements

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
