# Real Workload Diagnostics Evidence

## Capture

- Run ID: `20260601-195137`
- Launch path: controlled UI-smoke Debug app with dev install-channel metadata.
- Bundle:
  `/Users/morris/Developer/Alan/debug/artifacts/performance-diagnostics-workload/20260601-195137/export/alan-performance-diagnostics-1780314760886`
- Exported files:
  - `events.jsonl`
  - `summary.json`
- Workload shape: four terminal panes received programmatic workload text and an
  explicit `terminal.send_key` return key. The workload created four pid files
  under the run artifact and emitted terminal-output-like high-volume text.

The script reached diagnostics export and produced a valid bundle. A later
review-summary counting step initially exited under `pipefail` when
`automaticStutterMarker` had zero matches; the counting helper now treats zero
matches as `0`.

## Diagnostic Usefulness Review

The summary and retained events distinguish the relevant sources:

| Signal | Evidence |
| --- | --- |
| Alan app CPU pressure | `summary.json` includes `alanApp` process samples, with observed CPU up to `125.6`. |
| Main-thread/event duration | `events.jsonl` includes per-event `thread` and `duration_ms`; max observed `terminalRendererUpdate` duration was `50.659792 ms`. |
| Ghostty tick/refresh/wakeup | `events.jsonl` contains `910` Ghostty timing events across `ghosttyWakeup`, `ghosttyAppTick`, and `ghosttySurfaceRefresh`; max `ghosttyWakeup` duration was `19.837958 ms`. |
| Shell projection | `events.jsonl` contains `135` `shellRuntimeProjection` events; max observed duration was `8.521834 ms`. |
| Runtime publication | `events.jsonl` contains `269` `runtimeSnapshotPublish` events; max observed duration was `0.915959 ms`. |
| Child-process aggregate CPU | `summary.json` contains `3` `terminalChild` aggregate samples, with no command line, cwd, environment, or terminal text. |

The run did not cross the configured automatic stutter thresholds:
`stutterMarkerCount` is `0`. That is still diagnostically useful because the
bundle separates raw event families and child CPU pressure, so a no-stutter run
can be distinguished from missing instrumentation.

## Privacy Review

Manual review and grep checks were run against `events.jsonl` and `summary.json`.
The exported diagnostics did not contain:

- terminal workload marker text (`alan-perf-diag`)
- the repository path used for the capture
- command-line fields
- working-directory fields
- environment fields
- `OPENAI_API_KEY`
- `refresh-token`

Pane/content correlation is preserved with hashed IDs such as `pane_id_hash` and
`content_id_hash`; raw pane/content IDs are not required in the exported bundle.
