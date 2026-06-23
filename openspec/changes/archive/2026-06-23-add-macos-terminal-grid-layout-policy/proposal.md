## Why

Alan's macOS terminal layout is still point-first: split panes and control-plane
state can describe view dimensions while the actual Ghostty/PTY grid is only an
emergent result. Recent Alan Dev measurements showed a full visible-frame
terminal at `41x145`, while shell diagnostics could still report point-derived
values such as `1240` as terminal columns, making split sizing and resize
debugging hard to trust.

## What Changes

- Add a macOS terminal grid policy that treats integer terminal rows/columns as
  the runtime truth for PTY, renderer, transcript, and diagnostics, with
  Ghostty owning the actual grid calculation.
- Pass each terminal host's full assigned canvas to Ghostty and read Ghostty's
  reported renderer grid instead of reimplementing terminal grid math in the
  App shell. Default SwiftUI/App shell pane frames remain point-native.
- Use a single runtime sizing policy label: `max-fit`. It means Ghostty receives
  the full assigned terminal surface and keeps the maximum usable grid it can
  render, without forcing even columns or grid-perfect shell layout.
- Require terminal surfaces to visually fill their assigned panes. The terminal
  grid may use the maximum integer rows/columns inside that frame; leftover
  sub-cell pixels are terminal surface padding/background, not shell-layout
  gaps.
- Expose grid diagnostics through the macOS shell control plane so agents can
  compare window points, terminal canvas points, Ghostty grid, PTY grid, cell
  metrics, split ratios, and remainder placement.
- Fix terminal transcript/state dimensions to report terminal rows/columns, not
  view-point width or height.
- Keep the change scoped to macOS terminal layout. It does not add Linux
  behavior, replace Ghostty rendering, or force every fullscreen terminal width
  to be even.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `macos-terminal-runtime-foundation`: Require terminal runtime resize and
  transcript dimensions to use truthful Ghostty/PTY grid values and expose
  renderer-versus-PTY grid diagnostics.
- `macos-shell-workspace-interactions`: Require default split layout to remain
  point-native while terminal panes report Ghostty renderer grids inside their
  assigned frames.
- `macos-shell-control-plane-reliability`: Require shell state and mutation
  responses to expose grid layout diagnostics sufficient to debug mismatched
  point, renderer, and PTY dimensions.

## Impact

- macOS terminal runtime code will need a renderer-owned grid diagnostic
  boundary before PTY resize state is published.
- Terminal runtime service and host-view adapter code will need to project
  actual Ghostty renderer grid, PTY grid, cell metrics, and resize mismatch
  diagnostics.
- Shell state publication and `alan-dev shell state` output will need terminal
  dimension fields that are explicitly grid-based, with point dimensions kept in
  separate diagnostics fields.
- Runtime and diagnostics tests will need coverage for renderer-derived grids,
  PTY synchronization, point-only frame changes, and sub-cell remainder cases
  where the terminal surface still fills the pane.
- Alan Dev manual verification remains the expected macOS runtime validation
  target for this change.
