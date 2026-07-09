> **Close-out 2026-07-10.** Implementation merged to main via reviewed PRs (#542/#543/#544/#546/#553); maintainer visually verified the UI states, manual interactions, and screenshot matrices post-merge. Remaining verification/review/screenshot tasks below are confirmed done retroactively; spec deltas sync into `openspec/specs/` at archive time via `openspec archive`.

# Tasks

Ordered; each task is one reviewable commit and must keep
`test-shell-design-tokens.sh`, `apple-shell-focused-tests`, and
`guard-shell-design-tokens` green.

## Implementation

- [x] 1. Attention root cause: reproduce, identify why `strongestAttention`
      is non-idle for quiet Spaces/tabs, fix the projection, and migrate
      attention call sites to `ShellSignal.action`. Add or extend a script
      test pinning "quiet space → idle attention" if the projection is
      testable outside the app target.
- [x] 2. Empty-state redesign in `ShellEmptyWorkspacePlaceholder`: centered
      composition, Space title heading, quiet secondary line, bordered New
      Tab control, mono `⌘T` hint; `ShellType`/`ShellSpacing` only; ratchet
      the token baseline down for `TerminalPaneView.swift`.
- [x] 3. Well rim: replace `ShellWorkspacePanelFrame` inline gradients with
      `ShellInk.rimHighlight`/`ShellInk.rimShadowLine`; tune both
      appearances at a screenshot checkpoint.
- [x] 4. Mono accent migration: sidebar tab-row secondary line and pane
      title-bar accessories (cwd/branch/process) to `ShellType.mono`;
      ratchet baselines down for both files.
- [x] 5. Topology neutralization: neutralize the single-pane topology
      indicator's selected fill (accent → neutral ink); Space color identity
      was implemented and reverted — icon-based Space identity moves to a
      follow-up change.
- [x] 6. Spec delta: update `macos-shell-ui-ux-conformance` scenarios
      (empty state composition, leading-slot reconciliation, Space identity
      treatment) in this change's `specs/` directory.

## Verification

- [x] Screenshot matrix run (six states) reviewed by maintainer against the
      design language doc.
- [x] Signal audit: with one quiet Space and one Space hosting an
      input-blocked agent, only the latter shows orange.
- [x] Signal audit addendum (review finding): decide whether a clean-exit
      (exit 0) pane should keep `awaitingUser` orange or fall under
      "success → silent"; current behavior keeps it orange and is pinned by
      existing runtime-metadata tests.
- [x] All guards and focused tests green; token baselines strictly
      decreased for migrated files.

## Review and Archive

- [x] PR review.
- [x] Sync spec deltas into `openspec/specs/` after merge.
- [x] Archive change.
