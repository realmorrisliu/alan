## Why

The macOS shell has a sophisticated token layer and a detailed behavioral UI
contract, yet the visible result reads rough and derivative of Arc. The root
causes are systemic rather than cosmetic: there is no organizing design
identity, no typography or spacing scale (font sizes and paddings are scattered
magic numbers), the attention color saturates the Space slider, and the flat
~40-entry palette namespace gives new code no semantic guidance. Behavioral
specs answer "what happens" but cannot encode the judgment needed for visual
polish.

## What Changes

- Add a design language document (`docs/design/design-language.md`) defining
  the "Paper & Ink" identity: a light studio chrome around a dark precise
  terminal surface by day, inverting to "a lamp at night" in dark mode, with
  four principles each carrying an explicit decision test.
- Add `ShellType` (role-based two-track type scale: SF Pro + SF Mono, integer
  sizes only) and `ShellSpacing` (4pt-based semantic spacing scale) to
  `ShellDesignTokens.swift`.
- Reorganize `ShellPalette` into paper / ink / signal domains, including new
  `wellRim` / `wellShadow` tokens for the paper-ink boundary signature and a
  `signalBreath` luminance variable reserved for the future agent-activity
  subsystem. Old names remain as deprecated aliases; views migrate per-file in
  follow-up changes.
- Rework dark-mode palette values so chrome sits below the terminal surface in
  luminance, and establish a unified root paper material (one continuous
  `ShellMaterialBackgroundView(.sidebarGlass)` across the whole window root,
  replacing the former opaque-white root and sidebar-only backing) plus deepen
  light paper values so the white selection surfaces stop blending into
  near-white chrome. These are the two intended visual changes in this batch.
- Add governance guards: a baseline-exempt lint script banning raw font sizes,
  raw RGB literals, and non-whitelisted paddings in shell UI directories, and a
  justfile screenshot state-matrix target for visual review.

## Capabilities

### New Capabilities

- None in this change; the design language document governs appearance judgment
  and may be promoted into a dedicated spec once stabilized by follow-up
  changes.

### Modified Capabilities

- `macos-shell-ui-ux-conformance`: token roles referenced by the contract
  (radius scale, material roles, elevation) gain typography and spacing
  counterparts; dark-mode hierarchy gains the lamp worldview requirement.
  The light-appearance root-backing scenario from the pending
  `polish-macos-workspace-colors` change is superseded: this change
  establishes the unified root material treatment instead of the opaque-white
  root that change proposed.

## Impact

- `clients/apple/alan-macos/Support/ShellDesignTokens.swift` — additive token
  refactor plus dark-value and light-paper rework.
- `clients/apple/alan-macos/MacShellRootView.swift` — two deliberate touch
  points: move `ShellMaterialBackgroundView(.sidebarGlass)` to the window root
  body (replacing `ShellPalette.rootBacking`), and remove the pinned-sidebar
  `.background` block (which would double-stack material and re-create a seam).
- `docs/design/design-language.md` — new document.
- `scripts/` and `justfile` — new lint guard and screenshot matrix target.
- All other appearances are preserved via the alias strategy.

## Out of Scope (ordered follow-up changes)

Items 3–5 were identified from a populated-state screenshot review
(2026-06-12: one pinned + one unpinned tab, restored transcript, live shell).

1. Attention-saturation bug investigation (`strongestAttention` non-idle for
   visibly empty Spaces; reproduced with normal tabs open — all five Space
   slider icons render the action orange).
2. Empty-state redesign (`ShellEmptyWorkspacePlaceholder`).
3. Restored-transcript demarcation: `RestoredTerminalTranscriptView` content
   renders continuously into live shell output with no boundary, reading as a
   duplicated session. Add a quiet divider treatment (hairline + restored-at
   label, restored content one luminance step down).
4. Pane title-bar polish: the title bar renders flat on the terminal canvas
   (the `terminalChrome` material role is not visibly applied), the close
   affordance is orphaned at the far right, and branch/worktree accessories
   use SF Pro. Adopt ink-chrome wash, `ShellSpacing` paddings, and mono-track
   accessories (depends on this change's tokens).
5. Space identity system (per-Space low-saturation ink color + optional
   icon), extended to resolving the sidebar leading-slot spec conflict:
   `macos-shell-ui-ux-conformance` requires a single-pane topology indicator
   in one requirement ("Split tabs expose compact topology") and a tab
   kind/agent icon for single-pane tabs in another ("Sidebar Tab Rows Are
   Attention-Oriented Work Rows"). The implementation
   (`ShellPaneTopologyIndicator`) follows the former everywhere; the latter
   never landed. Keep the interactive topology indicator for split tabs,
   adopt kind/agent icons for single-pane tabs, and stop filling the
   single-pane indicator with the focus accent on selection (signal-semantics
   violation: focus indigo used as a redundant selected marker).
6. Agent breathing motion built on `signalBreath`.
