# macOS Sidebar Window Drag Interference Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent sidebar tab and space interactions from moving the Alan macOS shell window while preserving blank chrome drag and double-click zoom.

**Architecture:** The main shell window should not use global background dragging. Window movement remains owned by the existing explicit top-chrome AppKit overlay, whose hit test already excludes sidebar controls, traffic lights, and terminal pane chrome.

**Tech Stack:** Swift, SwiftUI/AppKit interop, OpenSpec, shell script Swift smoke tests.

---

## File Structure

- Modify `clients/apple/alan-macos/Support/ShellWindowPlacement.swift`: disable global background dragging for the primary shell window configuration.
- Modify `clients/apple/scripts/test-shell-window-placement.swift`: add regression coverage for sidebar tab-list and space-dock regions below the explicit draggable chrome band.
- Modify `openspec/changes/fix-macos-sidebar-window-drag-interference/tasks.md`: mark completed implementation and verification tasks.

### Task 1: Add Failing Placement Tests

**Files:**
- Modify: `clients/apple/scripts/test-shell-window-placement.swift`

- [ ] **Step 1: Add the failing tests to the test runner**

Add these calls near the other `ShellWindowDoubleClickZoomHitTesting` tests:

```swift
try verifiesSidebarTabListDoesNotTriggerWindowDrag()
try verifiesSidebarSpaceDockDoesNotTriggerWindowDrag()
```

- [ ] **Step 2: Add concrete hit-testing scenarios**

Add these methods near the existing sidebar chrome hit tests:

```swift
private static func verifiesSidebarTabListDoesNotTriggerWindowDrag() throws {
    let window = NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: 800, height: 520),
        styleMask: [.titled, .closable, .miniaturizable, .resizable],
        backing: .buffered,
        defer: false
    )
    let chromeSurface = ShellWindowChromeSurface(width: 264)

    let location = CGPoint(x: 128, y: window.frame.height - 128)

    expect(
        !ShellWindowDoubleClickZoomHitTesting.isWindowTopChromeZoomCandidate(
            locationInWindow: location,
            in: window,
            chromeSurface: chromeSurface
        ),
        "sidebar tab-list rows must not be treated as window drag or zoom candidates"
    )
}

private static func verifiesSidebarSpaceDockDoesNotTriggerWindowDrag() throws {
    let window = NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: 800, height: 520),
        styleMask: [.titled, .closable, .miniaturizable, .resizable],
        backing: .buffered,
        defer: false
    )
    let chromeSurface = ShellWindowChromeSurface(width: 264)

    let location = CGPoint(x: 128, y: 56)

    expect(
        !ShellWindowDoubleClickZoomHitTesting.isWindowTopChromeZoomCandidate(
            locationInWindow: location,
            in: window,
            chromeSurface: chromeSurface
        ),
        "sidebar space switcher rows must not be treated as window drag or zoom candidates"
    )
}
```

- [ ] **Step 3: Run focused test and confirm the existing hit-testing tests pass**

Run: `clients/apple/scripts/test-shell-window-placement.sh`

Expected: the added hit-testing scenarios pass because the explicit overlay already rejects these points. The product bug still exists until global window background dragging is disabled.

### Task 2: Disable Global Primary-Shell Background Dragging

**Files:**
- Modify: `clients/apple/alan-macos/Support/ShellWindowPlacement.swift`

- [ ] **Step 1: Change the window configuration**

Replace:

```swift
window.isMovableByWindowBackground = true
```

with:

```swift
window.isMovableByWindowBackground = false
```

in `AlanShellWindowPlacement.configure(_:, appearanceMode:)`.

- [ ] **Step 2: Run focused placement tests**

Run: `clients/apple/scripts/test-shell-window-placement.sh`

Expected: PASS. Existing overlay tests still prove blank chrome hits are accepted, while sidebar interaction regions are rejected.

### Task 3: Mark OpenSpec Tasks Complete

**Files:**
- Modify: `openspec/changes/fix-macos-sidebar-window-drag-interference/tasks.md`

- [ ] **Step 1: Mark implementation tasks complete**

Change completed task checkboxes for:

```markdown
- [x] 1.1 Disable global primary-shell background dragging for the main shell window.
- [x] 1.2 Preserve blank top chrome drag-to-move through the existing explicit chrome overlay.
- [x] 1.3 Verify traffic lights, sidebar titlebar controls, terminal pane titlebar controls, tab rows, and space controls are not treated as window-drag surfaces.
- [x] 2.1 Add focused `test-shell-window-placement.swift` coverage for sidebar content points outside the draggable top chrome.
- [x] 2.2 Run `clients/apple/scripts/test-shell-window-placement.sh`.
- [x] 2.3 Run `openspec validate fix-macos-sidebar-window-drag-interference --strict`.
```

Leave visual acceptance incomplete until Alan Dev is installed and the user confirms the interaction:

```markdown
- [ ] 2.4 Capture or request running-app visual acceptance confirming tab reorder no longer moves the Alan Dev window.
```

- [ ] **Step 2: Validate the OpenSpec change**

Run: `openspec validate fix-macos-sidebar-window-drag-interference --strict`

Expected: `Change 'fix-macos-sidebar-window-drag-interference' is valid`

### Task 4: Full Verification And Commit

**Files:**
- Verify all modified files.

- [ ] **Step 1: Run whitespace and OpenSpec validation**

Run:

```bash
git diff --check
openspec validate --all --strict
```

Expected: no whitespace errors and all OpenSpec items pass.

- [ ] **Step 2: Review diff**

Run: `git diff --stat && git diff`

Expected: diff contains only the plan, focused Swift placement test updates, the `isMovableByWindowBackground` change, and OpenSpec task status updates.

- [ ] **Step 3: Commit**

Run:

```bash
git add clients/apple/alan-macos/Support/ShellWindowPlacement.swift \
  clients/apple/scripts/test-shell-window-placement.swift \
  docs/superpowers/plans/2026-05-24-macos-sidebar-window-drag-interference.md \
  openspec/changes/fix-macos-sidebar-window-drag-interference
git commit -m "Fix sidebar window drag interference"
```

Expected: one implementation commit on `fix/macos-sidebar-window-drag-interference`.
