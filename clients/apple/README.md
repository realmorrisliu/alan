# Alan for macOS

`clients/apple` is Alan for macOS, the native Apple host for Alan.

The macOS path is Alan for macOS: a real terminal workspace whose terminal
state is readable and operable by both humans and agents.

## System Requirements

- Xcode 26+
- macOS 26+ for development

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
- `Views/Shell/`: shell composition plus focused sidebar, Space slider, tab-row,
  drop-target, activity, pane-topology, workspace, command-palette, settings,
  pane-tree, terminal-leaf, pane-title, terminal-overlay, and bounded-content
  SwiftUI components
- `Models/Shell/`: focused shell, Terminal Profile, managed-account, activity,
  context, terminal runtime, and pane/content/tree/tab/Space/workspace snapshot families; runtime support,
  focused presentation models, and settings navigation, Terminal Profile, managed-user,
  catalog, local-runtime, and diagnostics domain models
- `Controllers/`: target owner for observable app and shell controllers; current
  migration debt is tracked in `ARCHITECTURE.md`
- `Services/Shell/`: shell projection, persistence, control-plane transport,
  managed-account validation/planning/effects, privileged-helper contracts,
  file polling, event store, diagnostics, and command-execution services that
  keep runtime metadata and IO out of the observable host
- `Services/Terminal/`: terminal host runtime reporting, window observation,
  Ghostty platform adapters, host focus/pointer/keyboard/text-input adapters,
  input tracing, keyboard-layout lookup, boot resolution, render coordination,
  publication policy, agent-activity projection, and terminal runtime service collaborators
- `TerminalPaneView.swift` / `Views/Shell/Terminal/TerminalHostView.swift`: current
  terminal pane and AppKit host surfaces;
  settings, bounded content, pane-tree, terminal-leaf, title-bar, and overlay
  presentation live under `Views/Shell/`
- `ShellHostController.swift`: observable shell state and root controller lifecycle
- `ShellControlPlane.swift`: thin shell control-plane orchestration across the
  shell service owners

Run the current architecture report with:

```bash
bash clients/apple/scripts/check-architecture-maintainability.sh
```

## Quick Start

1. Open `clients/apple/alan-macos.xcodeproj` with Xcode
2. Select the `alan-macos` scheme
3. Select the `My Mac` run target
4. Run the app

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
By default, artifacts are built from the pinned Alan-maintained Ghostty fork
submodule at `third_party/ghostty`. The setup script initializes or verifies
that submodule, records source revision metadata in the artifact cache, and
reports stale local links during `--check`. Explicit developer overrides such
as `ALAN_GHOSTTY_REPO`, `ALAN_GHOSTTYKIT_PATH`,
`ALAN_GHOSTTY_RESOURCES_DIR`, and `ALAN_GHOSTTY_TERMINFO_DIR` remain supported
for fork development.

Alan's local Ghostty build is macOS-only by default. The script builds
`-Dxcframework-target=native` and `-Dsimd=false` because Alan does not need iOS
slices for the macOS terminal host, and Zig 0.15.2's bundled libc++ does not
currently compile Ghostty's SIMD C++ path against the macOS 27 SDK. Set
`ALAN_GHOSTTY_XCFRAMEWORK_TARGET=universal` or `ALAN_GHOSTTY_SIMD=true` only
when intentionally testing those upstream paths. The script also clears proxy
environment variables for Zig dependency downloads by default; set
`ALAN_GHOSTTY_ZIG_KEEP_PROXY=1` if your network requires Zig to use the process
proxy environment.

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
installs link `Contents/Resources/bin/alan` into the Homebrew prefix. For a direct app install, use **Tools > Install
Command Line Tools...** in the app to create PATH-visible symlinks. The app does
not silently modify shell startup files or use `~/.alan/bin`.

Direct app installs use **Check for Updates...** and Sparkle to read
`https://alanworks.app/appcast.xml`. Homebrew-managed installs should update
with `brew upgrade --cask alan`; Sparkle does not replace Homebrew-owned app
bundles.

The macOS app owns one primary shell context for the process. The default shell
surface uses the stable `window_main` identity, so reopen, activation, and New
Window commands focus the existing alan window instead of creating another
control plane. The durable restore authority is the channel-scoped
`shell-workspace-window_main.json` manifest. Shell-state snapshots exist only
inside the temporary CLI control-plane directory and are never persisted under
Application Support.

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

The smoke command launches the installed dev-channel app at
`~/Applications/Alan Dev.app` by default, using isolated runtime directories and
a stable zsh shell environment, then captures screenshots under
`debug/artifacts/apple-shell-ui-smoke/`. Install or refresh that app first:

```bash
just install-dev
```

To force a repo-local Debug build instead, run:

```bash
ALAN_UI_SMOKE_SKIP_BUILD=0 just apple-shell-ui-smoke
```

Because the current macOS project links Ghostty at build time, prepare the
ignored local Ghostty links before using that build mode:

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
script uses the dev install channel plus per-run shell control and Application
Support paths, and does not capture arbitrary existing terminal windows or log
terminal content.

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
- Bounded terminal transcript snapshots can seed restarted terminal panes with
  prior readable history; true PTY/process survival remains future work
- External Ghostty artifact cache plus ignored local links and app-bundled
  resources/terminfo
- Window-scoped file/socket shell control plane with pane lifecycle events,
  bounded socket requests, diagnostic surfacing, and truthful `terminal.send_text`
  delivery results

Alan for macOS owns renderer, input, windowing, terminal runtime, and local
shell-control integration. It also attaches to the matching stable/dev Alan OS
Host over aP and renders Agent Processes from Process References and caller-held
stream offsets. The app does not embed Alan Kernel, the Agent Execution Engine,
or Process lifecycle authority.

## Command-Line Build

```bash
# macOS
xcodebuild \
  -project clients/apple/alan-macos.xcodeproj \
  -scheme alan-macos \
  -configuration Debug \
  -destination 'generic/platform=macOS' \
  -derivedDataPath debug/xcode-derived/alan-macos-build \
  build

# Shell control-plane contract smoke
bash clients/apple/scripts/check-shell-contracts.sh

# Focused shell tests: model, fake runtime, control-plane, and App Intent routing.
# This target does not require real Ghostty artifacts.
just apple-shell-focused-tests

# Shell automation command seam tests
just apple-shell-automation-seams

# App Intents metadata review after building alan-macos with the command above.
just apple-shell-app-intents-metadata

# Ghostty-backed shell integration lane.
# Skips when local Ghostty links are absent; set ALAN_REQUIRE_GHOSTTY_INTEGRATION=1
# to make missing artifacts fail the command.
just apple-shell-ghostty-integration

# Shell UI smoke screenshots
# Defaults to the installed dev-channel app at ~/Applications/Alan Dev.app.
# Run `just install-dev` first; set ALAN_UI_SMOKE_SKIP_BUILD=0 to build a
# repo-local Debug Alan.app instead.
just apple-shell-ui-smoke

# Apple source architecture maintainability report
bash clients/apple/scripts/check-architecture-maintainability.sh
```
