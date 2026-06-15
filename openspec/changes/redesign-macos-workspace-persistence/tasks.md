## 1. Off-main serial writer

- [ ] 1.1 Confirm `ShellContentWorkspaceManifest` (and nested records) are `Sendable`; address any non-Sendable nested type.
- [ ] 1.2 Add a serial background `DispatchQueue` writer behind `ShellWorkspaceManifestStore`: encode + `Data.write(atomic:)` off the main thread, preserving corrupt-file quarantine behavior.
- [ ] 1.3 Provide a synchronous-write entry (for structural mutations) and an async-write entry (for content), both serialized on the one queue.
- [ ] 1.4 Add a focused test proving the terminal metadata/runtime callback path performs no synchronous main-thread disk write.

## 2. Cadence split + debounced content flush

- [ ] 2.1 Add an injectable scheduler/clock so the debounce + flush are deterministically testable (no wall-clock waits).
- [ ] 2.2 Mark restore content dirty on transcript change; schedule a single debounced flush that rebuilds the manifest on the main actor and dispatches the write to the background queue.
- [ ] 2.3 Keep structural mutations (create/close/reorder/pin/move/selection) persisting synchronously through the synchronous-write entry.
- [ ] 2.4 Remove the `activeTaskChanged → syncWorkspaceManifestFromShellState` write trigger on the hot path (do NOT change the `activeTask` field/model).
- [ ] 2.5 Tests: a burst of content changes coalesces into one write; a structural mutation writes synchronously; an active-task change alone triggers no write.

## 3. Lifecycle flush

- [ ] 3.1 Force a synchronous content flush on `resignActive`/background and on app termination so a clean exit never loses pending transcript.
- [ ] 3.2 Test the background/quit flush path persists the latest transcript snapshot.

## 4. Durability contract + test migration

- [ ] 4.1 Migrate the durability tests in `clients/apple/scripts/test-shell-runtime-metadata.swift` that assert synchronous manifest content after `updateTerminalMetadata` to the new bounded-window/flush contract using the injectable scheduler.
- [ ] 4.2 Run `just apple-shell-focused-tests` and confirm green.
- [ ] 4.3 Build and run `clients/apple/scripts/capture-performance-diagnostics-workload.sh`; confirm the main-thread manifest write is gone and `terminalMetadataCallback` tail latency (max) drops materially.

## 5. Spec sync and archive

- [ ] 5.1 After implementation and verification, sync the accepted delta into `openspec/specs/macos-shell-workspace-persistence/spec.md` (IO requirements only; do not touch the semantics requirements owned by introduce-cross-platform-shell-core).
- [ ] 5.2 Archive the change only after implementation is merged and the long-lived spec is updated.
