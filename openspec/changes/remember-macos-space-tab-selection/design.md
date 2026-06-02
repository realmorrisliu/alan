## Context

Alan's macOS shell currently has one global focused Space, Tab, and PaneSlot in
`ShellStateSnapshot`. `ShellHostController.select(spaceID:)` resolves a target
PaneSlot by checking whether the current global focused PaneSlot belongs to the
target Space; when it does not, it falls back to the first Tab in that Space.
That explains the observed behavior: returning to a Space loses the Tab the
user had selected there because the state model does not retain Space-local Tab
selection.

The workspace manifest has the same shape: it persists `selected_space_id` and
`selected_tab_id` globally, but each Space record stores only ordering and Tabs.
Restart can restore the active Space and active Tab, but it cannot remember the
selected Tab for inactive Spaces.

## Goals / Non-Goals

**Goals:**

- Remember each Space's last selected Tab during normal shell interaction.
- Restore the remembered Tab and preferred PaneSlot when a Space becomes active.
- Persist Space-local selected Tab state in the workspace manifest.
- Keep old manifests decodable and repair missing or invalid per-Space selection.
- Preserve empty Space behavior without fabricating Tab or PaneSlot focus.
- Preserve terminal ContentInstance and runtime identity across selection changes.

**Non-Goals:**

- Do not add a new visual settings surface or user preference for this behavior.
- Do not change Tab order, pin/unpin behavior, split tree semantics, or terminal
  runtime lifecycle beyond the selection target.
- Do not require daemon-side or Rust API changes unless an existing shell
  control response already exposes the affected focused IDs.
- Do not introduce cross-window Space selection sharing.

## Decisions

### Space records own remembered Tab selection

Add optional Space-local selected Tab fields to the macOS shell Space models
that need to round-trip selection:

- `ShellWorkspaceSpaceRecord.selectedTabID`
- `ShellContentWorkspaceSpaceRecord.selectedTabID`
- the runtime Space projection used by `ShellStateSnapshot` / content-state
  projection, if needed for controller and control-plane consistency

The existing global `focusedSpaceID` / `focusedTabID` fields remain the
authoritative active selection for the currently focused Space. Space-local
selection is a remembered target used when a Space becomes active again.

Alternative considered: keep an in-memory
`lastSelectedTabIDBySpaceID` dictionary only in `ShellHostController`. That
would fix same-process switching, but restart would still lose inactive Space
selection and manifest restore would remain unable to express the user's
workspace shape.

### Space switching resolves from remembered selection first

When selecting a Space, the controller resolves the target in this order:

1. remembered selected Tab for the target Space, if it still exists;
2. that remembered Tab's last focused PaneSlot, if represented and still valid;
3. the first valid PaneSlot in the remembered Tab;
4. the first Tab/PaneSlot in the target Space;
5. no Tab/PaneSlot focus for an empty Space.

Once the target resolves, the normal focus path updates the global focused Space,
Tab, PaneSlot, terminal render priorities, control-plane publication, and
terminal focus request.

Alternative considered: keep choosing the first Tab but update sidebar selection
state separately. That would leave terminal focus, render priority, and control
plane state inconsistent with what the UI shows.

### Tab focus updates the owning Space's remembered selection

Any accepted focus path that selects a Tab or PaneSlot updates the owning Space's
remembered Tab. This includes direct sidebar Tab clicks, keyboard Tab switching,
pane focus, command routing, automation focus commands, opening a new Tab, and
moving the current Tab into another Space. Context-menu target resolution must
continue to avoid changing selection until the action is executed.

Alternative considered: update remembered selection only on explicit Tab row
clicks. That would miss keyboard, split focus, control-plane, and command paths,
which are all first-class shell interactions.

### Selection repair is centralized

Manifest repair and runtime state repair should share the same rules:

- if a remembered Tab still exists in its Space, keep it;
- if it was removed, retired, or moved away, choose the first remaining Tab in
  that Space;
- if the Space has no Tabs, clear that Space's remembered Tab;
- if a Tab moves to another Space and it was selected, the destination Space
  remembers it only when the move follows current selection or the action
  explicitly selects the moved Tab;
- stale PaneSlot focus within a remembered Tab falls back to the first valid
  PaneSlot in that Tab.

Alternative considered: leave repair to individual mutations. That is easy to
start but fragile because close, move, pruning, manifest migration, and content
materialization all remove or relocate Tabs.

### Manifest compatibility remains optional-field based

Old workspace manifests decode with missing per-Space `selected_tab_id` fields.
During load, Alan seeds each Space's remembered selection from the optional field
when valid. For the globally selected Space, a valid legacy global
`selected_tab_id` is used as the remembered selection when the Space record does
not have its own value. Other Spaces without a recorded selection fall back to
their first Tab on first activation and then persist that remembered selection.

Alternative considered: bump the manifest schema and require a migration pass.
An optional-field migration is sufficient because the new field is additive and
missing values have a deterministic repair path.

## Risks / Trade-offs

- Per-Space and global focused IDs can drift if updates happen in separate
  helper paths -> route all accepted focus mutations through one selection sync
  helper and cover sidebar, keyboard, command, and control-plane paths.
- Tab move semantics can be ambiguous -> keep existing current-tab move behavior
  and only let non-current moves affect destination remembered selection when
  the executed action explicitly changes focus.
- Manifest repair may silently choose a first Tab after pruning -> record this as
  normal repair behavior and cover selected-pruned and empty-Space cases in
  tests.
- Adding fields to runtime Space records can touch many constructors -> keep
  defaults optional and preserve existing old-state decode compatibility.

## Migration Plan

1. Add optional per-Space selected Tab fields and old-manifest decode coverage.
2. Materialize and write back per-Space selected Tab state from the workspace
   manifest.
3. Route Space selection through remembered target resolution.
4. Update focus/open/move/close/prune repair paths to maintain remembered
   selection.
5. Add focused tests for same-process switching, restart restore, empty Spaces,
   selected Tab close, and Tab move-to-Space behavior.

Rollback is straightforward: ignore the optional per-Space selection fields and
fall back to the current global selected Space/Tab behavior.
