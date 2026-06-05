## Why

Closing a pane, tab, window, or the app can currently tear down terminal
runtimes even when a foreground command or agent session is still doing useful
work. After restart, Alan restores layout and cwd, but it loses the user's
terminal screen context, so a pane that previously showed useful output opens as
a fresh shell with no visible session history.

## What Changes

- Add a unified close guard for pane, tab, window, app, and Quick Terminal close
  requests that detects active terminal work before mutating shell state or
  releasing terminal runtimes.
- Require interactive close paths to present one confirmation when any affected
  terminal has a foreground command, running alan session, pending yield, or
  unknown active-task state.
- Require automation/control-plane close paths to report `requires_confirmation`
  instead of silently killing active terminal work unless a future explicit force
  mechanism is added.
- After interactive confirmation, request a bounded graceful shutdown window for
  active terminal runtimes, then persist bounded terminal transcript snapshots
  before forced finalization, app quit, or lifecycle teardown.
- Restore terminal panes after app restart with the saved transcript, cwd, title,
  layout, focus, and shell metadata, then start a new shell in the restored cwd
  without presenting extra user-facing "restored session" chrome.
- Keep true cross-app-quit PTY/session attach as a future capability; this
  change does not keep processes alive after the app closes.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `macos-shell-terminal-lifecycle`: Add safe close confirmation and bounded
  terminal transcript snapshot requirements while preserving the existing
  explicit close finalization model.
- `macos-shell-workspace-persistence`: Extend workspace manifest restore
  snapshots with bounded terminal transcript state and require restart restore
  to materialize that state.
- `macos-terminal-runtime-foundation`: Add runtime service responsibilities for
  capturing terminal transcript snapshots and bootstrapping restored transcript
  history into a newly created runtime.
- `macos-shell-control-plane-reliability`: Require automation close commands to
  report confirmation-required results for active terminal work instead of
  silently applying destructive close mutations.
- `macos-shell-build-test-contract`: Add focused tests and UI smoke verification
  for close guard behavior, manifest round trips, and restart transcript restore.

## Impact

- `clients/apple/alan-macos/ShellHostController.swift` needs a close guard path
  around pane, tab, window, app, Quick Terminal, menu, shortcut, command UI, and
  control-plane close intents.
- `clients/apple/alan-macos/Models/Shell/ShellWorkspaceManifest.swift` and
  snapshot materialization need a manifest-compatible terminal transcript
  payload with bounded size and old-manifest compatibility.
- `clients/apple/alan-macos/TerminalRuntimeService.swift`,
  `TerminalRuntimeRegistry.swift`, `GhosttyLiveHost.swift`, and terminal surface
  adapters need a snapshot extraction seam, a confirmed-close graceful shutdown
  request seam, and a restored-transcript startup path.
- `clients/apple/alan-macos/Controllers/Shell/ShellHostControlCommandHandling.swift`
  and shell control DTOs need a stable confirmation-required response for
  guarded close commands.
- Focused Apple scripts and UI smoke coverage need scenarios for active close
  confirmation, graceful shutdown before capture, idle close bypass, manifest
  snapshot round trip, restart with restored terminal output, and continued
  input in the new shell.
