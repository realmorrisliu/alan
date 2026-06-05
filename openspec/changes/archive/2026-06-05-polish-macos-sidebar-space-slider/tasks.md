## 1. Sidebar Layout Implementation

- [x] 1.1 Replace the existing Space header in `ShellSidebarView` with a top Space slider that shows the selected Space title and non-selected Space dots.
- [x] 1.2 Remove the bottom Space dock from the default sidebar layout and preserve stable bottom padding.
- [x] 1.3 Add an icon-only New Space control to the sidebar titlebar tool group in `MacShellRootView`.
- [x] 1.4 Move terminal profile selection from the visible Space header into a Space context menu available from the selected title and non-selected dots.
- [x] 1.5 Reorder the tab list so pinned tabs render first, then a divider only when pinned tabs exist, then New Tab, then unpinned tabs.
- [x] 1.6 Keep New Tab as a compact sidebar row that creates a normal unpinned terminal tab in the current Space and is not a drag/drop insertion target.

## 2. Interaction And Accessibility

- [x] 2.1 Verify selected Space title left click is a no-op and right click opens the selected Space context menu.
- [x] 2.2 Verify non-selected Space dots left click switch Spaces, hover exposes the Space title, and right click opens the target Space context menu.
- [x] 2.3 Preserve accessibility labels for selected Space, non-selected Space dots, New Space, New Tab, and Space profile actions.
- [x] 2.4 Preserve reduced-motion, reduced-transparency, increased-contrast, hover, selection, and keyboard-focus behavior for the new sidebar controls.

## 3. Focused Tests

- [x] 3.1 Add or update focused Swift sidebar tests for layout ordering, bottom Space dock removal, titlebar New Space placement, and profile menu disclosure.
- [x] 3.2 Update window-placement or sidebar interaction tests so the Space slider, dots, titlebar New Space button, and New Tab row are not treated as hidden-titlebar window-drag surfaces.
- [x] 3.3 Run the focused sidebar/window-placement test scripts that cover the changed layout and hit-test behavior.
- [x] 3.4 Run `openspec validate polish-macos-sidebar-space-slider --strict`.

## 4. Visual Verification

- [x] 4.1 Freshly relaunch Alan Dev before visual verification.
- [x] 4.2 Capture or document a light-mode pinned-sidebar screenshot showing the Space slider, titlebar New Space control, pinned divider behavior, New Tab row, unpinned tabs, and no bottom Space dock.
- [x] 4.3 If collapsed-sidebar rendering changes, capture or document the collapsed floating sidebar reveal.

Note: visual verification used a freshly launched isolated `app.alanworks.macos.ui-smoke`
build of the current app because the installed Alan Dev path is blocked by the
local install/signing environment. Evidence:
`debug/artifacts/sidebar-space-slider-ui-smoke-fixed/00-launch.png` covers the
default Space slider, titlebar New Space button, New Tab row, selected unpinned
tab, and no bottom Space dock. `debug/artifacts/sidebar-space-slider-pinned-fixture/output/00-launch.png`
covers pinned tab, pinned/New Tab divider placement, New Tab, unpinned tab, and
no bottom Space dock. The collapsed-sidebar wrapper was not changed, so no
separate collapsed reveal capture was required.

## 5. Review And Archive Readiness

- [x] 5.1 Confirm the implementation diff is limited to the sidebar layout, focused tests, and this OpenSpec change.
- [x] 5.2 Sync accepted delta specs into `openspec/specs/` after implementation is merged.
- [x] 5.3 Archive the completed OpenSpec change after synced specs validate.

Note: `git diff --name-only` is limited to the sidebar/root view, shell design
tokens, focused shell scripts, and the user-requested `AGENTS.md` OpenSpec
workflow rule. The unrelated untracked
`openspec/changes/add-macos-alan-owned-pty-runtime/` change remains untouched.
