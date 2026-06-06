## Context

Alan's macOS shell sidebar already follows the broad Arc-like structure: a
material sidebar, a fixed Space slider, a New Tab row, and pinned/unpinned tab
sections. The current row implementation is split between
`ShellCompactEmptyAction` for New Tab and `ShellTabSidebarRow` for real tabs.
Both rows use independent padding and typography, and real tabs always reserve
space for a subtitle, which makes the list feel taller than the reference.

The existing state model already distinguishes pinned and unpinned tabs, and
workspace manifest retention already uses `ShellTabActiveTaskState` with
`protectsFromPruning` to avoid retiring active work. The Clear affordance should
reuse that safety model instead of inventing a separate temporary-tab concept.

## Goals / Non-Goals

**Goals:**

- Make New Tab and ordinary tab rows share one compact sidebar row geometry.
- Match the Arc-style New Tab interaction: quiet at rest, full-row rounded
  material background on hover or keyboard focus.
- Let real tab rows render as either a vertically centered single line or a
  meaningful two-line row without changing the row's overall visual system.
- Add a Clear action that closes inactive temporary tabs in the current Space.
- Keep Clear conservative: unpinned tabs only, never the selected tab, and never
  tabs protected by active task state.
- Preserve existing tab creation, selection, pin/unpin, close, drag, and reorder
  semantics.

**Non-Goals:**

- No browser-history model, undo stack, recently closed tab list, or global
  "clear all spaces" behavior.
- No confirmation dialog for Clear in the initial design; safety comes from
  strict eligibility.
- No changes to daemon APIs, runtime session APIs, provider configuration, or
  terminal runtime contracts.
- No dark-mode redesign beyond preserving existing adaptive color behavior.

## Decisions

### Use one sidebar row metric model

Create a small row metric source for sidebar navigation rows, either private to
`ShellSidebarView.swift` or in `ShellDesignTokens.swift` if reused by multiple
sidebar views. It should define the compact row height, horizontal inset,
leading icon slot, close slot, text spacing, and drag/drop midpoint.

Alternatives considered:

- Tune `ShellCompactEmptyAction` and `ShellTabSidebarRow` separately. This is
  faster but would preserve two visual systems and make future row polish drift.
- Move all sidebar row rendering into a new component file immediately. That may
  be useful later, but this change can stay focused unless `ShellSidebarView` is
  already too awkward to edit safely.

### Make subtitle presence meaningful

`ShellTabSidebarRow` should choose single-line or two-line layout from a
meaningful subtitle decision rather than always rendering the subtitle line.
The subtitle is meaningful when it adds task, branch, folder, process, content,
or activity information that is not just a fallback category or a repeat of the
title. Single-line tabs should keep the title vertically centered in the same
row geometry; two-line tabs should use tighter spacing so they remain compact.

Alternatives considered:

- Always hide subtitles. This improves density but removes useful terminal and
  agent context.
- Always show subtitles with smaller fonts. This keeps context but still makes
  ordinary idle tabs visually busy.

### Add Clear as a batch state mutation

Clear should be a distinct shell operation that computes eligible tabs for the
current Space and closes them as one deterministic mutation. A tab is eligible
when it is:

- in the active Space,
- unpinned,
- not the selected tab,
- and not protected by its current `ShellTabActiveTaskState`.

The host should compute active task state from current runtime metadata when
available, falling back to persisted tab activity metadata where appropriate.
The mutation should preserve selected tab and pane focus if they remain valid.

Alternatives considered:

- Invoke existing close-tab behavior repeatedly from the Clear button. This
  reuses code but can stop midway if a guard appears and is harder to reason
  about in tests.
- Show a confirmation dialog listing tabs. This is safer in the abstract but too
  heavy for an Arc-like lightweight cleanup control, and the eligibility rule is
  already conservative.

### Repair tab dragging at the drag/drop boundary

The underlying reorder model is already covered by `test-shell-tab-organization`
and passes; the weak point is the SwiftUI drag/drop boundary. The current
sidebar drop delegate relies on view-local `activeTabDrag` state to identify the
dragged tab, while the drag session also carries an `NSItemProvider` payload.
That state can be lost or cleared independently of the drop session, which makes
the drop target unable to apply the reorder even though the state mutation path
works.

The implementation should treat the drag payload as the source of truth for the
dragged tab identity and use row-local state only for transient visual preview.
The drop delegate should load or otherwise receive a typed local payload that
contains the tab ID and source location, validate it against the current
`ShellStateSnapshot`, compute the target index, and route through
`ShellHostController.reorderTab`.

Alternatives considered:

- Keep relying on `activeTabDrag` and adjust the cleanup delay. This may mask
  one timing case but keeps source identity outside the drag session.
- Route all reorders through command buttons only. This preserves model
  behavior but abandons the expected sidebar pointer interaction.

### Show Clear only when it can act

Clear should appear as a subtle trailing affordance in the divider/control row
above New Tab only when the active Space has at least one eligible inactive
temporary tab. If no tabs are eligible, Clear should be hidden rather than
disabled.

Alternatives considered:

- Always show disabled Clear. This explains the feature but adds persistent
  visual noise.
- Put Clear in a context menu only. This hides useful cleanup for the common
  temporary-tab workflow.

## Risks / Trade-offs

- [Risk] Row height may still feel off when measured against the reference.
  -> Tune the metric through screenshot verification in Alan Dev after a fresh
  relaunch.
- [Risk] Subtitle filtering might hide context a user expected.
  -> Keep the meaningful-subtitle helper focused and covered by small tests.
- [Risk] Runtime active-task metadata can lag briefly behind terminal state.
  -> Use the same protection primitive as pruning and keep Clear eligible only
  when the task state is inactive or otherwise unprotected.
- [Risk] Batch closing can expose assumptions in mutation result focus repair.
  -> Add model tests for selected-tab preservation, protected-tab retention, and
  empty-Space outcomes.
- [Risk] SwiftUI drag/drop testing is harder than model testing.
  -> Cover pure target-index and payload validation helpers with automated tests
  and require fresh Alan Dev manual verification for actual pointer drag.

## Migration Plan

This is a local UI and shell-state behavior change. Existing workspace manifests
do not need migration. Rollback is reverting the OpenSpec change and associated
Swift code.

## Open Questions

None. The initial Clear scope is current Space only, inactive unpinned tabs
only, and selected/protected tabs are retained.
