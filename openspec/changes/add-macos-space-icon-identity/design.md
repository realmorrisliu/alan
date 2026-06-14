# Space Icon Identity — Design

Date: 2026-06-13
Status: Proposed, pending maintainer approval before implementation

## Decisions (from dialogue)

- Spaces are distinguished by **icon**, not auto-assigned color (color
  reverted in `polish-macos-paper-ink-identity`, commit 5f8b221).
- Policy: **auto-default + user override**.
- The auto-default must be **semantic** (derived from the Space's identity),
  because an arbitrary auto-rotation of symbols carries the same "machine
  picked it" flaw the user rejected in color. → Monogram from the Space title.

## Model: nil means monogram

`ShellSpace.presentationIconSystemName: String?` already exists and persists
through `ShellWorkspaceManifest` (`presentation_icon`). We give `nil` a new
meaning at the render layer:

- `nil` → render a **monogram** computed from the Space title at display time
  (not stored).
- non-nil + valid SF Symbol → render that **symbol** (user override).

No new model field is required. `ShellSpacePresentationIcon.resolvedSystemName`
currently collapses `nil` to `"square.grid.2x2"`; we stop using that default in
the slider and instead branch on presence.

### Monogram derivation
`ShellSpacePresentationIcon.monogram(forTitle:) -> String`:
- Trim whitespace; take the first grapheme cluster of the title.
- If it is a Latin letter, uppercase it.
- If the title is empty after trimming, return empty and the caller renders the
  neutral fallback symbol (`square.grid.2x2`) instead of a monogram.
- CJK / emoji / other scripts: use the first grapheme as-is.

Determinism: pure function of the title; script-testable.

## Rendering: monogram mode in the Space target

`ShellSidebarSpaceTrackTarget` currently renders `Image(systemName:)` in a
15×15 frame. Add a sibling path:

- If a valid symbol is set → `Image(systemName:)` (unchanged).
- Else if monogram non-empty → a `Text(monogram)` styled to sit in the same
  15×15 metrics: `ShellType.pro` at a small size with semibold weight, using
  the same foreground treatment (`iconForeground`) as the symbol, so selected /
  focused / hover / attention states behave identically.
- Else → fallback `Image(systemName: "square.grid.2x2")`.

The monogram is foreground-only (no filled tile/pill), consistent with "Paper
recedes" and the reverted-color discipline. Action signal still wins via the
existing `attention.requiresUserAction` branch in `iconForeground`.

Icon-only minimum width: the monogram is a single glyph, so it collapses at
least as well as a symbol.

## Override UI: curated picker from the Space context menu

`spaceContextMenu(for:)` already hosts a "Terminal Profile" submenu. Add:

- A primary **"Set Icon…"** affordance that presents a compact popover grid of
  curated SF Symbols (target UX, Arc-like).
- A **"Use Default"** entry that clears `presentationIconSystemName` back to
  nil (monogram).

Curated set (~24 workspace-relevant symbols, defined as a token list in
`ShellDesignTokens.swift` so it is reviewable in one place), e.g.: terminal,
chevron.left.forwardslash.chevron.right, hammer, wrench.and.screwdriver,
ant, flask, cube.box, shippingbox, server.rack, externaldrive, doc.text,
book, paintbrush, paintpalette, globe, network, lock, key, leaf, bolt,
sparkles, star, flag, folder.

Implementation note / fallback: a popover grid invoked from inside an AppKit
context menu can be fiddly. If the popover proves unreliable, the accepted
fallback is an **"Icon" submenu** listing the curated symbols as
`Label(name, systemImage:)` items plus "Use Default" — fully accessible, less
slick. The popover grid is the target; the submenu is the guaranteed floor.

## Setter + persistence

- `ShellStateMutations`: `setPresentationIcon(spaceID:systemName:)` returns new
  state with that Space's `presentationIconSystemName` replaced (nil clears).
  Validate the symbol name via the existing
  `ShellSpacePresentationIcon.isSupportedSystemName` before storing.
- `ShellHostController`: a method the view calls; persists through the existing
  manifest write path (no new persistence code — the field already serializes).

## Verification

- Script test: `monogram(forTitle:)` cases (Latin upper, lowercase→upper, CJK,
  empty→fallback, leading whitespace).
- Script test: resolution policy (nil→monogram-or-fallback; valid symbol→symbol;
  invalid symbol→fallback).
- Build + focused shell tests green; token guard/baseline unchanged (no raw
  literals; curated list uses string symbol names).
- Screenshot checkpoint: multi-Space slider shows distinct monograms by
  default; setting an icon overrides; "Use Default" restores the monogram;
  icon-only width still legible; dark mode.

## Why not …

- **Color theme (Arc-style) too**: dropped per the signal-scarcity principle
  and the user's call; can revisit as an explicit later option.
- **Full SF Symbol browser**: violates restraint; curated set first.
- **Storing the monogram**: unnecessary — it derives from the title, so it
  stays correct when the title changes and needs no migration.
