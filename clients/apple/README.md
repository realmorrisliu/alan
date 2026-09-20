# Alan for macOS

> Lifecycle: Alan for macOS is retired as a product direction (ADR-0054). This guide is retained only for legacy maintenance until scoped source/build removal, not authorization for new desktop features or releases.

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
  XPC wire/client/listener adapters, managed-user account and PTY helper owners,
  file polling, event store, diagnostics, and command-execution services that
  keep runtime metadata and IO out of the observable host
- `Services/Terminal/`: terminal host runtime reporting, window observation,
  Ghostty platform adapters, host focus/pointer/keyboard/text-input adapters,
  input tracing, keyboard-layout lookup, boot resolution, render coordination,
  publication policy, agent-activity projection, PTY contracts and implementations,
  Ghostty bootstrap/surface adaptation, transcript capture, and the window-scoped
  terminal runtime service, plus focused scrollback, semantic-command, input,
  search, selection, metadata, surface-state, and lifecycle collaborators
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

Without that override, retained maintenance fixtures resolve alan in this order:

1. `ALAN_SHELL_ALAN_PATH`
2. worktree-local `target/debug/alan`
3. worktree-local `target/release/alan`
4. `alan` from the current `PATH`

The app bundle, embedded CLI, Homebrew cask, Sparkle feed, and appcast are
retired distribution paths. Standalone CLI/Host installation is documented in
[`docs/standalone_cli_distribution.md`](../docs/standalone_cli_distribution.md);
this source tree does not own that lifecycle.

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

### Historical Shell UI Smoke

The app-only shell UI smoke lane is retained for source maintenance and is not
part of the supported CLI/Host product or distribution gate. When maintaining
this source, invoke the script directly against an explicitly built app:

```bash
bash clients/apple/scripts/test-shell-ui-smoke.sh \
  --skip-build --app "/path/to/Alan Dev.app"
```

The script uses isolated runtime directories and writes screenshots under
`debug/artifacts/apple-shell-ui-smoke/`.

To build a repo-local Debug app as part of this maintenance lane, omit
`--skip-build` and prepare the local Ghostty links described below.

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
ALAN_REQUIRE_UI_SCRIPTING_UI_SMOKE=1 \
  bash clients/apple/scripts/test-shell-ui-smoke.sh \
  --skip-build --app "/path/to/Alan Dev.app"
```

When local Ghostty artifacts are prepared, the smoke also captures basic
terminal input using only static smoke text. To require terminal-specific steps,
run:

```bash
ALAN_REQUIRE_TERMINAL_UI_SMOKE=1 \
  bash clients/apple/scripts/test-shell-ui-smoke.sh \
  --skip-build --app "/path/to/Alan Dev.app"
```

The smoke artifacts are generated from the controlled smoke window only; the
script uses per-run shell control and Application Support paths, and does not
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

# Retained Apple-source checks are individual scripts, not root just recipes.
bash clients/apple/scripts/test-alan-os-attachment.sh
bash clients/apple/scripts/test-shell-automation-command-seams.sh
bash clients/apple/scripts/test-shell-ghostty-integration.sh
# UI smoke requires an explicitly built app; see the section above.

# Apple source architecture maintainability report
bash clients/apple/scripts/check-architecture-maintainability.sh
```
