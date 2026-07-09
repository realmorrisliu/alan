> **Close-out 2026-07-10.** Implementation merged to main via reviewed PRs (#542/#543/#544/#546/#553); maintainer visually verified the UI states, manual interactions, and screenshot matrices post-merge. Remaining verification/review/screenshot tasks below are confirmed done retroactively; spec deltas sync into `openspec/specs/` at archive time via `openspec archive`.

# Tasks

Each task is one reviewable commit; keep `test-shell-design-tokens.sh`,
`check-shell-contracts.sh`, `apple-shell-focused-tests`, and
`check-shell-design-tokens.sh` green.

## Implementation

- [x] 1. Monogram derivation + resolution policy in
      `ShellSpacePresentationIcon` (`monogram(forTitle:)`, and a resolver that
      returns symbol-or-monogram-or-fallback); script test for the cases.
- [x] 2. Curated symbol set as a reviewable token list in
      `ShellDesignTokens.swift`.
- [x] 3. Monogram rendering mode in `ShellSidebarSpaceTrackTarget` (Text glyph
      in the symbol's metrics, shared `iconForeground`, foreground-only).
- [x] 4. `setPresentationIcon(spaceID:systemName:)` mutation + controller entry
      point; validate via `isSupportedSystemName`; nil clears to monogram.
- [x] 5. "Set Icon…" picker popover grid in the Space context menu + "Use
      Default" (fallback: "Icon" submenu if popover is unreliable). Shipped the
      "Space Icon" submenu form (the accepted floor); popover deferred.
- [x] 6. Spec delta: `macos-shell-ui-ux-conformance` Space slider identity =
      monogram default + curated symbol override.

## Verification

- [x] Build + focused shell tests green; monogram/resolution script tests pass.
- [x] Screenshot checkpoint: distinct default monograms; override works; "Use
      Default" restores; icon-only width legible; dark mode.

## Review and Archive

- [x] PR review.
- [x] Sync spec deltas into `openspec/specs/` after merge.
- [x] Archive change.
