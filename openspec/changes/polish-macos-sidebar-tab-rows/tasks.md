## 1. Row Metrics And Subtitle Semantics

- [ ] 1.1 Add a single compact sidebar row metric source for row height, icon slot, text spacing, close slot, horizontal inset, and drag midpoint.
- [ ] 1.2 Add a focused helper for meaningful sidebar tab subtitles so fallback or duplicate metadata can use the single-line layout.
- [ ] 1.3 Cover subtitle and metric helper behavior with focused Swift script tests or adjacent model tests.

## 2. Clear Temporary Tabs Behavior

- [ ] 2.1 Add a state-level or host-level operation that computes current-Space inactive unpinned tabs eligible for Clear.
- [ ] 2.2 Implement batch cleanup so Clear closes eligible tabs in one deterministic operation while preserving selected tab and pane focus.
- [ ] 2.3 Ensure Clear retains pinned tabs, the selected tab, tabs in other Spaces, and tabs protected by `ShellTabActiveTaskState.protectsFromPruning`.
- [ ] 2.4 Add tests for Clear eligibility, protected-tab retention, selected-tab retention, other-Space isolation, and empty-Space outcomes.

## 3. Sidebar UI Implementation

- [ ] 3.1 Update `ShellCompactEmptyAction` or its replacement so New Tab uses the compact row metrics and Arc-like idle, hover, and keyboard-focus states.
- [ ] 3.2 Update `ShellTabSidebarRow` so ordinary tabs render compact single-line and two-line layouts without row-size shifts.
- [ ] 3.3 Add the Clear affordance above New Tab only when the active Space has eligible inactive unpinned tabs.
- [ ] 3.4 Update drag/drop insertion midpoint and hit geometry to match the compact row metrics.
- [ ] 3.5 Preserve existing context menu, close button, pin/unpin, reorder, and tab creation behavior.

## 4. Verification

- [ ] 4.1 Run the focused Swift script tests covering row semantics, Clear behavior, and tab organization.
- [ ] 4.2 Run the relevant macOS build or test lane for the Alan app.
- [ ] 4.3 Fresh relaunch Alan Dev and capture screenshots for empty Space, New Tab hover, tabs present, Clear visible, single-line tab, and two-line tab states.
- [ ] 4.4 Compare screenshots against the Arc references for compact row height, muted New Tab idle state, hover background, Clear placement, and absence of layout shifts.

## 5. Review And Archive Readiness

- [ ] 5.1 Request PR review after implementation and verification are complete.
- [ ] 5.2 Before archive, sync accepted spec behavior into `openspec/specs/macos-shell-ui-ux-conformance/spec.md`.
- [ ] 5.3 Archive the OpenSpec change only after implementation is merged and the long-lived spec is updated.
