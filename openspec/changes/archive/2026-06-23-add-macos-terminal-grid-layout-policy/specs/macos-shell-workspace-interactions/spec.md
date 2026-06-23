## ADDED Requirements

### Requirement: Terminal Split Layout Remains Point-Native By Default
The macOS shell SHALL keep default split pane rendering point-native. Stored
split ratios remain the durable layout model, and rendered PaneSlots SHALL fill
their assigned SwiftUI/AppKit point frames. Terminal panes SHALL report the
integer terminal grid Ghostty renders inside those frames without shrinking the
default pane frame to satisfy terminal column or row divisibility.

#### Scenario: Default terminal split fills the shell frame
- **WHEN** a terminal tab has a horizontal or vertical split
- **THEN** each PaneSlot receives the point frame derived from the split tree and
  stored ratio
- **AND** Alan does not reduce either pane frame solely because the frame does
  not divide evenly into terminal rows or columns
- **AND** any sub-cell remainder is absorbed inside terminal rendering or
  diagnostics rather than exposed as an unusable shell-layout gap

#### Scenario: Max-fit odd root grid is diagnostic by default
- **WHEN** a terminal tab has an equal horizontal split and the root terminal
  grid has `145` usable columns under `max-fit`
- **THEN** Alan keeps the default split point frames intact
- **AND** terminal diagnostics may report the odd renderer grid
- **AND** Alan does not discard a usable column solely to make the default root
  terminal grid even

#### Scenario: Ratio resize preserves runtime identity
- **WHEN** the user drags a split divider in a terminal tab
- **THEN** Alan updates the stored split ratio within normal split bounds
- **AND** terminal diagnostics refresh from Ghostty inside the new point frames without
  restarting terminal ContentInstance runtimes

#### Scenario: Nested split point frames stay stable
- **WHEN** a nested split tree produces sub-cell terminal remainder inside one
  or more terminal panes
- **THEN** Alan leaves the nested split point frames unchanged
- **AND** repeated diagnostic passes with unchanged Ghostty renderer grids do
  not change default point frames

### Requirement: Terminal Workspace Reports Runtime Grid Policy
The macOS shell SHALL use `max-fit` terminal runtime sizing before publishing
terminal grid diagnostics.

#### Scenario: Full visible-frame window uses max-fit
- **WHEN** a terminal workspace fills the visible screen frame or a native
  fullscreen-equivalent content area
- **THEN** Alan reports `max-fit` diagnostics that preserve the maximum terminal
  rows and columns that fit the available terminal canvas
- **AND** Alan does not discard a usable column solely to make the root terminal
  grid even

#### Scenario: Sidebar width changes trigger one grid recalculation
- **WHEN** the sidebar expands, collapses, or changes width while terminal
  content is visible
- **THEN** Alan lets Ghostty resize from the full assigned terminal canvas and
  refreshes `max-fit` renderer grid diagnostics
- **AND** split ratios, PaneSlot identities, and terminal ContentInstance
  identities remain stable

### Requirement: Terminal Layout Adapts Across Display Classes
The macOS shell SHALL pass measured terminal host canvas sizes to Ghostty and
report Ghostty renderer grids rather than using hard-coded monitor sizes or a
single developer-machine baseline.

#### Scenario: Constrained display does not publish point dimensions as grid
- **WHEN** the available terminal canvas is narrow or short relative to the
  requested split layout
- **THEN** Alan keeps the point-native split layout
- **AND** Alan reports missing or small renderer grids explicitly without
  publishing point dimensions as terminal columns

#### Scenario: Large external display preserves capacity
- **WHEN** the available terminal canvas is substantially larger than the
  current built-in display baseline
- **THEN** Alan uses the same Ghostty-owned `max-fit` runtime policy to report
  usable terminal rows and columns
- **AND** PTY resize follows the Ghostty renderer grid

#### Scenario: Ultrawide display supports nested splits
- **WHEN** a terminal workspace uses nested splits on an ultrawide or otherwise
  very wide canvas
- **THEN** Alan reports each mounted terminal's Ghostty renderer grid without
  publishing point dimensions as terminal columns
- **AND** repeated diagnostic passes with unchanged renderer grids preserve
  PaneSlot and ContentInstance identities

#### Scenario: Font or cell metrics change
- **WHEN** terminal font size, renderer scale, or cell metrics change while the
  window size remains the same
- **THEN** Ghostty recalculates terminal rows and columns from the new cell
  metrics
- **AND** Alan mirrors the new renderer grid to PTY and shell state

### Requirement: Non-Terminal Panes Remain Point-Native
The macOS shell SHALL keep terminal grid layout as a terminal content adapter
and MUST NOT require non-terminal PaneSlot content to size itself by terminal
rows or columns.

#### Scenario: Mixed terminal and markdown split
- **WHEN** a tab contains a split with terminal content in one PaneSlot and
  markdown content in a sibling PaneSlot
- **THEN** the split layout assigns each PaneSlot a point frame from the split
  tree and stored ratios
- **AND** the terminal PaneSlot reports the Ghostty renderer rows and columns
  inside its assigned frame
- **AND** the markdown PaneSlot lays out from its point frame without being
  snapped to terminal columns

#### Scenario: Non-terminal minimum size participates in split layout
- **WHEN** a non-terminal PaneSlot provides a minimum width or height constraint
  and a sibling terminal PaneSlot provides terminal minimum grid constraints
- **THEN** Alan resolves the split using both content constraints at the
  PaneSlot point-frame layer
- **AND** terminal row and column rounding remains scoped to Ghostty after the
  point frames are assigned

#### Scenario: Terminal remainder does not quantize sibling content
- **WHEN** a terminal pane has sub-cell remainder inside a mixed-content tab
- **THEN** Alan does not resize or quantize non-terminal sibling content solely
  to satisfy terminal column divisibility
- **AND** any terminal remainder is absorbed inside the terminal content surface
