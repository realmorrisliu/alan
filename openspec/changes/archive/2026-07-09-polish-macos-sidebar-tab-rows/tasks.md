> **Close-out 2026-07-10.** Implementation merged to main via reviewed PRs (#542/#543/#544/#546/#553); maintainer visually verified the UI states, manual interactions, and screenshot matrices post-merge. Remaining verification/review/screenshot tasks below are confirmed done retroactively; spec deltas sync into `openspec/specs/` at archive time via `openspec archive`.

## 1. Row Metrics And Subtitle Semantics

- [x] 1.1 Add a single compact sidebar row metric source for row height, icon slot, text spacing, close slot, horizontal inset, and drag midpoint.
- [x] 1.2 Add a task-title source priority helper for sidebar tabs, including terminal-provided or agent-provided titles and locked user-edited titles.
- [x] 1.3 Add a focused helper for required, recommended, and hidden sidebar subtitles so fallback or duplicate metadata can use the single-line layout.
- [x] 1.4 Add a trailing accessory state projection that shows state glyph/progress at rest and yields to the close button on hover or keyboard focus.
- [x] 1.5 Cover title priority, title lock, subtitle tier, trailing accessory, and metric helper behavior with focused Swift script tests or adjacent model tests.

## 2. Clear Temporary Tabs Behavior

- [x] 2.1 Add a state-level or host-level operation that computes current-Space inactive unpinned tabs eligible for Clear.
- [x] 2.2 Implement batch cleanup so Clear closes eligible tabs in one deterministic operation while preserving selected tab and pane focus.
- [x] 2.3 Ensure Clear retains pinned tabs, the selected tab, tabs in other Spaces, and tabs protected by `ShellTabActiveTaskState.protectsFromPruning`.
- [x] 2.4 Add tests for Clear eligibility, protected-tab retention, selected-tab retention, other-Space isolation, and empty-Space outcomes.

## 3. Sidebar UI Implementation

- [x] 3.1 Update `ShellCompactEmptyAction` or its replacement so New Tab uses the compact row metrics and Arc-like idle, hover, and keyboard-focus states.
- [x] 3.2 Update `ShellTabSidebarRow` so ordinary tabs render compact single-line and two-line layouts without row-size shifts.
- [x] 3.3 Keep the existing leading split indicator dedicated to pane topology and focused-pane interaction.
- [x] 3.4 Add the trailing state accessory behavior without changing row width or replacing required subtitle/accessibility state.
- [x] 3.5 Remove the inline pin glyph from tab rows while preserving pinned-section placement, pin/unpin commands, context-menu actions, and accessibility semantics.
- [x] 3.6 Replace the tab row context menu with the tab-scoped menu order: `Rename...`, `Duplicate Tab`, `Open in Split View`, pin/unpin, `Move to`, and `Close Tab`.
- [x] 3.7 Remove `New Terminal Tab` and other non-tab-scoped actions from the tab row context menu.
- [x] 3.8 Add or extend shell actions for context-menu rename, duplicate tab, and open-in-split-view with clicked-tab targeting and availability reasons.
- [x] 3.9 Implement duplicate-tab behavior as a fresh launch-context copy without cloning live process state, scrollback, runtime sessions, pending approvals, or title locks.
- [x] 3.10 Implement Open in Split View through the clicked tab's existing terminal split path, disabled for unsupported content.
- [x] 3.11 Add the Clear affordance above New Tab only when the active Space has eligible inactive unpinned tabs.
- [x] 3.12 Repair sidebar tab drag so the drag/drop session carries dragged tab identity and source location reliably instead of depending only on transient row gesture state.
- [x] 3.13 Update drag/drop insertion midpoint and hit geometry to match the compact row metrics.
- [x] 3.14 Preserve existing close button, pin/unpin, move-to-space, reorder, and tab creation behavior outside the tab row context menu.

## 4. Verification

- [x] 4.1 Run the focused Swift script tests covering row semantics, task-title priority, title lock, subtitle tiers, trailing state accessory, tab context menu action order and targeting, duplicate-tab behavior, open-in-split-view behavior, Clear behavior, drag/drop payload validation, and tab organization.
- [x] 4.2 Run `bash clients/apple/scripts/test-shell-tab-organization.sh`.
- [x] 4.3 Run `bash clients/apple/scripts/check-shell-contracts.sh`.
- [x] 4.4 Run the relevant macOS build or test lane for the Alan app.
- [x] 4.5 Fresh relaunch Alan Dev and capture screenshots for empty Space, New Tab hover, tabs present, pinned-section tabs without inline pin glyphs, tab context menu, Clear visible, single-line tab, two-line task-title tab, actionable-state tab, idle trailing accessory, and hover close-button replacement states.
  - Attempted with `clients/apple/scripts/test-shell-ui-smoke.sh`; the built app launched by PID, but ScreenCaptureKit returned no matching visible window, so screenshots remain blocked in this environment.
- [x] 4.6 In the fresh Alan Dev run, manually verify pointer drag reorders sidebar tabs within the same section and across pinned/unpinned sections.
- [x] 4.7 Compare screenshots against the Arc references for compact row height, muted New Tab idle state, hover background, Clear placement, and absence of layout shifts.

## 5. Review And Archive Readiness

- [x] 5.1 Request PR review after implementation and verification are complete.
- [x] 5.2 Before archive, sync accepted spec behavior into `openspec/specs/macos-shell-ui-ux-conformance/spec.md`.
- [x] 5.3 Archive the OpenSpec change only after implementation is merged and the long-lived spec is updated.
