## 1. Space Icon State And Persistence

- [ ] 1.1 Add optional Space presentation icon metadata to the workspace manifest Space record and `ShellSpace` projection.
- [ ] 1.2 Decode old manifests without icon metadata and display a deterministic default Space icon without rewriting profile definitions.
- [ ] 1.3 Persist explicit Space icon metadata when present while keeping Terminal Profile icon ownership separate.
- [ ] 1.4 Add focused decode/writeback tests for old manifests, explicit icon metadata, absent icon fallback, and invalid icon fallback.

## 2. Track Layout Model

- [ ] 2.1 Replace count-based density tiers and `maximumVisibleSpaces` with a width-allocation model that includes every Space.
- [ ] 2.2 Implement the collapse path: `icon + full title`, `icon + truncated title`, then icon-only circular minimum width.
- [ ] 2.3 Compute horizontal overflow content width when all Space targets are at minimum size and still exceed available track width.
- [ ] 2.4 Keep item frames, track height, hit targets, and hover geometry stable across hover, selection, scrub, and Space count changes.
- [ ] 2.5 Add focused layout tests for one Space, several readable Spaces, more than nine Spaces, icon-only collapse, overflow sizing, and hover stability.

## 3. SwiftUI Slider Rendering

- [ ] 3.1 Render the Space slider as one continuous rounded track aligned to the sidebar row inset.
- [ ] 3.2 Render the selected Space as a compact liquid-glass tab inside the track, with adaptive material fallback when needed.
- [ ] 3.3 Render inactive Spaces as transparent track content with icon/title foreground treatment rather than independent pills, cards, or dots.
- [ ] 3.4 Add Space icon rendering for full, truncated, and icon-only states with stable accessibility labels.
- [ ] 3.5 Embed overflow content in a horizontal track scroller and auto-scroll the selected or scrub-focused Space into view without resizing the sidebar.
- [ ] 3.6 Remove hover-driven width, scale, opacity, and neighbor fade effects from the slider.

## 4. Interaction Semantics

- [ ] 4.1 Preserve immediate click selection for inactive Spaces and no-op behavior for clicking the selected Space.
- [ ] 4.2 Preserve Space context menus for selected, hovered, keyboard-focused, and scrub-focused Space targets.
- [ ] 4.3 Adapt drag scrub and horizontal wheel scrub to the scrollable track coordinate system.
- [ ] 4.4 Preserve vertical and ambiguous wheel/trackpad pass-through to sidebar scrolling.
- [ ] 4.5 Preserve keyboard and VoiceOver navigation with distinct Space targets, selected state, tab count, and preview/commit/cancel semantics.
- [ ] 4.6 Respect reduced motion by keeping the same state model without scale, spring, perspective, or cover-flow-like movement.

## 5. Verification

- [ ] 5.1 Run focused Swift/layout tests for Space slider layout and Space icon persistence.
- [ ] 5.2 Run `bash clients/apple/scripts/check-shell-contracts.sh`.
- [ ] 5.3 Run the relevant macOS build or test lane for Alan Dev.
- [ ] 5.4 Fresh relaunch Alan Dev only and capture light-mode screenshots for one Space, several readable Spaces, more than nine Spaces, icon-only overflow, selected liquid-glass tab, hover without geometry shift, and scrub preview.
- [ ] 5.5 Manually verify Alan Dev slider horizontal scrolling, click switching, context menu targeting, drag scrub, wheel scrub, vertical scroll pass-through, and keyboard navigation.
- [ ] 5.6 Run `openspec validate polish-macos-space-slider-track --strict`.

## 6. Review And Archive Readiness

- [ ] 6.1 Request review after implementation and verification are complete.
- [ ] 6.2 Before archive, sync accepted spec behavior into the long-lived `macos-shell-ui-ux-conformance` and `macos-shell-workspace-persistence` specs.
- [ ] 6.3 Archive the OpenSpec change only after implementation is merged and the long-lived specs are updated.
