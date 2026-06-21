## Context

Alan's macOS terminal UI currently receives concrete point frames from SwiftUI
layout, while Ghostty and the PTY derive terminal rows/columns later. That makes
the visible terminal grid an output of several independent rounding decisions
instead of an explicit layout contract.

The latest Alan Dev measurement for a full visible-frame window was:

- Window frame: `1512x887` points in the visible screen area.
- Terminal PTY size: `41 rows x 145 columns`.
- Control-plane state still had paths that could report view-point values such
  as `1240` as terminal columns.

The important conclusion is not that `145` is always the right width. The
conclusion is that Ghostty should be the one place that decides terminal grids,
and Alan should report whether the renderer, PTY, and control-plane state agree
with Ghostty's result.

## Goals / Non-Goals

**Goals:**

- Make Ghostty-reported terminal rows/columns the authoritative runtime unit
  for terminal PTY, renderer, transcript, and diagnostics.
- Keep point sizes as renderer/window diagnostics, not as terminal dimensions.
- Keep default pane layout point-native so split panes always occupy their full
  SwiftUI/App shell frames.
- Ensure the terminal surface visually fills its assigned pane; the integer
  character grid may be centered or padded inside the surface, but sub-cell
  leftovers must not become visible shell-layout gaps.
- Preserve maximum terminal capacity in fullscreen/full-visible-frame cases
  without forcing even columns.
- Adapt consistently across small built-in displays, common laptop displays,
  large external monitors, ultrawide displays, and changed terminal font/cell
  metrics by passing full host frames to Ghostty and observing the renderer grid.
- Keep non-terminal panes point-native so markdown, settings, document, PDF, or
  future app panes are not forced into terminal row/column constraints.
- Expose enough diagnostics to debug grid, point, renderer, PTY, sidebar, and
  split-ratio mismatches from `alan-dev shell state`.

**Non-Goals:**

- No Linux or Windows implementation in this change.
- No Ghostty renderer rewrite beyond using the existing Alan-owned PTY renderer
  attachment boundary.
- No requirement that every fullscreen or full-visible-frame terminal has an
  even root column count.
- No automatic grid-perfect pane, split, or window layout in default terminal
  workspaces.
- No Swift/App-shell reimplementation of Ghostty's grid calculation.
- No stable Alan verification target; runtime validation should use Alan Dev.
- No fallback to view-point dimensions as terminal rows/columns.
- No requirement that non-terminal content sizes itself by terminal rows or
  columns.

## Domain Language

- **Terminal grid**: Integer `rows x columns` used by the terminal emulator and
  PTY. This is the user-visible terminal capacity.
- **Renderer grid**: The grid Ghostty reports after applying a size to its
  surface.
- **PTY grid**: The grid visible to the child process through `TIOCSWINSZ` and
  tools such as `stty size`.
- **Canvas points**: The AppKit/SwiftUI point rectangle available for the
  terminal host.
- **Cell metrics**: The measured width and height in points for one terminal
  cell at the current font/configuration.
- **Display class**: A representative range of available terminal canvas sizes,
  not a hard-coded monitor model. Examples include constrained laptop canvas,
  current built-in full visible-frame canvas, large external canvas, ultrawide
  canvas, and high-font-size accessibility canvas.
- **Runtime sizing policy**: The host policy label for how Alan sizes the
  terminal surface. In this change `max-fit` means Alan gives Ghostty the full
  assigned terminal surface and accepts Ghostty's rendered grid.
- **Remainder**: Unused columns, rows, or points observed after Ghostty reports
  an integer renderer grid.
- **Sub-cell remainder**: Point space smaller than a full terminal cell after
  computing the maximum integer grid inside a terminal surface.
- **Terminal surface padding**: Terminal-owned background space used to absorb or
  center sub-cell remainders inside a pane.
- **Point-native pane**: A non-terminal PaneSlot whose content lays out from its
  assigned AppKit/SwiftUI point rectangle and its own minimum-size constraints,
  not from terminal rows or columns.

## Decisions

### Ghostty Renderer Grid Is The Runtime Source Of Truth

Alan should pass the full assigned host frame to Ghostty, then treat Ghostty's
reported renderer grid as the terminal grid source of truth. Alan-owned PTY
resize follows that renderer grid. Shell state and transcript snapshots publish
rows/columns from the renderer/PTY grid rather than from point width or height.

Alternative considered: keep point-first layout and improve rounding locally in
each split view. That keeps the current failure mode: control-plane state, PTY,
and renderer can disagree and there is no single place to explain why.

Alternative considered after Alan Dev verification: make default split pane
frames grid-aware so odd remainders are removed from the App shell layout. That
created visible unused regions and made terminal content fail to occupy the
full pane. The accepted route keeps pane frames point-native by default and
does not include grid-aware shell frame changes.

### Do Not Reimplement Ghostty Grid Math

Alan should not maintain a parallel Swift/App-shell grid solver for terminal
rows and columns. The terminal host view owns the point frame, Ghostty owns the
cell metrics and renderer grid, and Alan observes the resulting grid for PTY
resize and diagnostics.

Alternative considered: introduce a pure `ShellTerminalGridLayoutSolver` that
maps window, sidebar, divider, inset, and cell metrics into terminal grids.
That duplicated Ghostty behavior, created another rounding source, and implied
future grid-perfect split layout even after the product decision moved away
from that route.

### Keep Default Pane Layout Point-Native

The split tree remains a PaneSlot layout model for all content kinds. It assigns
point rectangles and honors each mounted content type's minimum-size
constraints. Terminal content hosts Ghostty inside its assigned rectangle and
reports the renderer grid Ghostty produces. This applies to terminal-only splits
as well as mixed content splits.

Markdown, settings, document, PDF, and future app panes should not be snapped to
terminal rows or columns. In mixed layouts such as `terminal | markdown`, the
terminal side receives its point frame and reports Ghostty diagnostics; the
markdown side receives its point frame and lays out normally.

Terminal diagnostics may report the renderer grid for affected panes after
split, window, or sidebar changes. They must not globally resize or quantize
panes to satisfy terminal column divisibility.

Alternative considered: make the entire split layout terminal-grid-aware. That
would simplify terminal-only split math but would leak terminal constraints into
the App shell frame model, leave unusable remainder areas in common windows,
and make future content panes feel incorrectly terminal-shaped.

### Fill The Terminal Surface, Not The Grid

The terminal host view must occupy the full pane frame assigned by the App
shell. Inside that frame, Ghostty uses the maximum integer grid it can render
from its current metrics and surface size, and Alan-owned PTY follows that
reported grid. If the frame leaves leftover pixels that are smaller than a full
cell, Alan should treat them as terminal-owned padding/background. The
grid can remain top-left aligned, centered, or use another stable terminal-local
padding rule, but the App shell must not show those leftovers as white borders
or unavailable pane area.

Alternative considered: shrink the pane frame to the exact cell grid size. That
keeps arithmetic neat but makes the terminal visually fail to use the full pane
and creates gaps during split/fullscreen layouts.

### Use One Runtime Sizing Policy

`max-fit` is the runtime sizing policy label for terminal hosts. Alan gives
Ghostty the full assigned terminal canvas, then accepts the renderer grid
Ghostty reports. An odd root grid such as `145` is acceptable and should remain
odd; Alan should not discard usable capacity or shrink shell frames to make it
even.

Rejected alternatives:

- Force equal terminal columns by reserving leftover space or changing pane
  frames. This makes the terminal fail to use the pane naturally.
- Choose ordinary window sizes from preferred terminal column targets. This
  overfits window placement to terminal grid arithmetic.
- Always force even root columns. That would make split math simple but would
  throw away useful fullscreen capacity and surprise users whose display
  naturally fits an odd column count.

### Validate By Display Class Matrix

The runtime should not branch on named devices or assume the current `1512x887`
visible frame. It should pass measured terminal canvas points to Ghostty and
report the renderer grid, PTY grid, cell metrics, and remainders that result.

Implementation should still maintain a test and manual verification matrix with
representative display classes:

| Class | Representative Inputs | Expected Policy Behavior |
| --- | --- | --- |
| Constrained laptop | narrow canvas, sidebar expanded, default font | renderer grid stays usable when possible and diagnostics do not publish point width as columns |
| Current built-in full visible frame | `1512x887` window, current font, sidebar state | `max-fit`, measured baseline such as `41x145`, default split panes fill their point frames, no visible sub-cell gap |
| Ordinary Alan-placed window | non-fullscreen, Alan controls placement | default layout remains point-native and Ghostty owns the grid inside the assigned terminal frame |
| Large external display | wide/tall canvas, sidebar expanded/collapsed | keep `max-fit` capacity from Ghostty and preserve split ratios/runtime identities |
| Ultrawide display | very wide canvas, multiple splits | avoid point-derived dimensions and maintain nested split stability |
| Accessibility font size | larger cell metrics on the same window | Ghostty recomputes rows/columns from cell metrics and Alan keeps PTY/renderer grids aligned |

This matrix is intentionally expressed as input classes. Actual measured
numbers can vary by macOS version, display scale, font, sidebar state, and
window chrome.

### Diagnostics Come Before Polishing Placement

The first implementation slice should publish renderer and PTY grid fields
without changing ordinary window placement. That makes it possible to verify
whether the terminal host, Ghostty, and PTY agree without coupling correctness
to grid-perfect layout.

Alternative considered: tune window sizes first. That would make the common
case look better while leaving the same blind spot for split/PTY mismatches.

## Risks / Trade-offs

- **Cell metrics are not available during early layout** -> Publish missing
  fields explicitly until Ghostty reports actual metrics and grid dimensions.
- **Resize loops between SwiftUI, Ghostty, and PTY** -> Resize PTY only when the
  Ghostty renderer grid changes, and record point-only changes separately.
- **Odd fullscreen grids make equal split arithmetic asymmetric** -> Keep
  `max-fit` capacity by design; do not force symmetric terminal grids.
- **Sidebar collapse/expand changes the canvas while terminal work is active** ->
  let the host resize Ghostty once, mirror the resulting renderer grid to PTY,
  and preserve split ratios and terminal runtime identities.
- **Diagnostics could become noisy** -> Keep full fields in debug/control-plane
  surfaces and avoid exposing them as default UI chrome.

## Migration Plan

1. Add grid diagnostics and distinguish grid dimensions from point dimensions
   in shell state and transcript snapshots.
2. Remove/pause Swift/App-shell grid solving and keep Ghostty as the owner of
   actual terminal rows and columns.
3. Keep default split tree frames and ratio persistence point-native; remove or
   pause any automatic grid-aware pane-frame snapping.
4. Pass each terminal host's full assigned point/backing frame to Ghostty.
5. Mirror Ghostty's reported renderer grid to Alan-owned PTY resize, then assert
   renderer and PTY grids converge.
6. Ensure terminal surfaces fill their pane visually and absorb sub-cell
   remainders inside terminal-owned background or padding.
7. Keep sidebar width tuning independent from grid-perfect layout.
8. Verify in Alan Dev with fullscreen/full-visible-frame, non-fullscreen,
   sidebar expanded/collapsed, two-way split, nested split, simulated
   display/canvas classes, larger terminal font/cell metrics, and clear or `ls`
   terminal commands.

Rollback is straightforward for implementation slices: keep the old point-first
layout path behind the development branch until the renderer diagnostics and
PTY synchronization tests pass, then remove it before merging the change.

## Open Questions

- No blocking product questions remain. The accepted product boundary is
  runtime grid truth plus visually full terminal surfaces, not grid-perfect
  shell layout.
