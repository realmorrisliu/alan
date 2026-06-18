## 1. Baseline And Diagnostics

- [x] 1.1 Capture an Alan Dev terminal sizing matrix for full visible-frame,
  ordinary window, sidebar expanded, sidebar collapsed, two-pane split, nested
  split, larger font/cell metrics, and representative constrained, large, and
  ultrawide canvas classes.
- [x] 1.2 Add terminal grid diagnostic value types for planned grid, renderer
  grid, PTY grid, canvas points, cell points, layout policy, remainder, and
  mismatch status.
- [x] 1.3 Fix shell state and transcript snapshot dimensions so terminal rows
  and columns are grid values, with point sizes exposed only in separate
  diagnostic fields.
- [x] 1.4 Add startup/reattach diagnostics for unavailable cell metrics,
  renderer grid, or PTY grid instead of inferring grid values from points.

## 2. Renderer-Owned Grid Boundary

- [x] 2.1 Remove/pause the Swift/App-shell terminal grid solver path; Ghostty is
  the owner of actual terminal rows and columns.
- [x] 2.2 Keep terminal host layout point-native and pass the full assigned
  terminal surface to Ghostty.
- [x] 2.3 Treat Ghostty renderer grid diagnostics as authoritative for terminal
  rows/columns instead of allocating grids from the split tree.
- [x] 2.4 Keep `max-fit` as the runtime policy label and report sub-cell
  remainder only as diagnostics derived from Ghostty grid/cell metrics.
- [x] 2.5 Add focused tests for renderer-derived grid diagnostics, point-only
  frame changes, transcript dimension refresh, and PTY synchronization.
- [x] 2.6 Add mixed-content layout/control tests proving non-terminal panes keep
  point-native frames while terminal panes report Ghostty renderer grids inside
  their assigned frames.

## 3. Terminal Runtime Integration

- [x] 3.1 Report terminal host canvas size and Ghostty cell metrics into the
  terminal runtime service without making host views own runtime truth.
- [x] 3.2 Route terminal host sizing through Ghostty by applying the full host
  surface size before PTY resize.
- [x] 3.3 Apply Ghostty's reported renderer grid to Alan-owned PTY resize.
- [x] 3.4 Record mismatch diagnostics when planned, renderer, and PTY grids do
  not converge after resize settles.
- [x] 3.5 Avoid PTY resize work when point frames change but Ghostty's renderer
  grid is unchanged.

## 4. Shell Workspace Integration

- [x] 4.1 Keep default split rendering point-native while terminal hosts report
  Ghostty grids and diagnostics inside their assigned frames, preserving
  existing split tree, ratios, PaneSlot identities, and ContentInstance
  identities.
- [x] 4.2 Remove or pause automatic grid-aware split frame snapping; current
  default split and equalize behavior must not shrink pane frames solely for
  terminal grid divisibility.
- [x] 4.3 Keep fullscreen and full-visible-frame terminal runtime diagnostics on
  `max-fit` so odd root grids preserve all usable columns without forcing even
  default layout.
- [x] 4.4 Ensure terminal surfaces visually fill their assigned panes and absorb
  sub-cell remainders inside terminal-owned background or stable padding.
- [x] 4.5 Refresh the Ghostty renderer grid once when sidebar width changes
  while preserving split ratios and terminal runtime identity.
- [x] 4.6 Ensure markdown, settings, document, and other non-terminal panes are
  not snapped to terminal rows or columns in mixed split layouts.

## 5. Control Plane And Agent Observability

- [x] 5.1 Expose terminal grid diagnostics in `alan-dev shell state` for each
  terminal ContentInstance.
- [x] 5.2 Include grid diagnostics in split resize responses and relevant
  diagnostic events without describing default splits as grid-snapped.
- [x] 5.3 Ensure control-plane responses distinguish terminal grid dimensions
  from canvas or host-view point dimensions.
- [x] 5.4 Add or update control fixtures for unavailable diagnostics, mismatch
  diagnostics, and point-only frame changes.

## 6. Verification And Closeout

- [x] 6.1 Run focused unit tests for renderer grid diagnostics, PTY resize, and
  shell state dimension serialization.
- [x] 6.2 Run the relevant macOS build/test command for the Alan Dev target.
- [x] 6.3 Manually verify in Alan Dev that `stty size`, renderer diagnostics,
  and shell state agree across the sizing matrix from task 1.1, including
  simulated or real alternate display/canvas classes.
- [x] 6.4 Manually verify a mixed terminal plus non-terminal split keeps the
  non-terminal pane point-native while terminal diagnostics still report the
  correct grid.
- [x] 6.5 Verify common terminal commands such as `ls` and `clear` remain fast
  and do not create visual gaps or stale dimensions after resize.
- [x] 6.6 Review the implementation diff for unrelated changes before any PR is
  opened.
- [ ] 6.7 After implementation is merged, sync accepted delta specs into
  `openspec/specs/` and mark the change archive-ready.
