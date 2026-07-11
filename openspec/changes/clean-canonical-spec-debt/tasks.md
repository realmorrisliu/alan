## 1. Canonical Purpose Cleanup

- [x] 1.1 Confirm the canonical tree contains exactly the 23 known generated Purpose placeholders and record any newly discovered placeholder before editing.
- [x] 1.2 Replace placeholder Purpose text for `agent-file-layout-contract`, `agent-mount-escalation`, `agent-namespace-runtime`, `ap-wire-transport`, `host-directory-mounts`, `live-mount-grant-namespace-projection`, `mount-grant-tool-sandbox-projection`, `os-sandbox-enforcement`, `plan9-kernel-substrate`, and `sandbox-autonomy-invariants` without changing requirement bodies.
- [x] 1.3 Replace placeholder Purpose text for `alan-shell`, `editable-buffer-file-server`, `macos-privileged-helper`, `macos-shell-action-registry`, `shell-core-authority-contract`, `shell-workspace-core-contract`, and `tool-result-presentation` without changing requirement bodies.
- [x] 1.4 Replace placeholder Purpose text for `auto-approve-policy`, `autonomous-review-mode`, `branching-execution-file-server`, `delegation-capability-alignment`, `llm-file-server`, and `message-routing` without changing requirement bodies.
- [x] 1.5 Review every new Purpose against its existing requirement headings and confirm the cleanup introduces no new normative behavior.

## 2. OpenSpec Configuration

- [x] 2.1 Move merge, canonical-spec sync, archive-readiness, and dated archive-path guidance into the supported `rules.tasks` list in `openspec/config.yaml`.
- [x] 2.2 Delete the unsupported `rules.archive` key and confirm proposal, design, specs, and tasks instruction lookup emits no unknown-artifact warning.

## 3. Active Change Repair

- [x] 3.1 Remove deleted `Views/Console/` and retired remote-control Console references from `add-macos-shell-component-system`, then recalculate its current source/baseline scope.
- [x] 3.2 Rewrite `define-groove-master-alan-app` proposal, design, specs, tasks, and implementation plan so the service tree and direct Alan for macOS file client are entry criteria and no host/ContentInstance compatibility bridge remains.
- [x] 3.3 Rewrite `define-updf-product-umbrella` so macOS preview work waits for direct file attachment and no `UPDFPreviewHostCompatibilityBridge` remains.
- [x] 3.4 Rewrite `add-alan-voice-mvp` so host capture integrates through the Voice Service file tree and no `AlanVoiceHostCompatibilityBridge` remains.
- [x] 3.5 Rewrite `define-alan-programmable-client-surface` so package command exposure waits for the canonical package/binfs mount and no namespace-bootstrap compatibility projection remains.
- [x] 3.6 Search all non-archived changes for callback, DTO, ContentInstance, host-action, named compatibility-bridge, and namespace-bootstrap authorization; resolve each current match or add a narrow explanatory allowlist.

## 4. Current-Surface Guard

- [x] 4.1 Add a focused repository check for generated Purpose placeholders, unsupported OpenSpec rule keys/instruction warnings, deleted source references, and new bridge authorization outside immutable archive history.
- [x] 4.2 Add negative fixtures proving each forbidden class fails with the owning file and rule, plus positive fixtures proving legitimate adapters, projections, historical archives, and this bounded cleanup change remain allowed.
- [x] 4.3 Wire the check into the normal documentation/OpenSpec quality gate without scanning generated build output.

## 5. Verification And Delivery

- [x] 5.1 Run the focused current-surface guard, `openspec validate --all --strict`, `git diff --check`, and any repository documentation validation affected by the edits.
- [x] 5.2 Review the final diff to confirm `openspec/changes/archive/` is byte-for-byte untouched and no production runtime code changed.
- [ ] 5.3 Open a narrowly scoped PR and keep the current HEAD under Codex review until all review threads are resolved, required CI is green, and a delayed refresh shows no new findings before merge.
- [ ] 5.4 After merge, sync these delta requirements into canonical specs, verify the two middle cleanup changes can start from main, and mark this change archive-ready only when the merged state and canonical specs agree.
