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
- Make ordinary tab rows fastest to identify by task first, then context, then
  process or structure.
- Support task titles supplied through terminal or agent metadata while
  preserving a user-edited title lock.
- Use the existing leading split indicator unchanged, with state glyphs in the
  trailing accessory slot and state text in the subtitle when needed.
- Remove the inline pin glyph because pinned state is already conveyed by a
  tab's position in the pinned section.
- Make the tab row context menu act only on the clicked tab, with a compact
  Arc-like action set tailored to Alan's terminal model.
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
- No Alan-owned title-generation backend in this change. Agents and terminal
  programs remain responsible for setting useful terminal titles or activity
  details; Alan is responsible for displaying those signals cleanly.

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

Subtitle display should use three tiers:

- Required: actionable or exceptional states such as input needed, failed,
  paused, exited, renderer failed, read-only, starting, or a high-priority
  activity in a non-primary pane.
- Recommended: disambiguating context for a task-title tab, such as repository,
  worktree, branch, agent, process, long-running command, or split-pane count.
- Hidden: fallback or duplicate metadata such as `Terminal`, `Shell`, a repeated
  directory name, or a default shell process that does not help distinguish the
  tab.

Alternatives considered:

- Always hide subtitles. This improves density but removes useful terminal and
  agent context.
- Always show subtitles with smaller fonts. This keeps context but still makes
  ordinary idle tabs visually busy.

### Prefer provided task titles with user title lock

The row title should be the stable identity of the tab. For agent-backed or
activity-backed tabs, the best title is the task being done, not merely the
directory or process. A terminal-provided or agent-provided task title such as
`Fix sidebar tab drag` should outrank `alan` or `Codex` when it is available,
safe to show, and not just a generic process or status label.

Title source priority should be:

1. User-edited locked title.
2. Terminal-provided or agent-provided task title.
3. Trusted activity detail or command subject, if it reads like a task title and
   is supplied by the running tool.
4. Content title for non-terminal content.
5. Repository, worktree, or working-directory title.
6. Process or tab-kind fallback.

Provided task titles may update by default while the tab remains unlocked. Once
the user manually renames a tab, the title becomes locked and terminal, agent,
repository, process, or status updates must not overwrite it. State labels such
as `Running`, `Thinking`, `Failed`, or `Input needed` should not replace the
task title; they belong in the trailing accessory and subtitle.

Alternatives considered:

- Keep directory-first titles and put the task in the subtitle. This is stable
  but makes multiple `alan` + `Codex` tabs slow to distinguish.
- Have Alan call an LLM to generate tab titles itself. This adds cost, latency,
  privacy and freshness questions, and duplicates a responsibility that agents
  such as Codex can already own through terminal title updates.
- Let title fully track the latest strongest signal, including state. This can
  feel smart but makes the row identity drift while the user is scanning.

### Keep structure left and status right

The leading split/topology indicator should stay dedicated to pane structure and
pane focus. It should not become a state or agent icon, because split structure
is already interactive and stable in that position.

Pinned state should not consume inline title-row space. Pinned tabs are visually
identified by their placement in the pinned section. When there are no unpinned
tabs, New Tab should follow the pinned rows directly without a divider or empty
control-row gap. A separate `pin.fill` glyph after the title is redundant and
makes the compact row harder to scan. Pin/unpin actions should remain available
through the existing context menu and command paths, and accessibility labels may
continue to expose the pinned state.

The trailing accessory slot should carry state at rest and become the close
button when the row is hovered or keyboard focused. High-priority states should
also appear as the first subtitle token so they remain visible when the close
button temporarily replaces the glyph. Idle rows may leave the trailing slot
empty until hover.

State priority should follow the product search order:

1. Needs input, failed, paused, exited, renderer failed, read-only, starting.
2. Project or repository context.
3. Agent, command, process, or progress.
4. Structure and content type.

Alternatives considered:

- Put state glyphs in the title. This makes status easy to notice but destabilizes
  the task-title visual anchor.
- Put state glyphs in the leading split indicator. This hides pane structure and
  overloads an existing interactive control.
- Put state only in the subtitle. This is compact but not fast enough for
  high-priority scanning.

### Make tab context menus tab-scoped

The sidebar tab row context menu should contain only actions that operate on the
clicked tab. The current `New Terminal Tab` entry does not satisfy that rule
because it creates a sibling tab in the Space rather than acting on the clicked
tab; it should remain available from the New Tab row, app commands, and any
Space-level menu, but not from a tab row context menu.

The target menu should be:

1. `Rename...`
2. `Duplicate Tab`
3. `Open in Split View`
4. divider
5. `Pin Tab` or `Unpin Tab`
6. `Move to` submenu, shown only when another Space exists
7. divider
8. `Close Tab`

`Rename...` should enter the same user-title-lock path described above: once
the user renames the tab, terminal, agent, activity, repository, process, and
status updates must not overwrite that title. The menu item acts on the clicked
tab even if that tab is not currently selected.

`Duplicate Tab` should create a fresh tab in the same Space near the clicked
tab, using the clicked tab's launch context where Alan can represent it: terminal
profile, working directory, shell command or restartable command subject, and
safe pane layout metadata. It must not pretend to clone live process state,
scrollback, pending approvals, runtime sessions, or title locks. If a tab cannot
be duplicated safely, the item should be disabled with an availability reason
rather than producing a partial clone.

`Open in Split View` should map to Alan's existing terminal split model rather
than introduce a separate browser-style split-tab group. It should operate on the
clicked tab, select that tab if necessary, and create a right-side split from the
clicked tab's focused pane or primary pane using the existing split creation
path. It should be disabled for tabs whose content cannot be split through the
terminal pane model.

Pin and unpin remain organization actions, but they should be menu commands only
rather than row badges. `Move to` remains a destination submenu for other Spaces.
`Close Tab` stays the destructive final action and keeps Alan's existing close
semantics; this change does not introduce Arc-style `Archive Tab` behavior.

Browser-specific entries such as `Copy Link`, `Share`, `Change Icon...`, `Mute`,
and `Customize Page...` should stay out of the initial Alan menu until Alan has a
real tab URL, sharing, icon, audio, or page-customization contract. Clear is also
excluded because it is a batch operation over multiple inactive tabs, not an
operation on the clicked tab.

Alternatives considered:

- Mirror Arc's full menu. This would add familiar labels but several entries do
  not have Alan product semantics yet and would make the menu feel speculative.
- Keep only currently implemented commands. This is easy but misses the intended
  tab identity and duplication workflow that the menu should grow into.
- Add `Archive Tab`. The chosen removal semantic is `Close Tab`; recovery or
  history can be designed separately if Alan later needs it.

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

### Show the temporary section control only when unpinned tabs exist

The divider/control row above New Tab marks the start of the temporary tab
section, matching Arc's behavior. It should appear whenever the active Space has
at least one unpinned tab, whether or not pinned tabs also exist. It should be
hidden entirely when the Space has no unpinned tabs, including empty Spaces and
Spaces with pinned tabs only, so New Tab stays visually grouped with the pinned
rows in those cases.

Clear should appear as a subtle trailing affordance in that divider/control row
only when the active Space has at least one eligible inactive temporary tab. If
no tabs are eligible, Clear should be hidden rather than disabled, while the
divider remains visible if unpinned tabs exist.

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
