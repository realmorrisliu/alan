## 1. Ownership And Inventory

- [ ] 1.1 Inventory current Ghostty wakeup, tick, refresh, focus, occlusion, and publication paths in `GhosttyLiveHost`, `TerminalRuntimeService`, `TerminalSurfaceController`, `TerminalHostView`, and `ShellHostController`
- [ ] 1.2 Record which paths are per-host today and which should move behind the window-scoped render coordinator
- [ ] 1.3 Identify all terminal visibility inputs: selected pane, split visibility, tab selection, space selection, zoom state, window occlusion, and window close

## 2. Priority Model And Visibility Propagation

- [ ] 2.1 Add a terminal runtime priority model for `foregroundInteractive`, `visibleBackground`, and `hiddenBackground`
- [ ] 2.2 Derive priority from shell selection, pane visibility, split zoom, tab and space selection, and window occlusion without changing terminal runtime identity
- [ ] 2.3 Propagate focus and occlusion state to embedded Ghostty surfaces when priority changes
- [ ] 2.4 Add catch-up transition hooks when terminal content moves from hidden to visible or foreground

## 3. Window-Scoped Render Coordinator

- [ ] 3.1 Introduce a window-scoped terminal render coordinator owned by the terminal runtime service or primary shell window owner
- [ ] 3.2 Route `GhosttyLiveHost` wakeups through the coordinator instead of independent per-host main-queue tick and refresh scheduling
- [ ] 3.3 Separate Ghostty app tick draining from surface refresh painting so hidden surfaces can process required state without repainting on every wakeup
- [ ] 3.4 Drain pending work by priority order: foreground interactive, visible background, then hidden background
- [ ] 3.5 Ensure coordinator teardown cannot resurrect closed terminal ContentInstance handles

## 4. Runtime Publication Throttling

- [ ] 4.1 Keep latest terminal runtime state in the runtime service for every ContentInstance regardless of priority
- [ ] 4.2 Publish foreground interactive state immediately for active input and visible controls
- [ ] 4.3 Coalesce visible background publication to display cadence
- [ ] 4.4 Throttle hidden background scrollback and renderer churn while preserving bounded publication for title, cwd, child exit, bell, attention, and failure summaries
- [ ] 4.5 Force a current-state publication when hidden terminal content becomes visible

## 5. Focused Verification

- [ ] 5.1 Add focused tests for priority derivation across selected panes, visible splits, hidden tabs, hidden spaces, split zoom, and window occlusion
- [ ] 5.2 Add fake-runtime tests proving hidden wakeups are coalesced instead of causing one immediate surface refresh per wakeup
- [ ] 5.3 Add tests proving foreground interactive work drains before visible and hidden background work
- [ ] 5.4 Add tests proving hidden-to-visible catch-up refreshes the existing terminal ContentInstance handle without restart or replacement
- [ ] 5.5 Add tests or instrumentation checks proving hidden publication is throttled while sidebar-relevant summaries remain observable

## 6. Stress And App Verification

- [ ] 6.1 Add debug-only counters for pending wakeups, app ticks, surface refreshes, coalesced refreshes, priority counts, and coordinator drain latency
- [ ] 6.2 Run a high-output background terminal smoke with foreground typing and focus changes
- [ ] 6.3 Run a multi-pane high-output smoke and record coalescing and drain-latency evidence
- [ ] 6.4 Build and install the Apple client with the project-supported command before visual verification
- [ ] 6.5 Relaunch the installed Alan app and verify foreground responsiveness plus hidden-terminal catch-up in the running app

## 7. Review And Archive Readiness

- [ ] 7.1 Run focused Apple client tests covering the new scheduler and publication paths
- [ ] 7.2 Run `openspec validate optimize-macos-terminal-background-rendering --strict`
- [ ] 7.3 Run `openspec validate --all --strict`
- [ ] 7.4 Prepare PR notes summarizing the scheduling contract, Ghostty alignment, and stress evidence
- [ ] 7.5 After merge, sync accepted delta requirements into `openspec/specs/` before archiving this change
