## 1. Diagnostics Model And State

- [x] 1.1 Define the diagnostics state machine, preference storage, and
  disabled no-op boundary for all performance probes.
- [x] 1.2 Define versioned event, summary, stutter-marker, process-sample, and
  export metadata models without terminal-content, command-line, cwd, path,
  environment, or secret fields.
- [x] 1.3 Implement the bounded in-memory ring buffer and fixed-window summary
  aggregation with configurable retention limits.
- [x] 1.4 Add automatic stutter markers for threshold-crossing main-thread,
  terminal, and shell projection durations.

## 2. Settings And Export Surface

- [x] 2.1 Add the default-off `Performance Diagnostics` setting to the existing
  macOS Settings model and view structure.
- [x] 2.2 Add `Export Recent Diagnostics` as a compact Settings action that
  exports the currently retained local buffer without requiring manual
  start/stop or manual lag markers.
- [x] 2.3 Ensure disabling diagnostics stops capture and clears unexported
  in-memory buffers while leaving exported local bundles untouched.
- [x] 2.4 Keep diagnostics controls out of default shell chrome, sidebar,
  toolbar, terminal overlays, and inspector-like surfaces.

## 3. Terminal And Shell Probes

- [x] 3.1 Add terminal probes around Ghostty wakeup, app tick, surface refresh,
  surface attach, and catch-up refresh paths.
- [x] 3.2 Add runtime probes around terminal runtime snapshot publication,
  metadata callbacks, scrollback updates, renderer updates, priority changes,
  and visibility changes.
- [x] 3.3 Add shell probes around `ShellHostController.updateTerminalRuntime`,
  projection into shell state, pane-state publication, selection changes, focus
  changes, and priority synchronization.
- [x] 3.4 Verify probe calls are behavior-neutral and do not alter scheduling,
  rendering priority, focus, publication, or terminal process lifecycle.

## 4. Process Sampling

- [x] 4.1 Add low-frequency process sampling for Alan process CPU, memory, and
  thread count.
- [x] 4.2 Add known terminal child-process aggregate CPU sampling without
  command-line, argument, cwd, environment, or output capture.
- [x] 4.3 Represent unknown or partially attributed child CPU pressure
  explicitly in summary output.

## 5. Export Format And Privacy

- [x] 5.1 Write local diagnostics bundles with `events.jsonl`, `summary.json`,
  app/build metadata, install channel, schema version, sampling intervals, and
  capture window metadata.
- [x] 5.2 Exclude terminal text, prompt text, stdout/stderr content, command
  lines, cwd, repository names, file paths, environment variables, bearer
  tokens, API keys, refresh tokens, and raw provider/auth store values from all
  exported files.
- [x] 5.3 Decide whether exported pane/content identifiers are stable IDs or
  hashed IDs, and keep cross-event correlation possible within one bundle.

## 6. Verification

- [x] 6.1 Add focused tests proving diagnostics are off by default, record only
  when enabled, and stop recording when disabled.
- [x] 6.2 Add bounded-buffer tests proving old events are evicted and memory
  state does not grow without bound.
- [x] 6.3 Add export tests proving bundle files, schema metadata, summary
  windows, stutter markers, priority grouping, and process samples are present.
- [x] 6.4 Add privacy tests using terminal-output-like, command-like, cwd-like,
  path-like, environment-like, and secret-like fixture strings and verify they
  are absent from exported diagnostics.
- [x] 6.5 Add behavior-neutral tests comparing representative terminal runtime
  scenarios with diagnostics disabled and enabled.
- [x] 6.6 Run focused Apple shell/runtime diagnostics tests and the relevant
  shell contract scripts.
- [x] 6.7 Capture one real multi-Codex workload diagnostics bundle and record
  whether the summary distinguishes Alan main-thread long events, Ghostty
  tick/refresh spikes, shell projection spikes, runtime publication spikes, and
  child-process aggregate CPU pressure.

## 7. Review And Archive Readiness

- [x] 7.1 Run `openspec validate add-macos-performance-diagnostics --strict`.
- [x] 7.2 Run `openspec validate --all --strict`.
- [x] 7.3 Review exported diagnostics output manually for privacy and diagnostic
  usefulness before claiming the change is ready.
- [ ] 7.4 Before archiving after merge, sync accepted delta requirements into
  `openspec/specs/`.
- [ ] 7.5 Archive the completed change only after implementation, verification,
  PR review, and merge are complete.
