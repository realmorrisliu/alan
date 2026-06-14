# Tasks

## Implementation

- [x] Write `docs/design/design-language.md` (see plan.md Task 1) (metaphor, four principles with
      decision tests, well-rim signature spec, signal semantics table,
      relationship to OpenSpec).
- [x] Add `ShellType` role-based type scale (Pro track: display/heading/row/
      caption; Mono track: monoLabel/monoCaption) to `ShellDesignTokens.swift`.
- [x] Add `ShellSpacing` semantic 4pt scale.
- [x] Reorganize `ShellPalette` into paper / ink / signal domains with
      deprecated aliases for old names; add `ShellInk.rimHighlight`,
      `ShellInk.rimShadowLine`, `ShellSignal.action` (renamed from
      `attention`), and `ShellSignal.breathLuminanceDelta`.
- [x] Rework dark-mode palette values to the lamp hierarchy (chrome below
      terminal in luminance).
- [x] Unify the chrome onto one root paper material surface (gate-feedback
      iteration: root `ShellMaterialBackgroundView(.sidebarGlass)` replaces
      the opaque-white root and the rejected sidebar-only backing; spec delta
      in `specs/macos-shell-ui-ux-conformance/spec.md`).
- [x] Add baseline-exempt token lint script under `scripts/` and wire a
      justfile recipe.
- [x] Add justfile screenshot state-matrix target (empty space, single tab,
      split panes, multi-Space, dark mode, reduced transparency).

## Verification

- [x] Build passes and existing focused shell tests pass (`just verify`,
      `just apple-shell-focused-tests`, token test, guard — all green
      2026-06-12).
- [x] Dark-mode screenshots confirm the lamp hierarchy (maintainer gate
      2026-06-12).
- [x] Light-mode paper separation reviewed at the maintainer gate; first
      iteration (sidebar-only backing) rejected for splitting the chrome,
      unified-root iteration approved.
- [x] Lint script passes with recorded baseline and fails on a seeded new
      violation.
- [ ] Full six-state screenshot matrix run (tool landed; interactive run
      deferred — superseded in practice by the gate screenshots; run before
      PR if desired via `just apple-shell-screenshot-matrix`).

## Review and Archive

- [ ] PR review.
- [ ] Sync any `macos-shell-ui-ux-conformance` delta into `openspec/specs/`
      after merge.
- [ ] Archive this change to `openspec/changes/archive/`.
