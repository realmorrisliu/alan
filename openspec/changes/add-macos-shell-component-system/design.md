## Context

The macOS SwiftUI client already has two of the three layers a design system needs:
design tokens (`Support/ShellDesignTokens.swift` — Paper & Ink palette, spacing,
type) and a nascent control library (`Views/Shell/Controls/ShellFormControls.swift`
with `ShellButton`, `ShellTextField`, `ShellSelectField`, `ShellIconTile`,
`ShellIconPickerPanel`). What is missing is the *contract that ties them together*
and *adoption*: the control library is referenced by exactly one surface (the Space
creation form). Raw-literal styling drift is *already* guarded: the project ships
`scripts/check-shell-design-tokens.sh` + `scripts/shell-design-token-baseline.txt`, a
per-file ratchet over `system(size:`/`Color(red:`/`NSColor(red:`/numeric `.padding(`
whose shell baseline is `TerminalPaneView.swift` 63, `ShellSidebarView.swift` 16,
`MacShellRootView.swift` 1. Two real gaps remain: (1) that guard is only a local `just`
recipe, not a CI check; (2) it does not address presentational *duplication* — the
same concepts live as `private struct`s inside large feature files. (An earlier draft
of this proposal mistakenly treated `ShellPalette.*` references as debt to drive to
zero; `ShellPalette` is the semantic token namespace, so referencing it is the
*compliant* outcome, not debt. That framing has been corrected.) The duplication:

- Five row implementations: `ShellSettingsRow`, `ShellSettingsAgentSummaryRow`,
  `TerminalInfoRow` (all in `TerminalPaneView.swift`), `ShellTabSidebarRow`, and
  `ShellSidebarTabControlRow` (in `ShellSidebarView.swift`).
- Parallel card/chip/panel types: `TerminalInfoCard`, `TerminalPaneChip`, and the
  `ShellWorkspacePanelFrame` modifier (all in `TerminalPaneView.swift`).
- A canonical press style (`ShellButtonPressStyle`) and field (`ShellTextField`)
  already exist in the control library but are not adopted across shell surfaces.

(`TimelineRow`, `SidebarActionButtonStyle`, `InlineActionButtonStyle`, and
`CompactDarkFieldStyle` live only under `Views/Console/` and are intentionally NOT in
scope — they belong to the isolated legacy/mobile console.)

Two related specs already exist and this design deliberately does not overlap them:
`macos-app-architecture-maintainability` owns file/ownership boundaries and the
AppKit/terminal-host bridge boundary; `macos-shell-ui-ux-conformance` owns the
visual/interaction rules (materials, space slider, collapsed sidebar). The new
`macos-shell-component-system` capability owns the *presentational reuse contract*:
token single-source, the primitive catalog, style/structure separation, the preview
gallery + accessibility baseline, and the migration discipline.

## Programmable Environment Alignment

`add-macos-shell-component-system` is a **host-surface/design-system** change under
the programmable environment direction, not an environment-core or app-domain change.
Its responsibility is to make the macOS host surface capable of rendering future
environment views with native SwiftUI primitives that are reusable, accessible,
previewable, and token-governed.

- **Environment role:** host surface / macOS design-system capability.
- **Runtime mapping:** none at the environment core layer. These primitives may render
  environment views and buffers later, but they do not define object, command, buffer,
  view, query, ledger, or agent semantics.
- **Native authority:** SwiftUI component files, semantic design tokens, preview
  galleries, accessibility behavior, and the design-token guard.
- **Host boundary:** layout, chrome, native controls, input presentation, selection
  affordances, and visual migration discipline. The host composes environment state; it
  is not the source of truth for that state.
- **Deferred migration:** terminal-host AppKit internals, Rust kernel/runtime
  capability, environment apps, and the current shell runtime remain outside this
  change.

## Goals / Non-Goals

**Goals:**
- Define a three-layer model — `Tokens → Primitives → Feature compositions` — with a
  single design-system home at `Views/Shell/Components/`.
- Make tokens the single styling source: feature surfaces stop hard-coding raw
  literals (`Color(red:`/`Color.red`/`.font(.system(size:`/numeric `.padding(`) and
  instead reference semantic token namespaces (`ShellPaper`/`ShellInk`/`ShellSignal`/
  `ShellPalette`/`ShellType`/`ShellSpacing`) or compose primitives. Referencing
  `ShellPalette.*` is the compliant outcome, not debt.
- Consolidate duplicated rows, styles, cards, chips, and field types into canonical
  primitives.
- Ship a `#Preview` gallery and accessibility baseline for every primitive.
- Migrate existing surfaces incrementally (strangler-fig), one surface per change,
  with measurable inline-styling reduction and screenshot parity.

**Non-Goals:**
- No big-bang rewrite of multiple surfaces in a single change.
- No changes to Ghostty/AppKit terminal-host internals (input, attachment, overlay
  layout) — those stay under the terminal-host boundary.
- No migration of the legacy/mobile remote-control console (`Views/Console/`). It is
  classified as legacy/mobile and `macos-app-architecture-maintainability` requires it
  to stay isolated from the primary macOS shell, so it is not pulled through the shell
  design-system home.
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

### Decision: The contract is a ratchet, so Phase 0 can sync to `openspec/specs/` honestly

The spec binds *new and migrated* shell code (compose primitives, no raw tokens) and
forbids *new* violations, while explicitly recording the inline styling and local
structs still in un-migrated surfaces as tracked migration debt whose counts must not
increase. This matters because Phase 0 migrates no surface: an absolute "no surface
inlines styling" contract would be false the moment it was synced to
`openspec/specs/` (e.g. `ShellSettingsRow`/`TerminalInfoRow` still live in
`TerminalPaneView.swift`). The ratchet form is true at sync time and tightens
surface-by-surface as the strangler-fig migrations land.

- **Alternative — defer the spec sync/archive until every surface is migrated:**
  rejected. The whole point of Phase 0 is to give new code a contract *now*; holding
  the contract out of `openspec/specs/` until the last migration would let drift
  continue unconstrained in the meantime.

### Decision: A single design-system home, absorbing `Controls/`

`Views/Shell/Components/` becomes the home; `ShellFormControls.swift` content moves
(or is re-grouped) under it. Keeping one home avoids a `Controls/` vs `Components/`
split that would itself become drift. The Apple README directory section is updated
to match (required by `macos-app-architecture-maintainability`).

### Decision: Reuse the existing design-token guard; do not build a parallel ratchet

The raw-literal floor is already owned by `scripts/check-shell-design-tokens.sh` (a
per-file baseline ratchet with `--update-baseline`). This capability **adopts** it
rather than defining a second count, avoiding a duplicate owner for the same behavior.
The one concrete tooling change is to **wire that guard into CI** (`ci.yml`), since it
is only a local `just` recipe today — that converts the ratchet from a manual promise
into a blocking gate. Forms the guard does not yet match (`RoundedRectangle` with a
literal radius, raw named hues like `Color.red`) are caught in review and may later be
folded into the guard as an extension; they are not a new spec-owned count.

- **Alternative — define a fresh four-signal count (ShellPalette/RoundedRectangle/
  color/font) in this spec:** rejected. It duplicated the existing guard, and worse,
  it counted `ShellPalette.*` as debt when `ShellPalette` is the semantic token
  namespace whose use is the desired end state — the metric pointed the wrong way.

### Decision: Strangler-fig migration order driven by reuse leverage

Build the primitives first (Phase 0), then migrate the five debt-carrying surfaces
highest-leverage-first: sidebar (most rows + selection states) → space slider →
settings surface → terminal-pane SwiftUI chrome → root chrome (`MacShellRootView.swift`).
Each surface is its own change/PR — including the two pairs that share a feature file
(sidebar vs space slider in `ShellSidebarView.swift`; settings vs terminal-pane chrome
in `TerminalPaneView.swift`) — so screenshots and metrics are reviewable in isolation
and the old implementation "dies back" gradually. Root chrome carries the least debt
(guard baseline 1) but is still its own owning phase so the guard baseline reconciles
to zero across all shell feature files.

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
2. **Phase 1+ — Per-surface strangler migration (separate changes):** the five
   debt-carrying surfaces — sidebar → space slider → settings surface → terminal-pane
   SwiftUI chrome → root chrome (`MacShellRootView.swift`). Each removes
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
