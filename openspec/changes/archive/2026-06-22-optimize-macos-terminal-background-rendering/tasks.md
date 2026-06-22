## 1. Ownership And Inventory

- [x] 1.1 Inventory current Ghostty wakeup, tick, refresh, focus, occlusion, and publication paths in `GhosttyLiveHost`, `TerminalRuntimeService`, `TerminalSurfaceController`, `TerminalHostView`, and `ShellHostController`
- [x] 1.2 Record which paths are per-host today and which should move behind the window-scoped render coordinator
- [x] 1.3 Identify all terminal visibility inputs: selected pane, split visibility, tab selection, space selection, zoom state, window occlusion, and window close

## 2. Priority Model And Visibility Propagation

- [x] 2.1 Add a terminal runtime priority model for `foregroundInteractive`, `visibleBackground`, and `hiddenBackground`
- [x] 2.2 Derive priority from shell selection, pane visibility, split zoom, tab and space selection, and window occlusion without changing terminal runtime identity
- [x] 2.3 Propagate focus and occlusion state to embedded Ghostty surfaces when priority changes
- [x] 2.4 Add catch-up transition hooks when terminal content moves from hidden to visible or foreground

## 3. Window-Scoped Render Coordinator

- [x] 3.1 Introduce a window-scoped terminal render coordinator owned by the terminal runtime service or primary shell window owner
- [x] 3.2 Route `GhosttyLiveHost` wakeups through the coordinator instead of independent per-host main-queue tick and refresh scheduling
- [x] 3.3 Separate Ghostty app tick draining from surface refresh painting so hidden surfaces can process required state without repainting on every wakeup
- [x] 3.4 Drain pending work by priority order: foreground interactive, visible background, then hidden background
- [x] 3.5 Ensure coordinator teardown cannot resurrect closed terminal ContentInstance handles

## 4. Runtime Publication Throttling

- [x] 4.1 Keep latest terminal runtime state in the runtime service for every ContentInstance regardless of priority
- [x] 4.2 Publish foreground interactive state immediately for active input and visible controls
- [x] 4.3 Coalesce visible background publication to display cadence
- [x] 4.4 Throttle hidden background scrollback and renderer churn while preserving bounded publication for title, cwd, child exit, bell, attention, and failure summaries
- [x] 4.5 Force a current-state publication when hidden terminal content becomes visible

## 5. Focused Verification

- [x] 5.1 Add focused tests for priority derivation across selected panes, visible splits, hidden tabs, hidden spaces, split zoom, and window occlusion
- [x] 5.2 Add fake-runtime tests proving hidden wakeups are coalesced instead of causing one immediate surface refresh per wakeup
- [x] 5.3 Add tests proving foreground interactive work drains before visible and hidden background work
- [x] 5.4 Add tests proving hidden-to-visible catch-up refreshes the existing terminal ContentInstance handle without restart or replacement
- [x] 5.5 Add tests or instrumentation checks proving hidden publication is throttled while sidebar-relevant summaries remain observable

## 6. Stress And App Verification

- [x] 6.1 Add debug-only counters for pending wakeups, app ticks, surface refreshes, coalesced refreshes, priority counts, and coordinator drain latency
- [x] 6.2 Run a high-output background terminal smoke with foreground typing and focus changes
- [x] 6.3 Run a multi-pane high-output smoke and record coalescing and drain-latency evidence
- [x] 6.4 Build and install the Apple client with the project-supported command before visual verification
- [x] 6.5 Relaunch the installed Alan app and verify foreground responsiveness plus hidden-terminal catch-up in the running app

## 7. Review And Archive Readiness

- [x] 7.1 Run focused Apple client tests covering the new scheduler and publication paths
- [x] 7.2 Run `openspec validate optimize-macos-terminal-background-rendering --strict`
- [x] 7.3 Run `openspec validate --all --strict`
- [x] 7.4 Prepare PR notes summarizing the scheduling contract, Ghostty alignment, and stress evidence
- [ ] 7.5 After merge, sync accepted delta requirements into `openspec/specs/` before archiving this change
