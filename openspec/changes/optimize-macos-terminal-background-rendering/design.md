## Context

The macOS shell already keeps terminal ContentInstance runtimes alive across
tab, space, split, zoom, and SwiftUI/AppKit view reconstruction. That is the
right lifecycle boundary for a terminal-first workspace, but it also means
background panes can continue producing terminal output while they are not the
user's active surface.

Today the embedded Ghostty host can schedule tick and refresh work per live
host. High-output background panes can therefore compete with the selected
terminal for main-thread time, surface refreshes, metadata projection, and
SwiftUI invalidation. Ghostty's own architecture separates terminal IO, renderer
work, focus, and occlusion more deliberately: surfaces can keep processing state
while invisible or unfocused renderers avoid unnecessary drawing.

This change keeps Alan's real-time background terminal model while making
rendering and UI publication priority-aware.

## Goals / Non-Goals

**Goals:**

- Keep terminal processes, PTYs, scrollback, and terminal state running in real
  time for background panes.
- Preserve foreground typing responsiveness when other panes produce heavy
  output.
- Coalesce embedded Ghostty tick and refresh work at a window level rather than
  scheduling unbounded per-host main-queue work.
- Treat foreground, visible background, and hidden background terminal surfaces
  differently for paint and SwiftUI publication.
- Force a catch-up pass when a hidden terminal becomes visible so the user does
  not see stale content.
- Add focused verification for priority transitions, hidden-output catch-up,
  refresh coalescing, and foreground responsiveness.

**Non-Goals:**

- Do not suspend background shell processes.
- Do not drop terminal output or make background PTY IO best-effort.
- Do not change the runtime identity model from terminal ContentInstance IDs.
- Do not introduce a new user-facing preference surface before the default
  scheduling policy is proven.
- Do not replace Ghostty's renderer or terminal state ownership.

## Decisions

### Runtime priority is a render and publication policy

Alan will classify terminal content as `foregroundInteractive`,
`visibleBackground`, or `hiddenBackground`. The priority controls surface focus,
occlusion, refresh cadence, metadata publication, and SwiftUI invalidation. It
does not control whether the underlying child process, PTY, scrollback, or
terminal state keeps running.

Alternative considered: pause or back-pressure hidden PTY reads. That would
reduce CPU in extreme cases but violates the terminal lifecycle contract and can
break long-running commands that depend on timely terminal consumption.

### Ghostty wakeup processing is separated from surface painting

A Ghostty wakeup means Alan may need to drain app-level work or process terminal
state. It does not imply that every surface should immediately paint. The
coordinator will allow bounded app tick processing while making surface refresh
decisions by terminal priority and visibility.

Alternative considered: throttle the existing per-host `scheduleTick` calls.
That is less invasive but keeps scheduling ownership spread across every live
host, making it harder to guarantee foreground priority or hidden-surface
catch-up.

### Render coordination is window scoped

Each shell window owns a terminal render coordinator associated with that
window's runtime service. Live hosts register pending wakeups with the
coordinator. The coordinator drains work on the main actor in priority order:
foreground interactive first, visible background next, hidden background last
and only for bounded state processing or explicit catch-up.

Alternative considered: use a single process-wide coordinator. That could reduce
total wakeups further, but it would mix independent window lifecycles and make
window teardown, focus, and occlusion propagation harder to reason about.

### SwiftUI publication is priority aware

Foreground terminal runtime state is published immediately. Visible background
runtime state is coalesced to the display cadence. Hidden background runtime
state keeps the latest value in the runtime service, but only sidebar-relevant
summaries such as title, cwd, child exit, bell, attention, or failure should
invalidate shell UI at a slower cadence. Continuous scrollback metrics and
renderer phase churn are deferred until the content becomes visible.

Alternative considered: keep all metadata publication immediate and only
coalesce Ghostty refreshes. That would leave SwiftUI root invalidation as a
separate source of jank when many hidden panes are active.

### Visibility transitions force catch-up

When a terminal changes from hidden to visible, or from visible background to
foreground interactive, Alan will run a catch-up tick and refresh before or
during first presentation. The user should see current terminal state without a
restart, scrollback loss, or stale first frame.

Alternative considered: rely on the next natural Ghostty wakeup. That keeps the
scheduler simpler but can show stale content after tab or space switches.

### Observability stays debug-only

The implementation will add debug-only counters for pending wakeups, app ticks,
surface refreshes, coalesced refreshes, priority counts, and coordinator drain
latency. These metrics are for validation and diagnosis, not default UI chrome.

Alternative considered: expose scheduler state in the primary shell UI. That
would conflict with the product direction that diagnostics remain progressively
disclosed.

## Risks / Trade-offs

- Hidden catch-up can still be expensive after very large output bursts. The
  mitigation is to bound per-drain work, keep foreground priority first, and add
  stress verification for high-output panes.
- Incorrect priority derivation could hide important terminal state. The
  mitigation is focused tests for tab switches, split zoom, pane movement,
  window occlusion, and hidden-to-visible transitions.
- Separating tick from refresh can miss required app mailbox side effects if the
  boundary is too aggressive. The mitigation is to keep close/error/process
  events drainable even when surface painting is deferred.
- Throttled publication can make sidebar metadata appear slightly delayed for
  hidden panes. The mitigation is to keep attention, exit, bell, failure, title,
  and cwd summaries on a slower but bounded publication path.

## Migration Plan

1. Add priority derivation and visibility propagation without changing process
   lifetime or delivery semantics.
2. Introduce the window-scoped render coordinator behind the existing host
   boundary and route host wakeups through it.
3. Split Ghostty app tick and surface refresh scheduling so hidden surfaces do
   not paint on every wakeup.
4. Add SwiftUI publication throttling for hidden runtime state while retaining
   latest service state.
5. Add focused tests and a documented high-output stress smoke.
6. Remove any obsolete per-host scheduling paths once the coordinator owns all
   live host wakeups.

Rollback is local to the Apple client: the coordinator can be bypassed by
restoring direct host scheduling while preserving the priority derivation types.

## Open Questions

No blocking OpenSpec questions. Exact cadence constants should be chosen during
implementation with focused tests and stress evidence.
