## Why

Restored terminal transcript snapshots currently appear as a separate SwiftUI
text block above the live terminal. The panel makes restart context visible, but
its text does not align with the real terminal surface and shell clear actions
do not remove it. The result feels disconnected from the terminal workflow:
useful restored context stays on screen after the user has intentionally cleared
the terminal, and the typography makes the restored output harder to scan.

## What Changes

- Keep restored transcript context as a visually distinct panel above the live
  terminal rather than replaying it into the new PTY.
- Polish the restored panel so its text aligns with the live terminal's text
  column and uses matching terminal-like monospace typography, row rhythm,
  foreground treatment, and full-width leading layout.
- Add a content-level clear path that removes the restored transcript snapshot
  from shell state, the runtime restored-cache, and future manifest writes.
- Dismiss the restored panel when the user invokes terminal clear intent through
  `Ctrl-L`, typed `clear` at the prompt, or Alan's Clear command such as Cmd-K
  or menu clear.
- Keep raw renderer or process restore out of scope. The panel remains readable
  session-continuity metadata, not a claim that the prior PTY or child process
  survived restart.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `macos-shell-workspace-persistence`: Clarify that restored transcript
  snapshots may render as a distinct restored-context panel and can be cleared
  from persisted restore state.
- `macos-terminal-runtime-foundation`: Add runtime restored-transcript cache
  eviction and clear-intent handling responsibilities.
- `macos-shell-ui-ux-conformance`: Define restored transcript panel visual
  polish: terminal-aligned text, stable panel sizing, and quiet native styling.
- `macos-shell-build-test-contract`: Require focused tests for panel layout,
  clear behavior, cache eviction, and manifest persistence after dismissal.

## Impact

- `clients/apple/alan-macos/TerminalPaneView.swift` needs restored panel layout
  polish and a clear callback instead of panel-only presentation state.
- `clients/apple/alan-macos/ShellHostController.swift` and shell state mutation
  helpers need a content-scoped restored transcript removal path.
- `clients/apple/alan-macos/TerminalRuntimeRegistry.swift` and
  `TerminalRuntimeService.swift` need a matching restored-cache eviction API.
- `clients/apple/alan-macos/TerminalHostView.swift`,
  `TerminalSurfaceController.swift`, and shell command/menu routing need to map
  supported clear intents to the shared restored transcript removal path while
  preserving normal terminal delivery.
- Focused Apple scripts should cover snapshot dismissal, manifest round trips,
  runtime cache eviction, and terminal-like restored panel layout.
