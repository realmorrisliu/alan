## ADDED Requirements

### Requirement: Shell State Exposes Terminal Grid Layout Diagnostics
The macOS shell control plane SHALL expose terminal grid layout diagnostics in
shell state for terminal ContentInstances so agents can distinguish terminal
rows/columns from window, canvas, or host-view point dimensions.

#### Scenario: Agent reads terminal grid diagnostics
- **WHEN** an agent queries shell state for a tab containing terminal content
- **THEN** each terminal ContentInstance projection includes grid diagnostics
  with renderer-derived planned grid, renderer grid when known, PTY grid when
  known, canvas point size, cell point size, active layout policy, and mismatch
  status
- **AND** point dimensions are not serialized as terminal rows or columns

#### Scenario: Diagnostic fields are partially unavailable
- **WHEN** renderer, PTY, or cell-metric fields are unavailable during startup,
  restore, or renderer reattachment
- **THEN** shell state marks those fields unavailable explicitly
- **AND** the control plane keeps serving bounded shell state responses

### Requirement: Split Mutations Report Terminal Grid Diagnostics
The macOS shell control plane SHALL report terminal grid diagnostics for split,
window, and sidebar resize observations when terminal content is mounted in
affected PaneSlots. These diagnostics SHALL NOT imply that default split
rendering is grid-snapped.

#### Scenario: Split resize reports terminal diagnostics
- **WHEN** a control client resizes a split for a tab with terminal panes
- **THEN** the response reports `applied: true`, the affected tab, and the
  resulting terminal grid diagnostics for affected panes
- **AND** the response does not describe the split as grid-fitted or
  grid-snapped

#### Scenario: Resize reports mismatch
- **WHEN** a control client or diagnostic path observes a terminal pane resize
  whose renderer grid differs from PTY actual grid
- **THEN** the response or diagnostic event reports a stable mismatch category
  and the renderer-derived planned, renderer, and PTY grids

#### Scenario: Point-only change does not become terminal resize
- **WHEN** a host-view point frame changes but the Ghostty renderer grid is
  unchanged
- **THEN** the control plane does not report a terminal row or column change
- **AND** terminal command responses continue to use the last authoritative
  terminal grid
