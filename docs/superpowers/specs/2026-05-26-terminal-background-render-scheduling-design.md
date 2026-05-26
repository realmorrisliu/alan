# Alan Terminal Background Render Scheduling Design

Date: 2026-05-26

## Problem

Alan keeps terminal panes alive across spaces, tabs, and splits. This is the
right product behavior, but it also means every active terminal can contribute
PTY work, renderer wakeups, metadata updates, and SwiftUI invalidation.

When many shells are open, or when background shells produce heavy output, Alan
can feel slow at the UI layer. The most important symptom to address is that a
high-output background pane must not drag down sidebar switching, tab and space
navigation, or the currently focused pane.

## Goals

- Keep background shells running in real time.
- Do not drop terminal output or pause PTY/child-process execution.
- Prevent hidden background terminals from producing main-thread refresh work at
  output frequency.
- Keep sidebar, tab, and space switching responsive while background panes are
  busy.
- Keep the selected terminal responsive for input and immediate refresh.
- Use Ghostty's macOS and renderer ownership model as the behavioral reference.

## Non-Goals

- Do not fork or modify Ghostty core for this optimization.
- Do not suspend, kill, or checkpoint background shell processes.
- Do not make hidden terminal contents visually live while they are not visible.
- Do not add visible diagnostic UI to the default shell surface.

## Current Evidence

Alan's embedded Ghostty host currently receives Ghostty wakeups and schedules
work onto the main queue from each live host. `AlanGhosttyLiveHost.scheduleTick`
coalesces within a single host, then performs `ghostty_app_tick(app)` and
`ghostty_surface_refresh(surface)` on the main queue.

Alan's terminal lifecycle keeps active terminal mounts alive across workspace
state, which is correct for real-time background execution. The missing boundary
is not process lifetime; it is render and UI publication priority.

Ghostty separates PTY/IO, renderer, and app-thread responsibilities. Each
surface owns IO and renderer threads. The renderer tracks visible and focused
state, adjusts macOS QoS, and avoids draw work when invisible. Ghostty's macOS
controller also synchronizes surface occlusion from window visibility.

## Design

### Surface Runtime Priority

Each terminal surface gets a runtime priority derived from shell state,
visibility, and focus:

- `foregroundInteractive`: the selected pane receiving user input.
- `visibleBackground`: a visible split pane that is not selected.
- `hiddenBackground`: panes hidden by tab, space, zoom, or window visibility.

These priorities do not affect shell process execution. They only control
rendering, refresh, and SwiftUI publication.

`foregroundInteractive` remains fully live: input, refresh, metadata, and
runtime snapshots publish immediately.

`visibleBackground` remains visually live but is coalesced to frame cadence so
it cannot starve the focused pane.

`hiddenBackground` keeps its PTY and terminal state live, but it is explicitly
unfocused and occluded from the embedded Ghostty surface perspective. It should
not request high-frequency surface refresh. When a hidden surface becomes
visible, Alan performs an immediate catch-up tick and refresh before treating it
as visually current.

### Ghostty-Aligned Tick Boundary

Alan should preserve Ghostty's ownership split:

- Ghostty wakeup means there is terminal or app state to process.
- App mailbox drain and surface paint are different concerns.
- Visibility and focus decide render priority.

Introduce a window-level `TerminalRenderCoordinator` owned by the terminal
runtime registry or the primary shell owner. `AlanGhosttyLiveHost` registers a
pending wakeup with the coordinator instead of directly scheduling independent
main-queue tick and refresh work.

The coordinator drains pending hosts on the main actor in priority order:

1. `foregroundInteractive`
2. `visibleBackground`
3. `hiddenBackground`

Foreground hosts may tick and refresh immediately. Visible background hosts may
tick and refresh at frame cadence. Hidden background hosts may drain required
Ghostty app work at a bounded rate or on demand, but should not perform
high-frequency surface refresh while hidden.

The coordinator must force a catch-up pass when priority rises from hidden to
visible or foreground.

### SwiftUI Publication Throttling

Rendering is not the only pressure source. Background scrollback, title, cwd,
progress, and runtime snapshot updates can also cause shell root invalidation.

Move terminal update publication behind the same priority model:

- `foregroundInteractive`: publish immediately.
- `visibleBackground`: coalesce to at most once per frame.
- `hiddenBackground`: retain the latest state in the registry and publish only
  sidebar-relevant summaries, coalesced to a slower interval.

Hidden background summaries may include title, cwd, process exit, bell,
foreground-command state, and error state. Hidden background scrollback metrics
should not continuously publish into SwiftUI; they should synchronize when the
surface becomes visible.

### Observability

Add lightweight diagnostics behind a debug or logging switch:

- pending wakeups per second
- Ghostty app ticks per second
- surface refreshes per second
- coalesced refresh count
- visible and hidden surface counts
- coordinator drain latency buckets

This must stay out of the default UI. It is for debugging, tests, and local
performance comparison.

## Validation

Focused tests should cover:

- priority derivation for selected pane, visible split pane, hidden tab, hidden
  space, and zoom-hidden pane
- hidden surfaces receiving wakeups without one refresh per wakeup
- foreground surfaces taking priority over background drain work
- hidden-to-visible transition forcing an immediate catch-up tick and refresh
- throttled publication of hidden scrollback/runtime updates

Stress smoke should simulate many terminal handles producing frequent wakeups
and assert that hidden refresh count is bounded while foreground refresh remains
immediate.

Manual validation should run several background high-output commands, such as
server logs, build logs, or `yes`, then verify:

- background commands continue running
- sidebar, tab, and space switching stays responsive
- the focused terminal accepts input without noticeable queueing
- switching back to a hidden terminal immediately shows current output

## Risks

Skipping hidden refresh must not skip required Ghostty app mailbox work. The
implementation should preserve a path to drain app-level events independently
from visible painting.

Over-throttling metadata could hide important state such as process exit or
bell. Those events should remain summary-published for hidden panes.

The coordinator must avoid retaining stale hosts after pane teardown. Runtime
finalization should unregister pending hosts and clear queued work for that
surface.
