## ADDED Requirements

### Requirement: Terminal Runtime Reports Truthful Grid Dimensions
The macOS terminal runtime SHALL treat terminal rows and columns as terminal
grid dimensions and SHALL keep AppKit/SwiftUI point sizes separate from
terminal grid dimensions in runtime metadata, transcript snapshots, and shell
state projection.

#### Scenario: Transcript snapshot uses terminal grid
- **WHEN** a live terminal ContentInstance produces a transcript snapshot
- **THEN** the snapshot dimensions report terminal rows and columns from the
  current terminal grid
- **AND** the snapshot does not report host-view point width or height as
  terminal rows or columns

#### Scenario: Runtime resize mirrors Ghostty grid to PTY
- **WHEN** a terminal ContentInstance host view changes size
- **THEN** Alan gives Ghostty the full assigned host surface size
- **AND** Ghostty derives and reports the renderer terminal grid
- **AND** Alan applies that Ghostty renderer grid to the Alan-owned PTY handle
- **AND** Alan does not shrink the host view or parent PaneSlot frame solely to
  make the frame grid-perfect

#### Scenario: Terminal surface absorbs sub-cell remainder
- **WHEN** the assigned host frame is not an exact multiple of the measured
  terminal cell size
- **THEN** the terminal host still visually fills the full assigned pane frame
- **AND** any leftover point space smaller than a full terminal cell is rendered
  as terminal-owned background or stable terminal-local padding
- **AND** the runtime does not publish that leftover point space as additional
  terminal rows or columns

#### Scenario: Renderer and PTY grid mismatch is diagnostic
- **WHEN** the renderer grid and PTY grid do not match after a resize settles
- **THEN** the runtime records a bounded diagnostic containing the
  renderer-derived planned grid, renderer grid, PTY grid, canvas points, cell
  metrics, and terminal ContentInstance identity
- **AND** Alan does not publish a point-derived value as a replacement terminal
  grid

### Requirement: Terminal Runtime Exposes Renderer And PTY Grid State
The macOS terminal runtime SHALL expose renderer and PTY grid state for each
live terminal ContentInstance so runtime resize behavior can be inspected
without relying on visual screenshots alone.

#### Scenario: Grid state is requested for live terminal content
- **WHEN** a debug or control-plane path requests terminal runtime state for a
  live terminal ContentInstance
- **THEN** the response includes renderer-derived planned rows and columns,
  renderer rows and columns when known, PTY rows and columns when known, canvas
  point size, cell point size, layout policy, and mismatch status

#### Scenario: Grid state is unavailable
- **WHEN** cell metrics, renderer grid, or PTY grid are not yet available
- **THEN** the runtime reports the missing field explicitly
- **AND** the runtime does not infer a terminal grid from point dimensions alone
