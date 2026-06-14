## Why

Spaces are visually indistinguishable: every Space slider target renders the
same default `square.grid.2x2` glyph. An earlier attempt auto-assigned a color
per Space, but color is machine-assigned and carries no meaning — and the same
objection applies to auto-rotating arbitrary symbols. The differentiator users
actually reason about is a per-Space icon they can recognize and control, as in
Arc. The model already carries `presentationIconSystemName` and persists it
through the workspace manifest; what is missing is a meaningful default, a way
to render a non-symbol default, and an affordance to set it.

## What Changes

- **Semantic auto-default (monogram).** When a Space has no user-set icon
  (`presentationIconSystemName == nil`), the slider target renders a monogram
  derived from the Space title's first character (Latin letters uppercased;
  CJK and other scripts use the first grapheme; empty/untitled falls back to a
  neutral symbol). This is deterministic, distinct, and tied to the Space's
  real identity — unlike color or rotated symbols. Auto-created Spaces are
  distinguishable with zero configuration.
- **User override via a curated icon picker.** A new "Set Icon…" affordance in
  the existing Space context menu opens a compact popover grid of a curated set
  of workspace-relevant SF Symbols. Choosing one sets
  `presentationIconSystemName`; a "Use Default" entry clears it back to the
  monogram. The picker is restrained (curated set, not the full SF Symbol
  catalog) per the design language.
- **Setter mutation + control path.** A `setPresentationIcon` shell mutation
  threads the choice through the controller and persists via the existing
  manifest field. (Persistence storage already exists; only the write path is
  new.)
- **Monogram render support.** The Space slider target gains a monogram
  rendering mode (a letter glyph in the same metrics as the symbol icon),
  selected when no symbol is set.

## Capabilities

### New Capabilities

- None (extends existing `macos-shell-ui-ux-conformance` Space slider
  behavior).

### Modified Capabilities

- `macos-shell-ui-ux-conformance`: Space slider targets gain a per-Space icon
  identity — a title-derived monogram by default, a user-chosen curated SF
  Symbol when set — replacing the deferred color-identity approach. The icon
  remains icon-foreground treatment within the existing slider geometry (no
  filled pills, no new chrome), and an action signal still takes precedence
  over identity.

## Impact

- `clients/apple/alan-macos/Models/Shell/ShellSnapshots.swift` —
  `ShellSpacePresentationIcon` gains monogram derivation helpers.
- `clients/apple/alan-macos/Models/Shell/ShellStateMutations.swift` —
  `setPresentationIcon(spaceID:systemName:)` mutation.
- `clients/apple/alan-macos/ShellHostController.swift` — setter entry point.
- `clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift` — monogram vs
  symbol rendering in the Space target; "Set Icon…" context-menu item + picker
  popover.
- Curated symbol set + monogram styling tokens in `ShellDesignTokens.swift`.
- Tests: script-level assertions for monogram derivation and the
  default/override resolution.

## Out of Scope

- Emoji or arbitrary-letter custom icons (curated SF Symbols + auto monogram
  only).
- Per-Space color theming (explicitly dropped).
- A full SF Symbol browser.
- Cross-device sync of icon choices beyond the existing manifest persistence.
