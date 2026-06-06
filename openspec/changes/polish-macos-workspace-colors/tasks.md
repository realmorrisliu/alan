## 1. Surface Ownership

- [ ] 1.1 Audit every `.windowBackdrop`, `ShellMaterialBackgroundView`, and `ShellTerminalSurfaceFrame` usage in the macOS shell path.
- [ ] 1.2 Add or adjust an adaptive opaque root backing token/view so light appearance uses `rgb(1,1,1)` and dark appearance uses a solid dark base.
- [ ] 1.3 Update `MacShellRootView` so the primary shell backing uses the opaque root backing instead of root-window material, transparency, or gradient wash.
- [ ] 1.4 If any non-root overlay still needs the old material behavior, split that behavior into a narrower role instead of reusing the root backing role.

## 2. Workspace Placeholder

- [ ] 2.1 Extract the no-tab/no-pane empty Space branch into a workspace-level placeholder view or equivalent focused branch.
- [ ] 2.2 Ensure the empty placeholder uses adaptive workspace text/control tokens in both light and dark appearance.
- [ ] 2.3 Preserve the empty placeholder `New Tab` action so it creates a normal terminal tab in the current Space.
- [ ] 2.4 Remove terminal dark canvas, terminal rim, and terminal surface shadow ownership from the empty Space placeholder.

## 3. Content Surface Refactor

- [ ] 3.1 Move terminal-surface styling so it is owned by terminal content rendering rather than the whole workspace canvas.
- [ ] 3.2 Preserve single-terminal, split-terminal, and restored-transcript terminal rendering after moving the terminal surface boundary.
- [ ] 3.3 Preserve markdown and settings rendering through `ShellBoundedContentLeafView` without terminal runtime creation or terminal-only dark canvas assumptions.
- [ ] 3.4 Preserve split layout sizing, divider behavior, focus styling, zoom behavior, and close/move actions across terminal, markdown, settings, and unavailable content leaves.

## 4. Verification

- [ ] 4.1 Add or update focused Swift/script tests for root backing ownership, empty Space placeholder ownership, and terminal-surface content scoping.
- [ ] 4.2 Run `bash clients/apple/scripts/check-shell-contracts.sh`.
- [ ] 4.3 Run the relevant focused shell UI Swift script tests touched by the refactor.
- [ ] 4.4 Run the relevant macOS app build or test lane for the changed Swift files.
- [ ] 4.5 Fresh relaunch Alan Dev and capture light-mode screenshots for empty Space, terminal tab, markdown content, and settings content.
- [ ] 4.6 Fresh relaunch Alan Dev and verify dark-mode screenshots or appearance-toggle states for empty Space, terminal tab, markdown content, and settings content.
- [ ] 4.7 Confirm the light-mode root backing samples as `rgb(1,1,1)` outside content-specific surfaces.

## 5. Review And Archive Readiness

- [ ] 5.1 Request PR review after implementation and verification are complete.
- [ ] 5.2 Before archive, sync accepted spec behavior into `openspec/specs/macos-shell-ui-ux-conformance/spec.md`.
- [ ] 5.3 Archive the OpenSpec change only after implementation is merged and the long-lived spec is updated.
