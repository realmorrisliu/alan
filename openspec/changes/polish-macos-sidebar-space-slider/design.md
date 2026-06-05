## Context

The current macOS shell sidebar uses three separate areas for related Space and
Tab actions:

- The top Space header shows the selected Space title, icon, and terminal
  profile menu.
- The bottom Space dock switches Spaces and creates new Spaces.
- The tab list places New Tab after the tab sections.

This layout works functionally, but it spreads the Space workflow across the
sidebar and creates extra controls in the default shell. The product direction
calls for a calmer, Arc-like material sidebar where Spaces and tabs remain
scannable and terminal content stays dominant.

## Goals / Non-Goals

**Goals:**

- Merge Space identity and Space switching into a compact top Space slider.
- Remove the bottom Space dock from the default sidebar layout.
- Move New Space into the existing sidebar titlebar control group.
- Move terminal profile selection from an always-visible header menu into the
  Space context menu.
- Place New Tab between pinned and unpinned tabs, with a divider only after
  pinned tabs exist.
- Keep interaction surfaces stable for collapsed-sidebar reveal, window dragging,
  keyboard focus, reduced motion, and accessibility.

**Non-Goals:**

- Redesign the entire sidebar visual system.
- Add complex Space overflow behavior beyond the current 8-Space cap.
- Introduce new daemon/runtime APIs or persistence formats.
- Add new Space management actions such as rename/delete unless already
  available locally.
- Replace the custom shell sidebar with `NavigationSplitView`.

## Decisions

### Decision: Replace Space header and dock with a top Space slider

The selected Space displays its title; non-selected Spaces display dots. Left
clicking a dot switches to that Space. Right clicking either the selected title
or a dot opens the corresponding Space context menu. Hovering a dot exposes the
Space title through help/tooltip text.

This keeps the default surface compact while preserving named Spaces. It also
removes the need for a persistent Space icon because the selected title becomes
the context anchor.

Alternative considered: keep the bottom Space dock and only restyle it into dot
mode. That preserves the current structure but does not solve the split between
Space identity, switching, and creation.

The Space slider is fixed at the sidebar level rather than rendered inside each
Space page. During a Space paging gesture, only the tab content moves underneath
it; the slider remains the stable global navigation control and may indicate the
target Space through dot preview state.

### Decision: Keep New Space in titlebar chrome, not the Space slider row

The New Space button joins the existing sidebar pin/unpin and appearance
controls in `MacShellRootView`. It stays icon-only and follows the same compact
chrome treatment as the adjacent controls. Pin/unpin and appearance stay on the
leading side of the sidebar titlebar, while New Space is right-aligned to the
sidebar surface with the standard trailing edge inset.

This keeps creation discoverable without turning the Space slider into a row of
mixed navigation and creation affordances. It also removes the bottom-right dock
button, which currently competes with tab-list actions.

Alternative considered: place New Space at the end of the Space slider. That is
visually compact but makes the slider handle both switching and creation,
increasing hit-test and overflow complexity.

### Decision: Move terminal profile selection to a Space context menu

The always-visible terminal profile menu is removed from the Space header. The
same profile choices move into a `Terminal Profile` submenu on the Space context
menu.

Profile selection is important but infrequent. Moving it behind right click keeps
the default sidebar focused on navigation while preserving per-Space profile
control.

Alternative considered: keep a tiny profile glyph beside the selected Space
title. That saves one click but keeps a secondary configuration control visible
in the highest-priority navigation row.

### Decision: Treat New Tab as the first unpinned-tab action

The tab list becomes pinned tabs, divider when pinned tabs exist, New Tab, then
unpinned tabs. New Tab visually behaves like a normal sidebar row with lighter
foreground treatment and creates a normal unpinned terminal tab in the current
Space.

This makes pinned tabs read as fixed entry points while New Tab remains attached
to the ordinary tab flow. There is no divider between New Tab and unpinned tabs.

Alternative considered: keep New Tab after all tabs. That is lower risk but
keeps creation far from the section it affects.

### Decision: Keep implementation scoped to existing sidebar components

The change should stay local to `ShellSidebarView`, `MacShellRootView`, shared
sidebar metrics/tokens, and focused sidebar tests. The existing sidebar paging,
collapsed floating panel, and tab drag/drop models remain in place.

This avoids broad architecture churn while allowing focused extraction of a
Space slider component and a reusable Space context menu builder.

## Risks / Trade-offs

- [Risk] Dots can become ambiguous when users have many named Spaces.
  -> Mitigation: keep the current 8-Space cap, provide help/tooltip labels, and
  preserve accessibility labels with Space title and tab count.

- [Risk] Moving New Space into the titlebar controls can interfere with hidden
  titlebar drag and double-click zoom behavior.
  -> Mitigation: extend focused window-placement checks so the right-aligned
  New Space control is an explicit interaction surface, while the spacer between
  leading controls and New Space remains blank chrome.

- [Risk] Removing the bottom Space dock changes muscle memory.
  -> Mitigation: keep the selected Space title at the top, preserve one-click dot
  switching, and maintain compact hover/selection feedback.

- [Risk] Profile selection becomes less visible.
  -> Mitigation: keep it in the Space context menu and use clear submenu labels.

- [Risk] New Tab placement can affect drag/drop insertion previews.
  -> Mitigation: keep New Tab out of drop targets and preserve insertion targets
  inside pinned and unpinned tab sections only.
