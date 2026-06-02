## Why

Recent macOS terminal performance work reduced some background rendering work,
but real daily use with multiple high-output Codex CLI sessions still feels low
frame-rate and sluggish. alan needs a user-accessible diagnostics mode that can
attribute stutter to Alan main-thread work, embedded Ghostty terminal work,
SwiftUI state projection, or child-process CPU pressure before making another
optimization pass.

## What Changes

- Add a macOS Settings-controlled `Performance Diagnostics` toggle, default off
  and available in all install channels.
- When enabled, collect a bounded in-memory performance trace for recent Alan
  terminal and shell activity without recording terminal text, prompts, command
  lines, cwd, repository paths, file paths, environment variables, or secrets.
- Record compact event and summary data for Ghostty wakeup/tick/refresh,
  terminal surface attach/catch-up, runtime snapshot publication, metadata
  callbacks, shell projection, pane-state publication, render priority, and
  visibility.
- Add low-frequency process sampling for Alan process CPU/thread metrics and
  aggregate child-process CPU pressure, without recording command lines.
- Provide an `Export Recent Diagnostics` action that writes the current local
  ring-buffer trace and summary to a user-selected diagnostics bundle.
- Add automatic stutter markers for threshold-crossing main-thread or shell /
  terminal event durations instead of requiring users to manually mark lag.
- Keep the first implementation diagnostic-only: it must not change terminal
  scheduling, rendering, focus, lazy rendering, or process lifecycle behavior.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `macos-shell-ui-ux-conformance`: Add a default-off Settings diagnostics
  control and export action that remain progressively disclosed rather than
  default shell chrome.
- `macos-terminal-runtime-foundation`: Define the privacy-preserving
  performance trace and process-sampling contract for terminal runtime,
  rendering, metadata, publication, and shell projection events.
- `macos-shell-build-test-contract`: Require focused verification that
  diagnostics can be toggled, export bounded traces, preserve privacy
  boundaries, and support real-workload diagnosis without changing terminal
  behavior.

## Impact

- `clients/apple/alan-macos/Models/Shell/ShellSettingsSurfaceModel.swift` and
  related Settings views will need a compact diagnostics toggle and export row.
- `clients/apple/alan-macos/TerminalHostRuntime.swift`,
  `TerminalRuntimeService.swift`, `TerminalHostView.swift`,
  `TerminalSurfaceController.swift`, and `GhosttyLiveHost.swift` will need
  narrow probe points around terminal runtime and rendering events.
- `clients/apple/alan-macos/ShellHostController.swift` and shell projection
  helpers will need probe points around runtime publication, metadata
  projection, selection/focus changes, and pane-state updates.
- A diagnostics controller, event recorder, process sampler, and export writer
  will need to live behind a low-overhead no-op boundary when diagnostics are
  disabled.
- Focused Apple scripts/tests will need privacy, bounded-buffer, toggle,
  export, and summary coverage.
