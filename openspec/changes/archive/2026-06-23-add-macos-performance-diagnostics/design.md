## Context

Alan for macOS now keeps terminal ContentInstance runtimes alive across spaces,
tabs, splits, zoom, and Quick Terminal Peak presentation. Recent background
rendering work added priority-aware rendering and publication so hidden panes
do less surface painting, but daily use with several Codex CLI sessions still
feels low frame-rate. The symptoms appear across foreground typing, tab/space
switching, Quick Terminal Peak, sidebar interaction, and hidden-to-visible
catch-up.

The next change should not guess at another optimization. The first slice needs
to produce evidence from real user workload, especially high-output Codex CLI
sessions, while preserving terminal-first behavior and avoiding content capture.

## Goals / Non-Goals

**Goals:**

- Add a default-off `Performance Diagnostics` control in macOS Settings that is
  available in every install channel.
- Collect bounded local traces while diagnostics are enabled.
- Attribute stutter across Alan main-thread work, embedded Ghostty terminal
  work, SwiftUI shell projection, and child-process CPU pressure.
- Export recent diagnostics as a local bundle for manual analysis.
- Preserve privacy by excluding terminal text, prompts, command lines, cwd,
  repository paths, file paths, environment variables, and secrets.
- Keep probes low overhead when enabled and near-zero overhead when disabled.

**Non-Goals:**

- Do not change terminal scheduling, rendering, focus, lazy rendering, or
  process lifecycle behavior in this diagnostic slice.
- Do not add automatic upload, telemetry, or remote diagnostics.
- Do not record stack traces continuously or build an Instruments replacement.
- Do not introduce a broad dashboard or persistent diagnostic chrome in the
  default shell.
- Do not claim performance improvement from this change alone.

## Decisions

### Diagnostics are opt-in product controls, not dev-only tooling

Settings will expose a single `Performance Diagnostics` toggle and an export
action for recent diagnostics. The feature is available in Dev and stable
channels because performance problems must be captured in the real environment
where they occur.

Alternative considered: restrict diagnostics to Alan Dev. That would reduce
surface area, but it would miss stable-channel and daily-workload evidence.

### Capture is continuous while enabled

When enabled, diagnostics maintain a bounded in-memory ring buffer and summary
windows for the recent past. Users do not need to press start, stop, or mark
stutter manually; threshold-crossing events create automatic stutter markers.

Alternative considered: explicit start/stop capture and manual stutter
markers. That is precise in a lab, but it is easy to miss the real lag window
during normal Codex work.

### Probe points are narrow and behavior-neutral

Instrumentation should sit around existing boundaries:

- Ghostty wakeup, app tick, surface refresh, attach, and catch-up.
- Terminal runtime snapshot publication and metadata callbacks.
- Surface scrollback / renderer state updates where those are already observed.
- Shell runtime projection, pane-state publication, selection, focus, and
  visible/hidden priority changes.
- Low-frequency process sampling for Alan process and known terminal child
  CPU/thread pressure.

The probes record event categories, durations, priority, visibility, pane or
content identity, and counters. They must not alter execution order or add
throttling.

Alternative considered: add richer instrumentation directly inside terminal IO
or PTY parsing first. That may become useful later, but it risks expanding the
first slice before we know whether the hot path is Ghostty refresh, SwiftUI
publication, or system CPU pressure.

### Data model uses events plus summaries

The recorder writes compact event records into memory and periodically updates
summary windows. Export writes:

- `events.jsonl` for recent event records.
- `summary.json` for aggregate counts, duration percentiles, stutter markers,
  process samples, priority grouping, and build metadata.
- Optional small metadata files for app version, install channel, capture
  window, and schema version.

The event schema should be stable enough for scripts, but first implementation
can remain local to the Apple client.

Alternative considered: summary-only diagnostics. Summary-only data is cheap,
but it makes it harder to align a stutter window with terminal refresh,
metadata publication, and shell projection spikes.

### Privacy boundaries are part of the contract

Diagnostics must not record terminal output, prompt text, command lines, cwd,
repo names, file paths, environment variables, secrets, or raw auth/provider
state. Process sampling records PIDs and numeric metrics, not command
arguments. Pane/content identifiers may use existing IDs while in memory; export
can hash them if needed to reduce workspace-structure leakage.

Alternative considered: record command names or current directories for better
human diagnosis. That would make traces more sensitive and is unnecessary for
the first question: which runtime path is responsible for frame drops.

### Diagnostics remain local and bounded

Recording uses an in-memory ring buffer with size and time-window limits.
Hot-path recording does not write files. Summary aggregation and process
sampling run at fixed low frequency. Closing the diagnostics toggle stops
sampling and clears unexported buffers; already exported local bundles remain
under user control.

Alternative considered: always-on persisted logs. That would make after-the-fact
analysis easier, but it increases privacy risk and could create a new
performance problem.

## Risks / Trade-offs

- Diagnostic overhead masks the original problem -> keep disabled probes as
  no-op, use bounded memory buffers, avoid hot-path file IO, and test overhead
  with diagnostics on and off.
- Trace cannot identify child-process CPU accurately -> record Alan process
  metrics separately from known terminal child aggregates and mark unknown
  attribution explicitly.
- Privacy regression through accidental text/path fields -> add focused export
  checks that scan fixtures and generated bundles for forbidden fields and
  assert command-line/cwd/path fields are absent.
- Summary is too coarse for manual attribution -> keep both event JSONL and
  aggregate summary, with automatic stutter markers for threshold crossings.
- Users expect the toggle to make Alan faster -> label it as diagnostics and
  keep the implementation behavior-neutral.

## Migration Plan

1. Add diagnostics state, recorder, schema, bounded ring buffer, summary
   aggregation, and export writer with tests.
2. Add Settings toggle and export action without changing default shell chrome.
3. Add terminal probes around Ghostty/runtime/surface paths.
4. Add shell probes around projection, pane-state publication, selection, and
   focus paths.
5. Add process sampling for Alan and known terminal child CPU/thread metrics.
6. Add focused privacy, bounded-buffer, toggle, export, and summary tests.
7. Use a real multi-Codex workload to capture a diagnostics bundle and decide
   the next optimization change from evidence.

Rollback is local to the Apple client: disable or remove the Settings toggle and
make the diagnostics controller permanently disabled. Because this slice must
not change scheduling or rendering behavior, rollback should not affect terminal
runtime semantics.

## Open Questions

- Exact ring-buffer limits and summary window length should be chosen during
  implementation with lightweight overhead checks.
- Export should initially use a save panel or the existing app-local data
  location; the implementation plan should choose the path that best matches
  current Settings patterns.
- Pane/content IDs may be exported as stable IDs or hashed IDs; the
  implementation should choose the least sensitive option that still supports
  cross-event correlation.
