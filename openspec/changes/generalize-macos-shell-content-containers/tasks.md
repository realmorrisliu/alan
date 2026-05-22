## 1. Model And Migration

- [x] 1.1 Rebase this change after `persist-macos-shell-workspaces` is archived, and read the accepted `ShellWorkspaceManifest` schema/materializer before implementation.
- [x] 1.2 Introduce v0.2 shell state value types: `ShellPaneSlot`, `ShellContentInstance`, content kind, content capabilities, and content payloads for terminal, markdown, and settings surfaces.
- [x] 1.3 Change pane layout leaves to reference `pane_slot_id` and change shell focus state to use `focused_pane_slot_id`.
- [x] 1.4 Replace new-state `panes: [ShellPane]` persistence with `pane_slots` and `contents`; keep v0.1 `ShellPane` decoding only as diagnostics/compatibility input, not as workspace restore authority.
- [x] 1.5 Add one-time manifest migration that turns terminal-only workspace restore snapshots into PaneSlot plus terminal ContentInstance restore snapshots while preserving Space/Tab IDs, selection, pin state, TTL anchors, and active-task metadata.
- [x] 1.6 Update shell state projection helpers so focused space, focused tab, focused PaneSlot, attention, titles, and sidebar rows derive from content-aware descriptors.

## 2. Terminal Adapter

- [ ] 2.1 Migrate the terminal runtime registry and terminal host attachment boundary from pane-keyed identity to terminal `content_id` identity.
- [ ] 2.2 Wrap terminal rendering and runtime attachment behind a terminal ContentInstance adapter that resolves the current PaneSlot mount point.
- [ ] 2.3 Move terminal metadata projection for cwd, title, process status, alan binding, surface readiness, and attention into the terminal content adapter boundary.
- [ ] 2.4 Ensure close PaneSlot, close tab, lifecycle retirement, PaneSlot move, PaneSlot lift, content replacement, and app shutdown finalize or preserve terminal runtimes according to content lifecycle specs.
- [ ] 2.5 Replace `pane.send_text` surfaces with `terminal.send_text` while keeping PaneSlot as an optional convenience target that resolves to terminal ContentInstance before runtime delivery.
- [ ] 2.6 Preserve existing terminal input, search, paste, terminal text delivery, reattachment, and pending delivery behavior through focused content-keyed runtime tests.

## 3. Non-Terminal Content Surfaces

- [ ] 3.1 Add a content rendering registry or equivalent switch that routes terminal, markdown, and settings descriptors to bounded SwiftUI/AppKit host views.
- [ ] 3.2 Implement read-only markdown content opening with file-backed title, descriptor persistence, and viewer rendering.
- [ ] 3.3 Implement alan settings as shell tab content using the shared shell chrome rather than a separate page/window model.
- [ ] 3.4 Document browser content as a deferred follow-up area and ensure v0.2 model naming does not block adding a browser ContentInstance kind later.

## 4. Workspace Mutations And Control Plane

- [ ] 4.1 Update tab and split creation paths to accept content intent while keeping New Terminal Tab as the default behavior.
- [ ] 4.2 Update split, focus, resize, equalize, PaneSlot lift, PaneSlot move, close PaneSlot, and close tab mutations to operate on content-agnostic PaneSlots.
- [ ] 4.3 Extend shell control-plane DTOs and responses to expose `pane_slots`, `contents`, `pane_slot_id`, `content_id`, content capabilities, and content-aware mutation results.
- [ ] 4.4 Replace terminal text delivery with a terminal-specific command such as `terminal.send_text` that resolves PaneSlot targets to terminal ContentInstances before delivery.
- [ ] 4.5 Reject terminal-specific commands against non-terminal ContentInstances with stable unsupported-content errors and observable diagnostics.
- [ ] 4.6 Emit shell events for PaneSlot creation, PaneSlot closure, content creation, content closure, content replacement, and content-specific command rejection.
- [ ] 4.7 Ensure workspace manifest updates for pin/live snapshots write content-aware restore payloads and do not dual-write terminal-only snapshots.

## 5. UI Integration

- [ ] 5.1 Update `ShellWorkspaceView` / pane layout leaf rendering so mixed content panes share split geometry and focus treatment.
- [ ] 5.2 Update sidebar tab rows, toolbar titles, pane title bars, and command input labels to use user-facing content titles and type hints.
- [ ] 5.3 Keep terminal-only status, search, and input affordances visible only on terminal content panes.
- [ ] 5.4 Make settings content a singleton tab target in v1 so repeated Open Settings focuses the existing ContentInstance.
- [ ] 5.5 Capture running-app visual evidence for light-mode mixed content tabs and split panes.

## 6. Verification And Archive Readiness

- [ ] 6.1 Add focused shell model tests for v0.1-to-v0.2 migration, mixed content split mutation, and content-aware focus behavior.
- [ ] 6.2 Add terminal runtime tests proving runtime continuity is keyed by `content_id` across PaneSlot move, tab move, view reattachment, and new-runtime creation after manifest restore.
- [ ] 6.3 Add fake runtime service tests for `terminal.send_text`, `content_id` delivery, missing runtime errors, and queued delivery diagnostics.
- [ ] 6.4 Add control-plane tests for `pane_slots` / `contents` query, content-aware split creation, `terminal.send_text` success, and non-terminal terminal-command rejection.
- [ ] 6.5 Add workspace manifest migration and lifecycle tests for terminal-only pin snapshots, terminal-only live snapshots, mixed content pin/live snapshots, inactive unpinned Tab retirement finalization, and the rule that `shell-state-window_main.json` is not restore authority.
- [ ] 6.6 Run the focused Apple shell contract scripts affected by model, control-plane, workspace manifest, and terminal-runtime changes.
- [ ] 6.7 Run the macOS app build or document any local dependency blocker with the exact failing command.
- [ ] 6.8 Validate `generalize-macos-shell-content-containers` with `openspec validate generalize-macos-shell-content-containers --strict`.
- [ ] 6.9 Run `openspec validate --all --strict` after `persist-macos-shell-workspaces` is archived or while both active changes validate together.
- [ ] 6.10 After implementation is merged, sync accepted requirements into `openspec/specs/` and confirm the change is archive-ready.
