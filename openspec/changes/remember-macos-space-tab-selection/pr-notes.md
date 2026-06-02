## Summary

- Adds Space-local remembered tab selection to shell state, content state, and workspace manifests via optional `selected_tab_id` fields.
- Updates Space switching to restore the selected Space's remembered tab before falling back to current focus or the first tab.
- Repairs remembered selections when tabs are created, focused, moved, closed, pruned, projected, materialized, merged from published state, and persisted.

## Compatibility

- Old manifests and shell snapshots without Space-local `selected_tab_id` still decode.
- Manifest repair seeds the globally selected Space from legacy global `selected_tab_id`, then falls back to the first retained tab or nil for empty Spaces.
- Published state merging preserves authoritative Space-local selection when an incoming older snapshot omits the new field.

## Verification

- `bash clients/apple/scripts/test-shell-workspace-manifest.sh`
- `bash clients/apple/scripts/test-shell-tab-organization.sh`
- `bash clients/apple/scripts/test-shell-runtime-metadata.sh`
- `bash clients/apple/scripts/test-terminal-surface-controller.sh`
- `bash clients/apple/scripts/test-shell-split-model.sh`
- `bash clients/apple/scripts/check-shell-contracts.sh`
- `openspec validate remember-macos-space-tab-selection --strict`
- `git diff --check`
- `xcodebuild -project clients/apple/alan-macos.xcodeproj -scheme alan-macos -configuration Debug -derivedDataPath debug/DerivedData/space-tab-selection build`
