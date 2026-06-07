## 1. Space Icon State And Persistence

- [x] 1.1 Add optional Space presentation icon metadata to the workspace manifest Space record and `ShellSpace` projection.
- [x] 1.2 Decode old manifests without icon metadata and display a deterministic default Space icon without rewriting profile definitions.
- [x] 1.3 Persist explicit Space icon metadata when present while keeping Terminal Profile icon ownership separate.
- [x] 1.4 Add focused decode/writeback tests for old manifests, explicit icon metadata, absent icon fallback, and invalid icon fallback.

## 2. Track Layout Model

- [x] 2.1 Replace count-based density tiers and `maximumVisibleSpaces` with a width-allocation model that includes every Space.
- [x] 2.2 Implement the collapse path: `icon + full title`, `icon + truncated title`, then icon-only circular minimum width.
- [x] 2.3 Compute horizontal overflow content width when all Space targets are at minimum size and still exceed available track width.
- [x] 2.4 Keep item frames, track height, hit targets, and hover geometry stable across hover, selection, scrub, and Space count changes.
- [x] 2.5 Add focused layout tests for one Space, several readable Spaces, more than nine Spaces, icon-only collapse, overflow sizing, and hover stability.

## 3. SwiftUI Slider Rendering

- [x] 3.1 Render the Space slider as one continuous rounded track aligned to the sidebar row inset.
- [x] 3.2 Render the selected Space as a compact liquid-glass tab inside the track, with adaptive material fallback when needed.
- [x] 3.3 Render inactive Spaces as transparent track content with icon/title foreground treatment rather than independent pills, cards, or dots.
- [x] 3.4 Add Space icon rendering for full, truncated, and icon-only states with stable accessibility labels.
- [x] 3.5 Embed overflow content in a horizontal track scroller and auto-scroll the selected or scrub-focused Space into view without resizing the sidebar.
- [x] 3.6 Remove hover-driven width, scale, opacity, and neighbor fade effects from the slider.

## 4. Interaction Semantics

- [x] 4.1 Preserve immediate click selection for inactive Spaces and no-op behavior for clicking the selected Space.
- [x] 4.2 Preserve Space context menus for selected, hovered, keyboard-focused, and scrub-focused Space targets.
- [x] 4.3 Adapt drag scrub and horizontal wheel scrub to the scrollable track coordinate system.
- [x] 4.4 Preserve vertical and ambiguous wheel/trackpad pass-through to sidebar scrolling.
- [x] 4.5 Preserve keyboard and VoiceOver navigation with distinct Space targets, selected state, tab count, and preview/commit/cancel semantics.
- [x] 4.6 Respect reduced motion by keeping the same state model without scale, spring, perspective, or cover-flow-like movement.

## 5. Verification

- [x] 5.1 Run focused Swift/layout tests for Space slider layout and Space icon persistence.
- [x] 5.2 Run `bash clients/apple/scripts/check-shell-contracts.sh`.
- [x] 5.3 Run the relevant macOS build or test lane for Alan Dev.
- [x] 5.4 Fresh relaunch Alan Dev only and capture light-mode screenshots for one Space, several readable Spaces, more than nine Spaces, icon-only overflow, selected liquid-glass tab, hover without geometry shift, and scrub preview.
- [ ] 5.5 Manually verify Alan Dev slider horizontal scrolling, click switching, context menu targeting, drag scrub, wheel scrub, vertical scroll pass-through, and keyboard navigation.
- [x] 5.6 Run `openspec validate polish-macos-space-slider-track --strict`.

Screenshot evidence captured:
`debug/artifacts/space-slider-track-final-ui/00-launch.png`,
`02-space-create.png`, `03-space-switch.png`,
`08-fourteen-spaces-overflow.png`, `09-overflow-no-hover.png`,
`10-overflow-hover.png`, and `11-overflow-scrub-preview.png`.
State evidence captured:
`state-after-overflow.json`, `state-after-drag-scrub.json`,
`state-after-keyboard-navigation.json`, and click-attempt snapshots.

Post-pass fixes:
User validation found the track background too faint and the Space targets still
using fixed maximum widths. The implementation now uses a dedicated gray
`sidebarSpaceSliderTrack` token and distributes readable/truncated/icon-only
targets evenly across the full track until the minimum target width would be
violated. Focused layout tests and contract checks were extended to prevent
regression to fixed maximum widths or the old faint track fill.
Fresh Alan Dev UI smoke after the fix captured updated screenshots in
`debug/artifacts/space-slider-track-feedback-ui/02-space-create.png` and
`03-space-switch.png`, showing the stronger gray track and equal distribution
for the two readable Space targets.

Manual verification notes:
`debug/artifacts/space-slider-track-manual-ui/state-before-manual-click.json`
and `state-after-click-switch-space-main.json` prove a live Alan Dev Space
slider click switched focus from `space_2` to `space_main`. The same run exposed
two Space slider targets through Accessibility with `AXPress` and `AXShowMenu`
actions, but context-menu presentation was not reliably captured from automated
coordinate or AX invocation, so 5.5 remains open for a human pass. Contract
checks now guard the context-menu profile action target (`space.spaceID`) and
drag scrub's horizontal scroll-offset mapping to keep those interaction paths
from regressing before the final human pass.

5.5 audit matrix:

- Horizontal scrolling: screenshots and layout tests prove icon-only overflow
  content exists inside the rounded track, but final human pass should confirm
  track scrolling with the pointer/trackpad.
- Click switching: live state evidence proves `space_2` -> `space_main`
  selection from a Space slider click.
- Context menu targeting: code and contract checks prove menu actions target the
  opened Space and cancel scrub preview first; Accessibility exposes `AXShowMenu`
  on Space slider targets. The final human pass must still confirm the menu
  visibly opens from selected and non-selected targets because automated
  screenshots did not capture the AppKit menu.
- Drag scrub: state and screenshot evidence prove scrub preview/commit on the
  overflow slider, and contract checks guard horizontal scroll-offset mapping.
- Wheel scrub: focused layout tests prove preview-before-commit routing and
  dwell commit; final human pass should confirm trackpad/wheel behavior in Alan
  Dev.
- Vertical scroll pass-through: focused layout tests and contract checks prove
  vertical/ambiguous wheel routing forwards to the tab list; final human pass
  should confirm ordinary sidebar scrolling over the slider.
- Keyboard navigation: state evidence proves keyboard Space navigation reached
  `space_14`, and contract checks guard preview/commit/cancel entry points.

## 6. Review And Archive Readiness

- [ ] 6.1 Request review after implementation and verification are complete.
- [x] 6.2 Before archive, sync accepted spec behavior into the long-lived `macos-shell-ui-ux-conformance` and `macos-shell-workspace-persistence` specs.
- [ ] 6.3 Archive the OpenSpec change only after implementation is merged and the long-lived specs are updated.
