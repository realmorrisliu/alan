## Why

The `add-macos-paper-ink-design-system` change established the Paper & Ink
foundation: design language, type/spacing/color tokens, lamp dark mode,
unified root paper material, and governance guards. The foundation is
deliberately invisible. Distinctiveness — "screenshot it and you know it's
Alan" — lives in the signature elements that consume those tokens, none of
which have landed. Chrome tone tuning cannot produce identity; these five
elements can.

## What Changes

Ordered by visible impact per unit of risk:

- **Signal scarcity (attention fix).** Investigate and fix
  `strongestAttention` reporting non-idle for Spaces with no actionable
  state (observed: most Space slider icons render action orange in every
  reviewed screenshot). After the fix, orange appears only when an agent or
  command is blocked on the user, per the signal semantics table in
  `docs/design/design-language.md`. Migrate the attention call sites from
  `ShellPalette.attention` to `ShellSignal.action`.
- **Empty-state redesign.** Replace `ShellEmptyWorkspacePlaceholder`'s
  left-hugging composition with a centered composition on the raised paper
  panel: Space title as the heading (fallback "Empty Space"), one quiet line,
  the New Tab action as a proper bordered control, and a `⌘T` key hint in the
  mono track. Uses `ShellType` / `ShellSpacing` exclusively.
- **Well rim application.** Promote the inline gradients of
  `ShellWorkspacePanelFrame` (TerminalPaneView.swift) to the
  `ShellInk.rimHighlight` / `ShellInk.rimShadowLine` tokens, tuned so the
  paper/ink boundary reads as a physical well edge in both appearances —
  the screenshot signature.
- **Mono accent migration.** Machine facts switch to `ShellType.mono`:
  sidebar tab-row secondary lines (worktree/branch), pane title-bar
  accessories (cwd, branch, process), and the empty-state key hint. Human
  copy stays SF Pro. This is the cheapest large step away from
  generic-Mac-app feel.
- **Space identity (topology neutralization only; icon identity deferred).**
  Auto-assigned color identity for Space slider targets was implemented but
  is being dropped in favour of an icon-based identity approach in a
  follow-up change. What landed and is retained: single-pane tabs stop
  filling the topology indicator with the focus accent when selected
  (selection is already conveyed by the row surface).

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `macos-shell-ui-ux-conformance`: empty-state composition requirement
  updated (centered, Space-titled, key-hint allowed as quiet mono caption);
  single-pane leading-slot scenario reconciled with the topology-indicator
  requirement (topology shape stays, selected fill becomes neutral); Space
  slider targets gain per-Space ink identity treatment.

## Impact

- `clients/apple/alan-macos/Models/Shell/*` — attention projection logic
  (bug fix) and Space identity ink derivation.
- `clients/apple/alan-macos/TerminalPaneView.swift` — empty state, well rim
  tokens, pane title-bar mono accessories.
- `clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift` — tab-row
  secondary mono line, Space target ink tint, topology selected fill.
- `scripts/shell-design-token-baseline.txt` — counts decrease as migrated
  files adopt tokens (ratchet down via `--update-baseline` in the same
  commits).
- Visual review via `just apple-shell-screenshot-matrix`.

## Out of Scope

- Restored-transcript demarcation (separate follow-up).
- Pane title-bar structural rework beyond accessories (close-affordance
  placement stays).
- Agent breathing motion (`signalBreath` consumer) — final batch.
- User-facing Space color picker UI (identity inks are auto-assigned this
  change; override UI later).
