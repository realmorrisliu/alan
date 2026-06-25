## ADDED Requirements

### Requirement: Renderer hosts do not own Alan Kernel runtime
Alan renderer hosts SHALL render app/service semantic snapshots grounded in Alan
Kernel files, processes, and stream files, and translate host input. They SHALL NOT
own operation descriptor registries, task supervision, projection authority, or
service stream-file authority.

#### Scenario: Ratatui host is inspected
- **WHEN** the Ratatui renderer host is implemented
- **THEN** it owns terminal frame rendering, terminal input capture, layout, and
  renderer caches
- **AND** it obtains semantic state through app/service snapshots and operation
  surfaces grounded in Alan Kernel files, processes, and stream files rather than a
  Ratatui-private application model

#### Scenario: SwiftUI host is added later
- **WHEN** a SwiftUI host renders the same app/service semantic view
- **THEN** it can consume the same app/service semantic snapshot and operation
  surfaces without depending on Ratatui or Crossterm types

### Requirement: Hosts consume semantic snapshots
Renderer hosts SHALL pull semantic view snapshots after invalidation or update
signals instead of consuming renderer-specific render patches from the core.

#### Scenario: View changes
- **WHEN** a process/task, buffer, object, or operation-surface update invalidates a
  view
- **THEN** the host can pull a versioned semantic view snapshot
- **AND** the host chooses its own diffing, caching, layout, and rendering
  strategy

#### Scenario: Snapshot is rendered in different hosts
- **WHEN** a conversation, task tree, form, diff, text document, or log stream
  snapshot is rendered by multiple hosts
- **THEN** each host renders it using host-native widgets or terminal cells
  while preserving the semantic content and action identities

### Requirement: Built-in view models have host adapters
Renderer hosts SHALL provide adapters for the built-in semantic view models
needed by the first Kernel slice.

#### Scenario: Alan Agent conversation vertical slice is rendered
- **WHEN** the first Ratatui renderer slice renders Alan Agent work
- **THEN** it supports conversation, form, task tree, and command palette
  semantic view snapshots

#### Scenario: Read/review surfaces are rendered
- **WHEN** text document, diff, object list, or log stream snapshots are
  introduced in the Alan Kernel
- **THEN** renderer hosts can add adapters for those built-in models without
  changing the core snapshot contract

### Requirement: Dynamic extension views use fallback rendering
Renderer hosts SHALL render unknown or extension-provided semantic views through
schema-versioned fallback behavior unless a host-specific renderer capability is
explicitly added later.

#### Scenario: Extension view is unknown to host
- **WHEN** a renderer host receives an extension view snapshot with a schema id
  it does not recognize
- **THEN** it presents a bounded fallback representation or unsupported-view
  diagnostic
- **AND** it does not execute extension renderer code by default

#### Scenario: Future renderer extension is considered
- **WHEN** a future change adds host-specific renderer extension support
- **THEN** it defines explicit renderer capabilities and permission boundaries
  separate from the semantic Alan Kernel contract

### Requirement: Host input becomes semantic input or command invocation
Renderer hosts SHALL translate raw host events into semantic input intents,
view-local input, or command invocations before crossing the runtime boundary.

#### Scenario: Terminal key is pressed
- **WHEN** Crossterm reports a key, paste, resize, or mouse event to the Ratatui
  host
- **THEN** the host translates it into a semantic input intent, view-local
  input, command invocation, or host-local layout change
- **AND** raw Crossterm event types do not become Alan Kernel state

#### Scenario: Native control is activated
- **WHEN** a SwiftUI button, menu item, keyboard shortcut, or responder action
  is activated in a future host
- **THEN** the host invokes the same command identity or semantic input intent
  that other hosts use for the equivalent action

### Requirement: View-local state is separated from host render state
Renderer hosts SHALL keep renderer-only cache and layout state separate from
app/service semantic view state that references Alan Kernel files, processes,
stream files, credentials, descriptors, access rights, and produced files.

#### Scenario: Selection matters semantically
- **WHEN** selection, focused field, filter text, scroll anchor, expanded
  semantic node, or active view mode is needed for restore, agent inspection,
  command routing, or another renderer
- **THEN** it is stored as app/service semantic view state grounded in Kernel
  anchors

#### Scenario: Layout cache is renderer-specific
- **WHEN** measured line wraps, terminal cell cache, pixel geometry, hover
  state, animation frame, or renderer-specific scroll detail is needed
- **THEN** it remains host render state and is not written as semantic
  app/service state

### Requirement: Host layout remains separate from semantic app state
Renderer hosts SHALL own physical layout while apps/services own semantic open
buffers, views, active view, focus relationships, and task projections.

#### Scenario: Host splits a pane
- **WHEN** a host splits, resizes, moves, or collapses panes, tabs, windows, or
  overlays
- **THEN** the physical layout change remains host-local unless it changes which
  semantic view is open or active

#### Scenario: Alan Kernel opens a view
- **WHEN** a command opens or focuses a semantic view
- **THEN** the host decides where and how to mount that view
- **AND** the Alan Kernel records the semantic open/focus state independent of the
  host's physical placement

### Requirement: Ratatui integration preserves terminal behavior
The Ratatui renderer host SHALL preserve the current compatibility Alan Shell
behavior while semantic rendering is introduced.

#### Scenario: Semantic path is introduced
- **WHEN** the Ratatui TUI starts consuming Alan Kernel semantic snapshots
- **THEN** compatibility session creation, hydration, reconnect replay, submissions,
  resume operations, interrupts, compaction, rollback, frame coalescing, and
  terminal scrollback behavior remain compatible with the accepted Rust inline
  TUI contract

#### Scenario: Semantic renderer fails during migration
- **WHEN** a semantic renderer path is incomplete or disabled during the first
  integration phase
- **THEN** the implementation can continue to use the existing TUI path for
  unsupported surfaces until focused tests prove semantic parity
