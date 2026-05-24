## Context

The primary macOS shell currently configures its window as movable by background.
That is convenient for a hidden-titlebar shell, but it makes AppKit treat more of
the SwiftUI content background as a window-drag surface. Sidebar tab rows also
own their own drag/drop gesture for reordering, which creates a conflict: a tab
drag can be interpreted partly as a window move.

Alan already has an explicit AppKit overlay for top blank chrome behavior:
`ShellWindowDoubleClickZoomOverlayView` accepts hits only in the intended
titlebar/chrome band and calls `performDrag(with:)` for non-double-click drags.
That overlay is a better owner for window dragging than global background
dragging because it can express the intended hit boundary.

## Goals / Non-Goals

**Goals:**

- Make tab reorder and sidebar space interactions never move the shell window.
- Preserve blank titlebar/chrome drag-to-move behavior.
- Preserve blank titlebar double-click visible-frame zoom behavior.
- Keep the fix narrowly scoped to shell chrome hit testing.
- Add focused regression coverage for the interaction boundary.

**Non-Goals:**

- Do not redesign tab reorder, drop targets, or space swipe behavior.
- Do not change terminal surface input ownership.
- Do not add a new visible drag handle.
- Do not change quick terminal panel behavior unless the same bug is observed
  there separately.

## Decision

Use the existing explicit chrome overlay as the only primary-shell window-drag
owner. The primary shell window should not use full `isMovableByWindowBackground`
behavior because that makes ordinary sidebar surfaces compete with window
movement. Blank top chrome remains draggable because the overlay already calls
`performDrag(with:)` for accepted non-double-click mouse-down events.

This keeps window movement tied to visible chrome intent:

- Blank top titlebar/chrome: can drag window and can double-click zoom.
- Sidebar tab rows: own tab select/reorder/drop interactions.
- Sidebar space controls and command launcher: own their own click/drag
  interactions.
- Traffic-light controls and lightweight sidebar titlebar buttons: remain
  standard controls and are not consumed by the overlay.
- Terminal surface and pane titlebar controls: keep existing terminal/pane
  ownership and do not move the window.

## Alternatives Considered

1. **Disable full background dragging and rely on explicit chrome overlay.**
   This is the selected approach. It is narrow, testable, and matches the
   existing overlay architecture.

2. **Keep full background dragging and add non-moving host views around sidebar
   controls.** This is more local in appearance but easy to miss in SwiftUI
   subtrees. Future sidebar controls could regress unless each one gets the same
   AppKit protection.

3. **Temporarily disable window background dragging only during tab drag.** This
   handles reorder but not other sidebar drags, and it introduces state recovery
   risk if drag cancellation or app deactivation interrupts the gesture.

## Verification

- Add shell window placement tests proving accepted blank top chrome still hits
  the overlay.
- Add tests proving sidebar content points below the blank chrome band are not
  window drag or zoom candidates.
- Keep existing tests proving traffic-light controls, sidebar titlebar controls,
  and terminal pane titlebar controls are not consumed by the overlay.
- Run focused macOS shell placement tests and strict OpenSpec validation.
