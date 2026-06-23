## Scheduling Contract

- Terminal process lifetime, PTY IO, scrollback, and pending input delivery stay real time for foreground, visible background, and hidden background panes.
- `TerminalRuntimeRenderPriority` controls focus, occlusion, refresh cadence, catch-up, and SwiftUI publication. It does not suspend a terminal runtime or replace the `ContentInstance` identity.
- Foreground interactive work drains first, visible background work drains next, and hidden background work drains last. Hidden surfaces retain pending state and force catch-up when promoted to visible or foreground.
- Runtime publication keeps the latest state in the service for every terminal while throttling hidden scrollback and renderer churn. Sidebar-relevant title, cwd, exit, bell, attention, and failure summaries remain publishable on a bounded path.

## Ghostty Alignment

- Embedded Ghostty wakeups now route through a window-scoped `TerminalRenderCoordinator`.
- Ghostty app tick processing is separated from surface refresh painting, so hidden surfaces can process required state without repainting on every high-output wakeup.
- Priority changes propagate focus and occlusion to the embedded surface before Alan treats the terminal as foreground-current.
- Coordinator teardown cancels pending work and does not resurrect closed terminal handles.

## Stress Evidence

- Installed with `just install-dev` using Developer ID signing into `/private/tmp/alan-app-install/Alan Dev.app`.
- Relaunched the installed app with `clients/apple/scripts/test-shell-ui-smoke.sh --skip-build --app /private/tmp/alan-app-install/Alan Dev.app --terminal-steps always --ui-scripting-steps never --keep-running`.
- UI smoke captured:
  - `/private/tmp/alan-render-ui-smoke-metrics-5/00-launch.png`
  - `/private/tmp/alan-render-ui-smoke-metrics-5/01-space-create.png`
  - `/private/tmp/alan-render-ui-smoke-metrics-5/02-tab-open.png`
  - `/private/tmp/alan-render-ui-smoke-metrics-5/03-split-right.png`
  - `/private/tmp/alan-render-ui-smoke-metrics-5/04-terminal-input.png`
- Manual control-plane stress used root `/private/var/folders/3v/mr9cv4y12l30h9y_mtc2txx80000gn/T/alan-render-stress-smoke-metrics-5/window_main`.
- Stress scenario:
  - hidden tab `pane_5` ran 2500 lines of delayed output while backgrounded;
  - visible split `pane_6` ran 1800 lines of delayed output;
  - foreground `pane_4` accepted repeated `terminal.send_text` inputs and focus changes while the background output was active;
  - switching back to `pane_5` showed the existing runtime caught up without restart.
- `terminal.render_metrics` evidence was written to `/private/tmp/alan-render-ui-smoke-metrics-5/stress-summary.json`:
  - `wakeupRequests`: 619
  - `drainBatches`: 471
  - `surfaceRefreshes`: 514
  - `coalescedSurfaceRefreshes`: 7
  - `catchUpRefreshes`: 166
  - `foregroundInteractiveDrains`: 50
  - `visibleBackgroundDrains`: 407
  - `hiddenBackgroundDrains`: 64
  - `maxDrainBatchSize`: 3
  - `maxDrainLatencyMs`: 82.528666
- Final stress state kept `pane_4`, `pane_5`, and `pane_6` `input_ready=true` with `surface_readiness=ready`; `pane_5` and `pane_6` ended `process_state=running`, and `pane_4` was still accepting foreground command input.
