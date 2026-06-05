## Why

The first-pass macOS Space slider removes the old header and bottom dock, but
its static active-title-and-dot layout still feels under-polished compared with
native tab bars. The next iteration should make Space navigation scale cleanly
from a few named Spaces to a full set while adding a deliberate, fast scrub
interaction for switching without making the default sidebar busy.

## What Changes

- Add an adaptive Space slider density model:
  - `1-3` Spaces render as quiet Safari-like named tabs.
  - `4-6` Spaces render active Space as a full title and inactive Spaces as
    compact short-title tabs.
  - `7-9` Spaces render active Space as a title and inactive Spaces as compact
    indicators that can expand on hover or scrub focus.
- Increase the default Space cap from 8 to 9.
- Add hover preview behavior that locally expands or highlights the hovered
  Space without switching the active Space.
- Add scrub preview behavior for horizontal trackpad/wheel input and press-drag
  input on the slider.
- Commit scrub selection only after release or a short focus dwell so the tab
  pager does not churn during fast scrubbing.
- Preserve Space context menus, click-to-switch behavior, reduced-motion
  behavior, keyboard access, VoiceOver labels, and hidden-titlebar hit testing.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `macos-shell-ui-ux-conformance`: Defines adaptive Space slider density,
  hover preview, scrub preview/commit behavior, the 9-Space cap, accessibility,
  reduced-motion, and scroll-input protection.
- `macos-shell-build-test-contract`: Adds focused verification expectations for
  adaptive density tiers, slider input states, scrub preview/commit behavior,
  and titlebar/window hit-test preservation.

## Impact

- Expected Apple client changes are local to the sidebar Space slider and
  related shell input/hit-test checks, especially `ShellSidebarView.swift`,
  shared shell design tokens, and focused Apple shell scripts.
- No daemon, runtime, protocol, provider, persistence, or dependency changes are
  expected.
- Visual verification will need a fresh Alan Dev launch when implementation
  reaches review.
