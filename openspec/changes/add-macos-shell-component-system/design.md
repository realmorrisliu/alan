## Context

The macOS SwiftUI client already has two of the three layers a design system needs:
design tokens (`Support/ShellDesignTokens.swift` — Paper & Ink palette, spacing,
type) and a nascent control library (`Views/Shell/Controls/ShellFormControls.swift`
with `ShellButton`, `ShellTextField`, `ShellSelectField`, `ShellIconTile`,
`ShellIconPickerPanel`). What is missing is the *contract that ties them together*
and *adoption*: the control library is referenced by exactly one surface (the Space
creation form), while the rest of the UI hand-rolls styling — ~189 direct
`ShellPalette.*` references, ~71 inline `RoundedRectangle` shapes, and duplicated
presentational concepts living as `private struct`s inside large feature files:

- Five row implementations: `ShellSettingsRow`, `ShellSettingsAgentSummaryRow`,
  `ShellTabSidebarRow`, `TerminalInfoRow`, `TimelineRow`.
- Three press/hover styles: `SidebarActionButtonStyle`, `InlineActionButtonStyle`,
  `ShellButtonPressStyle`.
- Parallel card/chip/field types: `TerminalInfoCard`, `TerminalPaneChip`,
  `CompactDarkFieldStyle` (the last duplicates `ShellTextField`).

Two related specs already exist and this design deliberately does not overlap them:
`macos-app-architecture-maintainability` owns file/ownership boundaries and the
AppKit/terminal-host bridge boundary; `macos-shell-ui-ux-conformance` owns the
visual/interaction rules (materials, space slider, collapsed sidebar). The new
`macos-shell-component-system` capability owns the *presentational reuse contract*:
token single-source, the primitive catalog, style/structure separation, the preview
gallery + accessibility baseline, and the migration discipline.

## Goals / Non-Goals

**Goals:**
- Define a three-layer model — `Tokens → Primitives → Feature compositions` — with a
  single design-system home at `Views/Shell/Components/`.
- Make tokens the single styling source: feature surfaces stop reading raw color
  tuples / `ShellPalette.*`.
- Consolidate duplicated rows, styles, cards, chips, and field types into canonical
  primitives.
- Ship a `#Preview` gallery and accessibility baseline for every primitive.
- Migrate existing surfaces incrementally (strangler-fig), one surface per change,
  with measurable inline-styling reduction and screenshot parity.

**Non-Goals:**
- No big-bang rewrite of multiple surfaces in a single change.
- No changes to Ghostty/AppKit terminal-host internals (input, attachment, overlay
  layout) — those stay under the terminal-host boundary.
- No new Swift build target / module (`AlanDesignSystem` package) at this time.
- No token *value* changes; this is structure/adoption, not a visual redesign.
- No changes to Rust crates or non-macOS clients.

## Decisions

### Decision: Three-layer model with wrapper-View primitives, not a style-only library

Organize as `Tokens → Primitives (Views) → Feature compositions`. Primitives are
small composable `View`s (e.g. `ShellRow` = icon + title + subtitle + accessory with
hover/selected states), built internally from shared `ButtonStyle`/`ViewModifier`
types.

- **Alternative — style/modifier-centric (no wrapper Views):** more SwiftUI-minimal,
  but a flat set of modifiers is undiscoverable and can't express compound structure
  (a row is structure, not a style). Rejected as the top-level organization, but its
  mechanics are *used inside* primitives (the "style vs structure" requirement).
- **Alternative — separate `AlanDesignSystem` Swift module** for compile-time
  boundary enforcement: strongest isolation, but heavy for a single app and
  mismatched with the script-driven Apple test setup. Rejected (YAGNI); the boundary
  is enforced by convention + a measurable lint-style gate instead.

### Decision: A single design-system home, absorbing `Controls/`

`Views/Shell/Components/` becomes the home; `ShellFormControls.swift` content moves
(or is re-grouped) under it. Keeping one home avoids a `Controls/` vs `Components/`
split that would itself become drift. The Apple README directory section is updated
to match (required by `macos-app-architecture-maintainability`).

### Decision: Enforce the token single-source rule with a measurable gate, not a compiler

The contract bans feature-layer raw `ShellPalette.*` / inline `RoundedRectangle`.
Since there is no separate module to enforce this at compile time, each migration
change is gated on a *count trending to zero* for the targeted surface (a grep-based
metric in review) plus screenshot parity. This is observable without new tooling and
fits the existing screenshot-review workflow.

### Decision: Strangler-fig migration order driven by reuse leverage

Build the primitives first (Phase 0), then migrate surfaces highest-leverage-first:
sidebar (most rows + selection states) → space slider → console → settings panels →
terminal-pane SwiftUI chrome. Each surface is its own change/PR so screenshots and
metrics are reviewable in isolation and the old implementation "dies back" gradually.

- **Alternative — big-bang refactor (originally requested):** rejected as the
  *implementation* strategy. UI regressions are visual and screenshot-reviewed; a
  single mega-PR is unreviewable and stacks regression risk. The *spec* still
  describes the full target end-state — ambition lives in the contract, safety lives
  in the rollout.

## Risks / Trade-offs

- **Visual regression during migration** → Each phase holds screenshot parity for its
  surface and runs `apple-shell-focused-tests` + UI smoke before merge; one surface
  per change keeps the diff inspectable.
- **Over-abstraction (extracting primitives used once)** → Only promote a primitive
  when ≥2 surfaces need it, or when consolidating ≥2 existing duplicates; the catalog
  is justified by the duplication evidence in Context, not invented up front.
- **`ShellRow` over-generalization** (one row type that can't fit every case) → Keep
  it a focused icon/title/subtitle/accessory row with explicit state inputs; surfaces
  with genuinely distinct structure may compose primitives rather than force-fit
  `ShellRow`.
- **Convention-only token boundary drifts back** → The per-surface metric gate makes
  regressions visible in review; if drift recurs, a CI grep check can be added later
  (deferred, not built now).
- **Scope creep into terminal-host AppKit** → The spec explicitly scopes migration to
  SwiftUI chrome and defers host internals to the terminal-host boundary.

## Migration Plan

1. **Phase 0 — Foundation (this change):** create `Views/Shell/Components/`, relocate
   the existing control library into it, define canonical primitives (`ShellRow`,
   `ShellBadge`/`ShellChip`, card/panel surface modifiers, the unified press/hover
   style, `ShellSectionHeader`), add `#Preview` galleries + accessibility baseline,
   update the README. No feature surface is migrated yet.
2. **Phase 1+ — Per-surface strangler migration (separate changes):** sidebar → space
   slider → console → settings panels → terminal-pane SwiftUI chrome. Each removes
   inline styling in its surface, deletes the now-dead local structs/styles, and
   verifies the metric + screenshot gate.
3. **Rollback:** each phase is an isolated change; reverting one surface's change
   restores its prior inline implementation without affecting the primitives or other
   surfaces.

## Open Questions

- Final home name: `Views/Shell/Components/` vs renaming to `Views/Shell/DesignSystem/`.
  (Leaning `Components/` to align with the existing `Controls/` it absorbs;
  resolvable in Phase 0 without affecting the contract.)
- Whether `ShellBadge` and `ShellChip` are one primitive with a variant or two
  primitives — decide when consolidating `TerminalPaneChip` against actual call sites.
