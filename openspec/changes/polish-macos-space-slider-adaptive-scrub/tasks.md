## 1. Baseline And Model

- [x] 1.1 Confirm `polish-macos-sidebar-space-slider` is merged, archived, or explicitly selected as the implementation baseline.
- [x] 1.2 Raise the default macOS Space creation cap from 8 to 9 and update related creation guardrails.
- [x] 1.3 Extract or add a deterministic Space slider layout model for Space count, selected index, hovered index, scrub focus, available width, and reduced-motion state.
- [x] 1.4 Define layout output for the `1-3`, `4-6`, and `7-9` density tiers, including title truncation and indicator hit areas.

## 2. Static Slider Layout

- [x] 2.1 Render the `1-3` tier as full-title Safari-like tab or pill controls without Space icons.
- [x] 2.2 Render the `4-6` tier with selected full title and inactive compact short-title controls.
- [x] 2.3 Render the `7-9` tier with selected title and inactive compact indicators.
- [x] 2.4 Preserve single-line height, sidebar edge insets, fixed top slider placement, and no bottom Space dock.

## 3. Hover And Scrub Interaction

- [x] 3.1 Add hover preview state that locally highlights or expands Space targets without selecting them.
- [x] 3.2 Add press-drag scrub input with a horizontal threshold and edge resistance at first/last Space.
- [x] 3.3 Add horizontal wheel or trackpad scrub input with clear horizontal-intent gating.
- [x] 3.4 Add scrub preview state that distinguishes focus Space from the currently selected Space.
- [x] 3.5 Commit scrub selection on drag release or after the wheel/trackpad focus dwell window.
- [x] 3.6 Cancel scrub preview for Escape, context menu open, invalid target, or selected Space deletion.

## 4. Motion, Accessibility, And Hit Testing

- [x] 4.1 Add lightweight cover-flow-style scrub emphasis for the focused Space and neighboring Spaces.
- [x] 4.2 Add reduced-motion behavior that preserves the state model without scale-heavy or springy motion.
- [x] 4.3 Preserve Space context menus for selected, hovered, and scrub-focused Space targets.
- [x] 4.4 Preserve VoiceOver labels, selected-state announcement, tab count, keyboard preview, Enter commit, and Escape cancel.
- [x] 4.5 Preserve hidden-titlebar hit testing so slider controls are clickable and blank sidebar chrome remains a double-click zoom target.

## 5. Focused Verification

- [x] 5.1 Add focused layout-model tests for the `1-3`, `4-6`, and `7-9` density tiers and 9-Space cap.
- [x] 5.2 Add focused interaction tests for hover preview, click immediate switch, drag scrub preview/commit, wheel scrub preview/commit, and cancel behavior.
- [x] 5.3 Add focused scroll-routing tests proving vertical or ambiguous wheel input does not steal tab-list scrolling.
- [x] 5.4 Update shell contract checks for adaptive slider tiers, 9-Space cap, removed bottom Space dock, and hit-test preservation.
- [x] 5.5 Run focused shell/sidebar/window-placement scripts that cover the changed layout and input behavior.
- [x] 5.6 Run `openspec validate polish-macos-space-slider-adaptive-scrub --strict`.

## 6. Visual Review And Archive Readiness

- [x] 6.1 Freshly relaunch Alan Dev before visual verification.
- [x] 6.2 Capture or document light-mode sidebar evidence for representative `1-3`, `4-6`, and `7-9` Space counts.
- [x] 6.3 Capture or document hover expansion, scrub preview focus, and post-commit selected state.
- [x] 6.4 Confirm the implementation diff is limited to sidebar Space slider polish, focused tests, and this OpenSpec change.
- [ ] 6.5 Sync accepted delta specs into `openspec/specs/` after implementation is merged.
- [ ] 6.6 Archive the completed OpenSpec change after synced specs validate.
