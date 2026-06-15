## 1. Off-main serial persistence writer

- [x] 1.1 Confirm `ShellContentWorkspaceManifest` and `ShellStateSnapshot` (and nested records) are `Sendable`; address any non-Sendable nested type.
- [x] 1.2 Add one persistence-writer seam covering both files (workspace manifest + control-plane shell-state file): encode + `Data.write(atomic:)` on a serial background queue, preserving corrupt-file quarantine behavior.
- [x] 1.3 Provide synchronous-write entries (structural mutations) and async-write entries (debounced content), both serialized on the one queue.
- [x] 1.4 Add a focused test proving the terminal metadata/runtime callback path performs no synchronous main-thread disk write of either file.

## 2. Split publishControlPlaneState + debounced flush

- [x] 2.1 Add an injectable scheduler/clock so the debounce + flush are deterministically testable (no wall-clock waits).
- [x] 2.2 Split `publishControlPlaneState` into a prompt in-memory publish (always synchronous) and disk persistence (debounced off-main on the hot path, synchronous on structural mutations).
- [x] 2.3 Route `updatePaneState`'s publish through the debounced (coalesced) path; keep all other (structural/lifecycle) callers synchronous.
- [x] 2.4 Debounced flush rebuilds manifest + shell-state on the main actor and dispatches both writes to the background queue; remove the `activeTaskChanged → sync write` trigger (do NOT change the `activeTask` field/model).
- [x] 2.5 Tests: a burst of terminal callbacks coalesces into one write of each file; a structural mutation writes synchronously; an active-task-only change triggers no synchronous write.

## 3. Lifecycle flush

- [x] 3.1 Force a synchronous content flush on `resignActive`/background and on app termination so a clean exit never loses pending transcript.
- [x] 3.2 Test the background/quit flush path persists the latest transcript snapshot.

## 4. Durability contract + test migration

- [x] 4.1 Migrate the durability tests in `clients/apple/scripts/test-shell-runtime-metadata.swift` that assert synchronous manifest content after `updateTerminalMetadata` to the new bounded-window/flush contract using the injectable scheduler.
- [x] 4.2 Run `just apple-shell-focused-tests` and confirm green.
- [x] 4.3 Build and run `clients/apple/scripts/capture-performance-diagnostics-workload.sh`; confirm the main-thread manifest write is gone and `terminalMetadataCallback` tail latency (max) drops materially.

## 5. Spec sync and archive

- [ ] 5.1 After implementation and verification, sync the accepted delta into `openspec/specs/macos-shell-workspace-persistence/spec.md` (IO requirements only; do not touch the semantics requirements owned by introduce-cross-platform-shell-core).
- [ ] 5.2 Archive the change only after implementation is merged and the long-lived spec is updated.
