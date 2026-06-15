## ADDED Requirements

### Requirement: Presentational primitives live in a single design-system home

The macOS SwiftUI client SHALL keep reusable presentational primitives — surfaces,
controls, rows, indicators, and labels — together in one design-system home under
`clients/apple/alan-macos/Views/Shell/Components/`, absorbing the existing
`Views/Shell/Controls/` library. New and migrated shell feature surfaces SHALL
compose these primitives rather than redefining equivalent presentational structs
locally. Shell surfaces not yet migrated MAY retain their existing local structs as
tracked migration debt (see the migration-debt requirement below), but no feature
file SHALL introduce a *new* local duplicate of an existing primitive's role.

#### Scenario: A reusable presentational primitive is added

- **WHEN** a developer adds a button, field, row, card, chip, badge, tile, or
  section-label primitive intended for reuse across surfaces
- **THEN** it is defined in the design-system home under `Views/Shell/Components/`
- **AND** it is `internal` or `public` (not `private` to a feature file) so other
  surfaces can compose it

#### Scenario: A new or migrated feature file needs a presentational treatment

- **WHEN** new shell code, or a surface being migrated, needs a
  row/card/chip/badge/field/button treatment
- **THEN** it composes the corresponding design-system primitive
- **AND** it does not declare a new `private struct` that duplicates an existing
  primitive's role

#### Scenario: Apple client README is inspected

- **WHEN** a developer reads the Apple client README directory section
- **THEN** the `Views/Shell/Components/` design-system home is documented as the owner
  of reusable presentational primitives

### Requirement: Design tokens are the single styling source

Design tokens SHALL be the single source of styling values for the macOS SwiftUI
client. Only the design-system layer (primitives and their styles under
`Views/Shell/Components/`, plus `Support/ShellDesignTokens.swift`) SHALL read raw
color/number tuples or reference `ShellPalette.*` directly. New and migrated shell
feature surfaces SHALL consume semantic tokens or primitives instead of raw styling
values. This includes, beyond `ShellPalette.*`: raw color literals
(`Color(red:/white:/.sRGB/hex …)` constructors and raw named hues such as
`Color.red`/`.orange`/`.green`/`.white`/`.black`) and raw typography literals
(`.font(.system(size:…))`, which SHALL come from the `ShellType` token scale). The
structural system colors `Color.clear`, `Color.primary`, `Color.secondary`,
`Color.accentColor` and semantic system fonts (`.headline`, `.body`, …) remain
permitted. The raw-styling usage remaining in not-yet-migrated surfaces is tracked
migration debt (see the migration-debt requirement below).

The migration-debt ratchet counts an explicit, named set of measurable signals
(`ShellPalette.*`, `RoundedRectangle`, raw color literals, and raw `.font(.system(size:))`
typography literals). That set is a proxy, not the full extent of the rule: a
raw-styling form not captured by the counted signals — e.g. a literal `CGFloat` corner
radius, a hard-coded `LinearGradient`/`RadialGradient`, a literal `.shadow(…)`, or a
`Capsule`/`Circle`/`Rectangle` carrying a raw fill — is still a violation of this
requirement and SHALL be rejected in review even though it does not move a counted
number.

#### Scenario: A new or migrated feature surface needs a color or shape metric

- **WHEN** new shell code, or a surface being migrated, needs a background color,
  selection tint, corner radius, spacing, or border treatment
- **THEN** it obtains it by composing a design-system primitive or referencing a
  semantic token exposed for that purpose
- **AND** it does not read a raw `(Double, Double, Double)` tuple, reach into
  `ShellPalette.*`, or use a raw color literal (`Color(red:…)`/`Color.red` etc.;
  `Color.clear`/`.primary`/`.secondary`/`.accentColor` are allowed)

#### Scenario: A new token is needed by a feature

- **WHEN** an existing semantic token does not cover a feature's need
- **THEN** the new value is added to the design-system token layer with a semantic
  name (e.g. `sidebarSelection`, not `blue500`)
- **AND** the feature consumes the named token rather than inlining the literal value

### Requirement: Style is separated from structure

Reusable appearance behavior for shell surfaces SHALL be expressed as `ButtonStyle`,
`TextFieldStyle`, `LabelStyle`, or `ViewModifier` types owned by the design-system
layer — this covers press feedback, hover state, focus ring, and field chrome. New
and migrated shell feature surfaces SHALL NOT define their own ad-hoc style or
modifier types for these concerns, and the design-system layer SHALL expose one
canonical style per concern rather than duplicating it.

#### Scenario: Press or hover feedback is needed in a new or migrated shell control

- **WHEN** a new or migrated shell control needs press-scale, hover, or focus feedback
- **THEN** it applies the shared design-system style/modifier for that concern
- **AND** it does not introduce a new shell `ButtonStyle` that duplicates the
  canonical press/hover style

#### Scenario: Field chrome is needed in a new or migrated shell control

- **WHEN** a new or migrated shell text-entry control needs border/background/focus
  chrome
- **THEN** it uses the canonical design-system field primitive or its shared style
- **AND** it does not introduce a parallel shell field-style type that duplicates the
  canonical one

### Requirement: Every primitive ships a preview gallery and accessibility baseline

Each design-system primitive SHALL ship a SwiftUI `#Preview` gallery that renders
its meaningful states (at minimum default, hover, selected, disabled where
applicable, and dark appearance), and SHALL satisfy a baseline of accessibility:
Dynamic Type scaling, a VoiceOver-accessible label, and respect for reduce-motion.

#### Scenario: A primitive is reviewed

- **WHEN** a reviewer opens a design-system primitive
- **THEN** a `#Preview` exists that shows the primitive across its applicable states
  in both light and dark appearance

#### Scenario: A control primitive renders for an assistive user

- **WHEN** a control primitive is presented to VoiceOver or under increased Dynamic
  Type sizes
- **THEN** it exposes an accessible label and scales legibly without clipping its
  content
- **AND** any animated feedback honors the reduce-motion setting

### Requirement: Pre-existing inline styling and local structs are tracked migration debt

Pre-existing inline styling SHALL be tracked migration debt and the ratchet measured
against two objective invariants. First, the counted styling-signal occurrences on
shell *feature* surfaces — `ShellPalette.*`, `RoundedRectangle`, raw color literals
(`Color(red:/white:/.sRGB/hex …)` plus raw named hues `Color.red`/`.orange`/`.green`/
`.white`/`.black`; structural `.clear`/`.primary`/`.secondary`/`.accentColor`
excluded), and raw typography literals (`.font(.system(size:…))`; semantic system
fonts excluded), all measured outside the design-system layer, which is permitted to
reference tokens directly — SHALL NOT increase. Second, no shell feature file SHALL
introduce a *new* local struct that
duplicates a design-system primitive's role (row, card, chip, badge, button, field,
section label, or surface/background modifier); feature-specific composite views that
do not duplicate a primitive role (e.g. layout, split, title-bar, or find-bar views)
are unaffected and legitimately stay in feature files. Each strangler-fig migration
SHALL reduce the counts and fold the known primitive-role duplicates for its targeted
surface until they reach zero outside the design-system layer.

The baseline at the time the component layer is introduced, against which the
"SHALL NOT increase" ratchet is measured, is recorded here so future migrations have
a concrete reference point:

- **Inline `ShellPalette.*` occurrences on shell feature surfaces:** ≈ 136
  (all-files 197, minus console 0, minus the design-system layer
  `Support/ShellDesignTokens.swift` 46 and `ShellFormControls.swift` 15).
- **Inline `RoundedRectangle` occurrences on shell feature surfaces:** ≈ 29
  (all-files 71, minus console 34, minus the design-system layer `ShellFormControls.swift` 8).
- **Raw color literals on shell feature surfaces:** ≈ 20 (`TerminalPaneView.swift` 16,
  `ShellSidebarView.swift` 4, `MacShellRootView.swift` 0), counting raw named hues and
  `Color(...)` value constructors but excluding `.clear`/`.primary`/`.secondary`/
  `.accentColor`. No raw `Color(...)` value constructors exist today; the debt is
  named hues (`.white` 13, `.orange`/`.green` 2 each, `.black` 2, `.red` 1).
- **Raw `.font(.system(size:))` typography literals on shell feature surfaces:** ≈ 37
  (`TerminalPaneView.swift` 30, `ShellSidebarView.swift` 6, `MacShellRootView.swift` 1),
  to be replaced by the `ShellType` token scale; semantic system fonts are excluded.
- **Known pre-existing primitive-role duplicates (non-exhaustive consolidation
  backlog, not a closed allow-list):** rows — `ShellSettingsRow`,
  `ShellSettingsAgentSummaryRow`, `TerminalInfoRow` (`TerminalPaneView.swift`),
  `ShellTabSidebarRow`, `ShellSidebarTabControlRow` (`ShellSidebarView.swift`);
  card/surface — `TerminalInfoCard`, the `ShellWorkspacePanelFrame` modifier,
  `ShellSettingsNavigationRowBackground` (`TerminalPaneView.swift`),
  `ShellSidebarRowBackground` (`ShellSidebarView.swift`); chip — `TerminalPaneChip`.
  The compliance test is the *role-based* rule above, not membership in this list, so
  an incomplete enumeration is not a loophole: any new primitive-role duplicate is a
  violation whether or not it appears here.

These counts are measured as `all − console − design-system-layer` (the
exclusion-glob form is unreliable with a path argument); the occurrence counts above
are ceilings, not floors.

#### Scenario: Migration debt is enumerated when the component layer lands

- **WHEN** the component layer is introduced (its foundation change)
- **THEN** the shell surfaces still carrying inline styling or primitive-role
  duplicate structs are recorded as the migration backlog (one follow-up change per
  surface)
- **AND** the baseline counts (≈ 136 `ShellPalette.*`, ≈ 29 `RoundedRectangle`,
  ≈ 20 raw color literals, ≈ 37 raw `.font(.system(size:))` literals) and the known
  primitive-role duplicate list above are captured as the ratchet reference point
- **AND** the long-lived spec is true at that point: it requires *new and migrated*
  code to comply and forbids *new* violations, not that all debt is already cleared

#### Scenario: A change would add new inline styling or a primitive-role duplicate

- **WHEN** a change adds, to a shell feature surface instead of composing a primitive,
  a new inline `RoundedRectangle`/`ShellPalette.*` use, a raw color literal
  (`Color(red:…)` or a raw named hue like `Color.red`), a raw `.font(.system(size:…))`
  typography literal, another raw-styling form not captured by the counted signals
  (e.g. a literal corner radius or gradient), or a new local struct that duplicates a
  primitive's role (row/card/chip/badge/button/field/label/surface treatment)
- **THEN** it is rejected: the counted signals SHALL NOT increase, no other raw
  styling is introduced, and no new primitive-role duplicate is added
- **AND** the developer composes the existing primitive or adds one to the
  design-system home

#### Scenario: A change adds a feature-specific composite view

- **WHEN** a change adds a new private subview that arranges existing primitives or
  encodes feature-specific layout (e.g. a split, title-bar, or find-bar view) without
  re-implementing a primitive's role
- **THEN** it is permitted: such composite views are not migration debt and may live
  in feature files

### Requirement: Surfaces adopt the component layer via measurable strangler-fig migration

Existing surfaces SHALL be migrated to the component layer one surface at a time, as
separate changes, with each migration reducing inline styling in the targeted
surface and preserving its visual behavior. The component layer SHALL NOT be adopted
through a single big-bang rewrite of multiple surfaces at once — and, because a single
feature file may host more than one surface, a migration change SHALL target a
surface, never a whole multi-surface file at once.

The migration backlog units are *surfaces*, but its completeness SHALL be verified
against the per-file debt the debt itself defines, so no surface is silently left
unowned. At the time the component layer lands the entire feature-surface debt resides
in exactly three files (whose counts sum to the baseline), hosting five surfaces:

- `TerminalPaneView.swift` — `ShellPalette.*` 82, `RoundedRectangle` 14 — hosts two
  surfaces: **terminal-pane SwiftUI chrome** and the **settings surface** (separate
  migration changes).
- `Views/Shell/ShellSidebarView.swift` — `ShellPalette.*` 49, `RoundedRectangle` 11 —
  hosts two surfaces: **sidebar** and **space slider** (separate migration changes).
- `MacShellRootView.swift` — `ShellPalette.*` 5, `RoundedRectangle` 4 — one surface:
  **root chrome** — the visual controls only (collapse/appearance/new-space controls,
  ghost chrome). Window placement (hidden-titlebar, minimum size, traffic-light
  metrics) is out of scope: it stays in the app/window support component
  (`Support/ShellWindowPlacement.swift`) under `macos-app-architecture-maintainability`
  and is not pulled into the component-layer migration.

(82+49+5 = 136 and 14+11+4 = 29, matching the recorded baseline; this sum is the
file-level completeness check. A file's debt is fully retired only once *all* of its
hosted surfaces are migrated, so the per-file arithmetic catches an omitted file while
the one-surface-per-change rule keeps each PR's screenshot diff isolated.)

#### Scenario: A surface migration change is proposed

- **WHEN** a change migrates a shell surface (terminal-pane SwiftUI chrome, settings
  surface, sidebar, space slider, or root chrome) to the component layer
- **THEN** it targets that single surface, not several surfaces — even when two
  surfaces live in the same feature file (e.g. terminal-pane chrome vs settings in
  `TerminalPaneView.swift`, or sidebar vs space slider in `ShellSidebarView.swift`)

#### Scenario: The backlog is checked for completeness

- **WHEN** the migration backlog is reviewed or a "counts reach zero" claim is made
- **THEN** every surface hosted by a shell feature file with non-zero feature-surface
  debt has its own owning migration change (five at landing: terminal-pane chrome,
  settings surface, sidebar, space slider, root chrome)
- **AND** the per-file feature-surface debt counts reconcile with the recorded
  baseline (their sum equals it), so no file — including `MacShellRootView.swift` — is
  left unowned

#### Scenario: A surface migration change is verified

- **WHEN** a surface migration change is reviewed
- **THEN** the count of inline `RoundedRectangle` shapes and direct `ShellPalette.*`
  references in the targeted surface trends toward zero (outside the design-system
  layer)
- **AND** screenshot comparison shows no unintended visual regression for that surface
- **AND** `just apple-shell-focused-tests` and the UI smoke check pass

#### Scenario: Terminal-host internals are encountered during migration

- **WHEN** a migration touches a file that also contains Ghostty/AppKit terminal-host
  internals (e.g. terminal surface input, attachment, overlay layout)
- **THEN** only the SwiftUI presentational chrome is migrated to primitives
- **AND** the terminal-host bridge behavior remains owned by the terminal-host
  boundary defined in `macos-app-architecture-maintainability` and is not folded into
  presentational primitives

#### Scenario: Legacy/mobile console surfaces are out of scope

- **WHEN** the component layer is adopted or migrated
- **THEN** the legacy/mobile remote-control console surfaces (`Views/Console/`) are
  NOT migrated through the shell design-system home
- **AND** they remain isolated from the primary macOS shell as required by
  `macos-app-architecture-maintainability`
