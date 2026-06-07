## Why

The current macOS Space slider uses density tiers, hover-driven expansion, dot-only
indicators, and a hard cap of nine Spaces. That made the top sidebar navigation
feel narrow, unstable, and unlike the Safari-style tab strip reference the shell
is converging on.

## What Changes

- Replace the floating Space pill/dot treatment with a continuous rounded track
  that acts as the shared background for every Space.
- Render the selected Space as a liquid-glass tab inside the track, while
  inactive Spaces remain embedded in the track background without individual
  pill surfaces.
- Support a Space icon in every Space tab and use `icon + title` when width
  allows.
- Distribute Space targets across the full rounded track until the minimum
  target width is reached, instead of retaining a fixed maximum target width.
- Replace hover-driven width, scale, and opacity changes with stable geometry;
  hover may only apply subtle foreground or tint treatment.
- Remove the nine-Space visual cap. All Spaces participate in the slider.
- Add a responsive collapse path: full title, truncated title, and finally an
  icon-only circular tab at the minimum width.
- Allow horizontal scrolling inside the rounded track when all Spaces have
  collapsed to icon-only and still overflow the sidebar width.
- Keep Space click selection, context menus, keyboard navigation, drag scrub,
  wheel scrub, and vertical-scroll pass-through semantics, but adapt them to the
  stable track model.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `macos-shell-ui-ux-conformance`: Update the default Space slider visual,
  density, overflow, icon, hover, and scrub contracts from the current
  capped/density-tier model to a Safari-style rounded track with liquid-glass
  selected tab and horizontal overflow scrolling.
- `macos-shell-workspace-persistence`: Persist optional Space presentation
  icon metadata as part of the workspace manifest and shell state projection,
  with backwards-compatible defaults for existing manifests.

## Impact

- Affected SwiftUI views:
  `clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift`.
- Affected layout helpers:
  `clients/apple/alan-macos/Support/ShellSidebarSpaceSliderLayout.swift` and
  related wheel/scrub support where coordinate mapping depends on item frames.
- Affected state and persistence models:
  `ShellSpace` and the workspace manifest Space record gain optional
  presentation icon metadata with a deterministic fallback when absent.
- Affected design tokens:
  `clients/apple/alan-macos/Support/ShellDesignTokens.swift`.
- Affected tests/contracts:
  focused Space slider layout tests, shell sidebar UI tests, and
  `clients/apple/scripts/check-shell-contracts.sh`.
- No daemon API, runtime session API, provider configuration, terminal runtime,
  icon picker, or broader Space-management changes are intended.
