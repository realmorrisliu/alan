## Why

Alan's Quick Terminal Peak is specified as a detached global terminal surface,
but the current implementation couples Peak presentation to the main shell host,
workspace view tree, and synchronous terminal surface attachment. This has
already produced a stable-channel launch crash from invalid AppKit collection
behavior, and the same coupling makes shortcut-triggered Peak presentation able
to stall the main shell window before the Peak is visible.

The product contract is still sound: one global quick-terminal runtime should
survive hide/show and be promotable into a normal Alan tab. The fix is to move
Peak presentation behind a dedicated controller/window boundary while preserving
the existing runtime identity and shell command semantics.

## What Changes

- Add a dedicated Quick Terminal presentation controller that observes the
  global quick-terminal slot and drives a small window/focus state machine.
- Move `NSPanel` ownership, Space behavior, ordering, and focus timing into a
  Quick Terminal window presenter instead of embedding those details in the
  primary shell owner.
- Replace the Peak's reuse of the full workspace `TerminalPaneView` with a
  narrow Quick Terminal content view that renders only the terminal surface and
  minimal Peak chrome.
- Keep `ShellHostController` responsible for quick-terminal state mutations,
  command routing, cwd selection, close guard semantics, and `Open in Space`
  promotion.
- Keep one global terminal runtime slot; hide/show preserves the runtime, close
  tears it down, and promotion moves the existing runtime into a normal tab.
- Restore persisted quick-terminal content as hidden on app launch so restart
  never auto-presents the detached Peak.
- Stage verification so the first implementation slice proves compilation and
  stable-channel launch safety, while full state-machine, AppKit harness, and
  Quick Terminal behavior tests may follow in a dedicated verification slice.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `macos-quick-terminal-peak`: Tighten Peak ownership and lifecycle around a
  dedicated presentation boundary while preserving global runtime identity.
- `macos-shell-workspace-persistence`: Require persisted quick-terminal
  presentation to restore hidden instead of auto-presenting the Peak at launch.
- `macos-shell-build-test-contract`: Define the staged verification contract for
  the refactor, including stable-channel launch safety and later AppKit harness
  coverage.

## Impact

- `clients/apple/alan-macos/App/AlanMacPrimaryShellOwner.swift` should stop
  owning detailed Quick Terminal panel behavior directly.
- `clients/apple/alan-macos/ShellHostController.swift` should keep command and
  shell-state ownership but delegate Peak presentation to the dedicated
  controller.
- `clients/apple/alan-macos/TerminalPaneView.swift` should no longer be the
  Peak's primary composition surface; a focused Quick Terminal content view
  should provide the minimal terminal-first UI.
- `clients/apple/alan-macos/TerminalRuntimeRegistry.swift` and
  `TerminalRuntimeService.swift` may need narrow attach/focus seams so the
  presentation controller can defer terminal surface work until the Peak window
  is visible.
- Focused Apple scripts, an AppKit harness, and stable-channel verification
  should be updated in the follow-up verification slice.
