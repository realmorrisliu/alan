## 1. Window Drag Boundary

- [ ] 1.1 Disable global primary-shell background dragging for the main shell window.
- [ ] 1.2 Preserve blank top chrome drag-to-move through the existing explicit chrome overlay.
- [ ] 1.3 Verify traffic lights, sidebar titlebar controls, terminal pane titlebar controls, tab rows, and space controls are not treated as window-drag surfaces.

## 2. Verification

- [ ] 2.1 Add focused `test-shell-window-placement.swift` coverage for sidebar content points outside the draggable top chrome.
- [ ] 2.2 Run `clients/apple/scripts/test-shell-window-placement.sh`.
- [ ] 2.3 Run `openspec validate fix-macos-sidebar-window-drag-interference --strict`.
- [ ] 2.4 Capture or request running-app visual acceptance confirming tab reorder no longer moves the Alan Dev window.
