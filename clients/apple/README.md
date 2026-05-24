# Alan for macOS

`clients/apple` is alan's native Apple client project, supporting macOS and iOS.

The macOS path is Alan for macOS: a real terminal workspace whose terminal
state is readable and operable by both humans and agents.

## System Requirements

- Xcode 26+
- macOS 26+ for development
- iOS 26+ simulator/device for iOS target

## Directory Structure

The current app is being split from a flat Swift source directory into durable
owner folders. The accepted target layout and current file inventory are
recorded in [`ARCHITECTURE.md`](./ARCHITECTURE.md).

- `AlanApp.swift`: current app entry point
- `App/`: macOS app delegate, duplicate-instance startup, primary shell owner,
  shell commands, and primary window presentation helpers
- `Support/`: shared design tokens, native material wrappers, and window
  placement support, plus small AppKit-backed shell support adapters
- `MacShellRootView.swift`: thin primary macOS shell composition root
- `Views/Shell/`: shell sidebar, workspace composition, command palette, and
  other primary macOS shell SwiftUI components
- `Views/Console/`: legacy/mobile remote-control console surface
- `Models/API/`: daemon API response, operation, and JSON value models
- `Models/Console/`: legacy/mobile console value types
- `Models/Shell/`: shell command enums, launch targets, snapshots, and shell mutation helpers
- `Controllers/`: target owner for observable app and shell controllers; current
  migration debt is tracked in `ARCHITECTURE.md`
- `Services/Daemon/`: daemon HTTP client, event page reader, and console event reducer ownership
- `Services/Shell/`: shell projection, persistence, control-plane transport,
  file polling, event store, diagnostics, and command-execution services that
  keep runtime metadata and IO out of the observable host
- `Services/Terminal/`: terminal host runtime reporting, window observation,
  and terminal runtime service collaborators
- `TerminalPaneView.swift` / `TerminalHostView.swift`: current terminal pane and host surfaces
- `ShellModel.swift` / `ShellHostController.swift`: current shell state and controller
- `ShellControlPlane.swift`: thin shell control-plane orchestration across the
  shell service owners

Run the current architecture report with:

```bash
bash clients/apple/scripts/check-architecture-maintainability.sh
```

## Quick Start

1. Open `clients/apple/alan-macos.xcodeproj` with Xcode
2. Select the `alan-macos` scheme
3. Select a run target: `My Mac` or an iOS simulator/device
4. Run the app

Default endpoint is `http://127.0.0.1:8090`; you can change it in the UI.

### Local Ghostty Prep

The macOS shell spike now includes a native AppKit terminal-host scaffold plus
a plain-shell-first boot contract. To prepare a local `GhosttyKit.xcframework`
for the next integration slice, run:

```bash
./clients/apple/scripts/setup-local-ghosttykit.sh
```

To check whether the ignored local links are already present without changing
the workspace, run:

```bash
./clients/apple/scripts/setup-local-ghosttykit.sh --check
```

This follows the same boundary as `cmux`: Ghostty stays external, the script
syncs artifacts into a cache outside the repo, and then creates ignored local
links at `clients/apple/GhosttyKit.xcframework`,
`clients/apple/ghostty-resources`, and `clients/apple/ghostty-terminfo`.
It prefers explicit overrides first, then a local `~/Developer/ghostty`
checkout.

By default, the macOS app boots each new pane into your login shell. You can
override that boot contract with:

```bash
ALAN_SHELL_LOGIN_SHELL=/absolute/path/to/zsh
```

Or force a one-off startup command with:

```bash
ALAN_SHELL_BOOT_COMMAND='tmux attach || tmux new'
```

If you want an alan-targeted surface to launch a specific alan binary, set:

```bash
ALAN_SHELL_ALAN_PATH=/absolute/path/to/alan
```

Without that override, the macOS terminal workspace resolves alan in this order:

1. `ALAN_SHELL_ALAN_PATH`
2. worktree-local `target/debug/alan`
3. worktree-local `target/release/alan`
4. app-bundled `Contents/Resources/bin/alan`
5. `alan` from the current `PATH`

The app bundle is also the command-line distribution unit. Homebrew cask
installs link `Contents/Resources/bin/alan` and `Contents/Resources/bin/alan-tui`
into the Homebrew prefix. For a direct app install, use **Tools > Install
Command Line Tools...** in the app to create PATH-visible symlinks. The app does
not silently modify shell startup files or use `~/.alan/bin`.

The macOS app owns one primary shell context for the process. The default shell
surface uses the stable `window_main` identity, so reopen, activation, and New
Window commands focus the existing alan window instead of creating another
control plane. Older fixed `shell-state-v0.1.json` files are not loaded; the
current persisted state file is scoped as `shell-state-window_main.json`.
During the rename, Alan for macOS reads existing shell state from the historical
`Application Support/AlanNative` directory when the new
`Application Support/alan-macos` file is missing, then writes future state only
to `Application Support/alan-macos`. No destructive migration is performed.

### Window Capture Helper

For screenshot-driven UI iteration on the native macOS app, use:

```bash
zsh ./clients/apple/scripts/capture-alan-window.sh --list
zsh ./clients/apple/scripts/capture-alan-window.sh --output .artifacts/alan-window.png
zsh ./clients/apple/scripts/capture-alan-window.sh --channel dev --output .artifacts/alan-dev-window.png
```

You can also target a specific running process:

```bash
zsh ./clients/apple/scripts/capture-alan-window.sh --pid 12345 --output .artifacts/alan-window.png
```

The helper uses ScreenCaptureKit, so it may require Screen Recording permission
for your terminal on first use.

### Shell UI Smoke

For a repeatable shell UI smoke flow, use:

```bash
just apple-shell-ui-smoke
```

The smoke command builds `alan-macos` into repo-local DerivedData, launches a
controlled app instance with isolated runtime directories and a stable zsh shell
environment, then captures screenshots under
`debug/artifacts/apple-shell-ui-smoke/`. Because the current macOS project links
Ghostty at build time, prepare the ignored local Ghostty links first:

```bash
./clients/apple/scripts/setup-local-ghosttykit.sh
```

The default flow does not require Accessibility permission: it launches the app,
drives space creation, tab creation, split creation, and terminal input through
alan's shell control plane, then captures the controlled smoke window. When
Accessibility is available for `osascript`/System Events, the script also
captures command UI, keyboard space/tab switching, and pane-scoped Find. To
require those UI-scripting steps, run:

```bash
ALAN_REQUIRE_UI_SCRIPTING_UI_SMOKE=1 just apple-shell-ui-smoke
```

When local Ghostty artifacts are prepared, the smoke also captures basic
terminal input using only static smoke text. To require terminal-specific steps,
run:

```bash
ALAN_REQUIRE_TERMINAL_UI_SMOKE=1 just apple-shell-ui-smoke
```

The smoke artifacts are generated from the controlled smoke window only; the
script builds with a dedicated smoke bundle identifier, uses the dev install
channel plus per-run shell control and Application Support paths, and does not
capture arbitrary existing terminal windows or log terminal content.

## Current Features (v0.1)

### Desktop (macOS)

- Alan for macOS root with Arc-like sidebar/workspace chrome
- Local typed shell snapshot preview
- Native AppKit terminal-host scaffold sized and focused by the shell host
- Plain-shell-first boot profile projection for the selected pane, with alan as
  an explicit optional surface type
- Ghostty readiness discovery for local developer integration
- Live Ghostty-backed host path with runtime diagnostics, fallback config, and
  command-resolution inspection
- External Ghostty artifact cache plus ignored local links and app-bundled
  resources/terminfo
- Window-scoped file/socket shell control plane with pane lifecycle events,
  bounded socket requests, diagnostic surfacing, and truthful `terminal.send_text`
  delivery results

### Mobile (iOS)

- Remote-control-first layout (Chat / Timeline dual panels)
- Same core controls as desktop:
  - connect to remote daemon
  - session switching and message submission
  - yield approval/input resume

## Protocol and Endpoints

The client uses the existing `/api/v1/sessions/*` compatibility layer:

- `POST /sessions`: create session
- `GET /sessions`: list sessions
- `POST /sessions/{id}/submit`: submit `Op`
- `GET /sessions/{id}/events/read`: incremental event polling
- `GET /sessions/{id}/read`: load session metadata + history
- `POST /sessions/{id}/fork`: fork session
- `POST /sessions/{id}/rollback`: rollback turns (in-memory only; non-durable)
- `POST /sessions/{id}/compact`: trigger compaction
- `DELETE /sessions/{id}`: delete session

## Command-Line Build

```bash
# macOS
xcodebuild \
  -project clients/apple/alan-macos.xcodeproj \
  -scheme alan-macos \
  -destination 'generic/platform=macOS' build

# Shell control-plane contract smoke
bash clients/apple/scripts/check-shell-contracts.sh

# Shell automation command seam tests
just apple-shell-automation-seams

# Ghostty-backed shell integration lane.
# Skips when local Ghostty links are absent; set ALAN_REQUIRE_GHOSTTY_INTEGRATION=1
# to make missing artifacts fail the command.
just apple-shell-ghostty-integration

# Shell UI smoke screenshots
just apple-shell-ui-smoke

# Apple source architecture maintainability report
bash clients/apple/scripts/check-architecture-maintainability.sh

# iOS
xcodebuild \
  -project clients/apple/alan-macos.xcodeproj \
  -scheme alan-macos \
  -destination 'platform=iOS Simulator,name=iPhone 16' build
```
