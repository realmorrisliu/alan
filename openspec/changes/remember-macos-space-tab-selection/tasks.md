## 1. Manifest And State Model

- [x] 1.1 Add optional per-Space selected Tab fields to terminal-only and content-container workspace manifest Space records.
- [x] 1.2 Add old-manifest decode tests proving missing per-Space selected Tab fields are accepted.
- [x] 1.3 Add materializer coverage for seeding the globally selected Space's remembered Tab from legacy global selected Tab state.
- [x] 1.4 Add runtime Space projection support for Space-local remembered Tab selection where controller or control-plane state needs it.

## 2. Space Selection Resolution

- [x] 2.1 Add a focused failing test showing Space A restores its second Tab after switching to Space B and back.
- [x] 2.2 Update `ShellHostController.select(spaceID:)` to resolve target Tab/PaneSlot from the selected Space's remembered selection before falling back to the first Tab.
- [x] 2.3 Ensure sidebar clicks, keyboard Space switching, swipe commit, action registry routing, and control-plane Space selection use the shared Space selection path.
- [x] 2.4 Preserve empty Space selection with nil Tab and nil PaneSlot focus.

## 3. Remembered Selection Maintenance

- [x] 3.1 Update accepted Tab and PaneSlot focus paths to record the owning Space's remembered selected Tab.
- [x] 3.2 Update new Tab creation so the target Space remembers the newly focused Tab.
- [x] 3.3 Update Tab close and lifecycle pruning repair so removed remembered Tabs fall back to the first retained Tab or nil for empty Spaces.
- [x] 3.4 Update Move to Space behavior so source and destination remembered selection outcomes match current focus semantics.
- [x] 3.5 Ensure context menu target resolution still does not mutate remembered Space selection before action execution.

## 4. Persistence Writeback

- [x] 4.1 Persist each Space's remembered selected Tab into the workspace manifest after selection, reorder, pin/unpin, close, move, and pruning mutations.
- [x] 4.2 Keep global selected Space/Tab fields aligned with the currently focused Space/Tab for restart focus.
- [x] 4.3 Add manifest round-trip tests for multiple Spaces with different remembered selected Tabs.
- [x] 4.4 Add restart restore tests proving inactive Space selections are retained and used after switching Spaces.

## 5. Verification

- [x] 5.1 Run `bash clients/apple/scripts/test-shell-workspace-manifest.sh`.
- [x] 5.2 Run `bash clients/apple/scripts/test-shell-tab-organization.sh`.
- [x] 5.3 Run `bash clients/apple/scripts/test-shell-runtime-metadata.sh`.
- [x] 5.4 Run `bash clients/apple/scripts/test-terminal-surface-controller.sh`.
- [x] 5.5 Run `bash clients/apple/scripts/check-shell-contracts.sh`.
- [x] 5.6 Run `xcodebuild -project clients/apple/alan-macos.xcodeproj -scheme alan-macos -configuration Debug -derivedDataPath debug/DerivedData/space-tab-selection build`.
- [x] 5.7 Run `openspec validate remember-macos-space-tab-selection --strict`.

## 6. Review And Archive Readiness

- [x] 6.1 Prepare PR notes describing the per-Space selection model, old-manifest compatibility, and tested Space-switching paths.
- [ ] 6.2 After implementation is merged, sync accepted spec deltas into `openspec/specs/` before archiving the change.
