# Implementation Readiness

## Current Status

- OpenSpec progress: `28/30` tasks complete.
- Implementation-side work is complete for the diagnostic slice.
- Remaining tasks are post-merge/archive tasks:
  - `7.4` sync accepted delta requirements into `openspec/specs/` before archive.
  - `7.5` archive after implementation, verification, PR review, and merge.

Do not mark those archive tasks complete before the change is reviewed and
merged.

## Implemented Surfaces

- Settings exposes a default-off `Performance Diagnostics` control and compact
  export action.
- Diagnostics recording is bounded, opt-in, local-only, and clears unexported
  buffers when disabled.
- Hot-path probes check the diagnostics enabled state before constructing
  retained events or starting diagnostic-only timers, keeping disabled
  diagnostics on the no-op path.
- Export writes `events.jsonl` and `summary.json` with schema/build/capture
  metadata.
- `events.jsonl` uses one compact JSON object per retained event line, and
  `summary.json` exposes `countsByKind` as event-kind string keys for scripts.
- `summary.json` includes retained-window duration stats by event kind and
  priority / visibility grouping for direct attribution without reprocessing
  raw events first.
- Exported pane/content identifiers are hashed for correlation without raw
  workspace IDs.
- Probe points cover Ghostty wakeup/tick/refresh, terminal runtime publication,
  metadata callbacks, renderer updates, scrollback, shell projection, pane-state
  publication, selection/focus changes, visibility, and priority changes.
- Process sampling records Alan app CPU/thread/memory and aggregate Alan
  descendant CPU pressure as terminal child pressure without command lines, cwd,
  environment, or terminal text.
- Local control commands support diagnostics enable/export/child-pressure
  capture for repeatable verification.
- `terminal.send_key` sends an explicit Return key path for terminal control
  automation; workload scripts do not rely on newline text to submit commands.

## Real Workload Evidence

See `real-workload-diagnostics.md` for the exported bundle review.

Observed in the real workload bundle:

- `135` `shellRuntimeProjection` events.
- `269` `runtimeSnapshotPublish` events.
- `910` Ghostty timing events across wakeup/tick/refresh.
- `3` `terminalChild` aggregate CPU samples.
- `stutterMarkerCount` was `0`, proving no threshold-crossing marker was emitted
  in that capture while raw event families and process pressure remained
  distinguishable.

Privacy review found no terminal workload text, repo path, command-line fields,
working-directory fields, environment fields, `OPENAI_API_KEY`, or
`refresh-token` in exported diagnostics.

## Verification Run

Validated without operating the user's running `Alan Dev` instance:

- `bash -n clients/apple/scripts/capture-performance-diagnostics-workload.sh`
- `bash clients/apple/scripts/test-shell-performance-diagnostics.sh`
- `bash clients/apple/scripts/test-terminal-runtime-service.sh`
- `bash clients/apple/scripts/test-terminal-surface-controller.sh`
- `bash clients/apple/scripts/test-shell-automation-command-seams.sh`
- `bash clients/apple/scripts/test-shell-runtime-metadata.sh`
- `bash clients/apple/scripts/test-shell-settings-surface.sh`
- `bash clients/apple/scripts/check-shell-contracts.sh`
- `just apple-shell-focused-tests`
- `xcodebuild -project clients/apple/alan-macos.xcodeproj -scheme alan-macos -configuration Debug -destination generic/platform=macOS -derivedDataPath /private/tmp/alan-xcode-derived-perfdiag-rebased -clonedSourcePackagesDirPath /private/tmp/alan-xcode-spm-perfdiag-rebased -skipPackagePluginValidation -skipMacroValidation build`
- `openspec validate add-macos-performance-diagnostics --strict`
- `openspec validate --all --strict`
- `git diff --check`

The isolated worktree used ignored Ghostty artifact symlinks pointing at the
existing local Ghostty cache; no Alan Dev process was launched or controlled for
these verification commands.

## Operational Notes

- `capture-performance-diagnostics-workload.sh` defaults to LaunchServices
  `open` mode because direct executable launch did not reliably initialize the
  shell control plane.
- When `ALAN_PERF_DIAG_SKIP_BUILD=1` is used, the capture script now defaults to
  the repo UI-smoke DerivedData `Alan.app` instead of the user's installed
  `Alan Dev.app`.
- The capture script still accepts explicit `ALAN_UI_SMOKE_APP_PATH` and
  `ALAN_UI_SMOKE_APP_EXECUTABLE` overrides for controlled validation.
