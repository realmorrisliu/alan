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
client. New and migrated shell feature surfaces SHALL obtain colors, typography, and
spacing from the semantic token namespaces in `Support/ShellDesignTokens.swift`
(`ShellPaper`, `ShellInk`, `ShellSignal`, `ShellPalette`, `ShellType`, `ShellSpacing`)
or by composing a primitive, and SHALL NOT hard-code raw styling literals. Referencing
a semantic token namespace — including `ShellPalette.*` — is the *compliant* outcome,
not debt; the debt is *raw literals* such as `Color(red:…)`/`NSColor(red:…)`
constructors, raw named hues (`Color.red`/`.orange`/`.green`/`.white`/`.black`),
`.font(.system(size:…))` typography, and hard-coded numeric `.padding(…)`. The
structural system colors `Color.clear`/`.primary`/`.secondary`/`.accentColor` and
semantic system fonts (`.headline`, `.body`, …) remain permitted.

The raw-literal floor SHALL be enforced by the project's existing design-token guard
(`scripts/check-shell-design-tokens.sh` with baseline
`scripts/shell-design-token-baseline.txt`), which this capability adopts rather than
re-defining a parallel count — the guard already ratchets per-file raw-literal counts
(`system(size:`, `Color(red:`, `NSColor(red:`, numeric `.padding(`) and SHALL be run
in CI. Raw-literal forms the guard does not yet match (`RoundedRectangle` with a
literal radius, raw named hues like `Color.red`) SHALL be caught in review and MAY be
folded into the guard as an extension; they are not a separate ratchet owned by this
spec.

#### Scenario: A new or migrated feature surface needs a color, font, or metric

- **WHEN** new shell code, or a surface being migrated, needs a background color,
  selection tint, font size, corner radius, spacing, or border treatment
- **THEN** it obtains it by composing a design-system primitive or referencing a
  semantic token namespace (`ShellPaper`/`ShellInk`/`ShellSignal`/`ShellPalette`/
  `ShellType`/`ShellSpacing`)
- **AND** it does not hard-code a raw literal (`Color(red:…)`/`Color.red`/
  `.font(.system(size:…))`/numeric `.padding(…)`); `Color.clear`/`.primary`/
  `.secondary`/`.accentColor` and semantic system fonts are allowed

#### Scenario: A new token is needed by a feature

- **WHEN** an existing semantic token does not cover a feature's need
- **THEN** the new value is added to the design-system token layer with a semantic
  name (e.g. `sidebarSelection`, not `blue500`)
- **AND** the feature consumes the named token rather than inlining the literal value

#### Scenario: The raw-literal guard runs in CI

- **WHEN** CI runs for a change touching the macOS client
- **THEN** the design-token guard (`scripts/check-shell-design-tokens.sh`) is executed
  as a blocking check, not only available as a local `just` recipe
- **AND** a change that raises a file's raw-literal count above its recorded baseline
  fails CI

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

Pre-existing debt SHALL be tracked against two invariants, neither of which treats
semantic-token usage as debt. First, the raw-literal floor: the project design-token
guard (`scripts/check-shell-design-tokens.sh`, baseline
`scripts/shell-design-token-baseline.txt`) ratchets per-file raw-literal counts and
SHALL NOT regress — its recorded baseline at the time the component layer lands is
`MacShellRootView.swift` 1, `TerminalPaneView.swift` 63, `ShellSidebarView.swift` 16
(and console files 19 / 86, which this capability does not own). Second, no shell
feature file SHALL introduce a *new* local struct that duplicates a design-system
primitive's role (row, card, chip, badge, button, field, section label, or
surface/background modifier); feature-specific composite views that do not duplicate a
primitive role (e.g. layout, split, title-bar, or find-bar views) are unaffected and
legitimately stay in feature files.

`ShellPalette.*` (and the other semantic token namespaces) is *not* debt — referencing
it is the compliant outcome. Each strangler-fig migration SHALL lower the guard's
per-file raw-literal baseline for its targeted surface (running
`check-shell-design-tokens.sh --update-baseline` after a reviewed reduction) and fold
the known primitive-role duplicates, until the surface composes primitives and carries
no raw literals.

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

#### Scenario: Migration debt is enumerated when the component layer lands

- **WHEN** the component layer is introduced (its foundation change)
- **THEN** the shell surfaces still carrying raw-literal debt are exactly those in the
  design-token guard baseline (`MacShellRootView.swift`, `TerminalPaneView.swift`,
  `ShellSidebarView.swift`), recorded as the migration backlog (one follow-up change
  per surface), and the known primitive-role duplicate list above is captured
- **AND** the long-lived spec is true at that point: it requires *new and migrated*
  code to comply and forbids *new* violations, not that all debt is already cleared

#### Scenario: A change would add new raw styling or a primitive-role duplicate

- **WHEN** a change adds, to a shell feature surface instead of composing a primitive,
  a new raw-literal styling value (`Color(red:…)`/`Color.red`/`.font(.system(size:…))`/
  numeric `.padding(…)`/`RoundedRectangle` with a literal radius) or a new local struct
  that duplicates a primitive's role (row/card/chip/badge/button/field/label/surface
  treatment)
- **THEN** it is rejected: the design-token guard's per-file count SHALL NOT regress
  (for the forms it matches), the review rejects the forms it does not yet match, and
  no new primitive-role duplicate is added
- **AND** the developer composes the existing primitive, references a semantic token,
  or adds a primitive to the design-system home

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
against the design-token guard's per-file raw-literal baseline, so no surface is
silently left unowned. At the time the component layer lands the shell raw-literal debt
resides in exactly three files (the guard baseline, console excluded), hosting five
surfaces:

- `TerminalPaneView.swift` — guard baseline **63** — hosts two surfaces:
  **terminal-pane SwiftUI chrome** and the **settings surface** (separate migration
  changes).
- `Views/Shell/ShellSidebarView.swift` — guard baseline **16** — hosts two surfaces:
  **sidebar** and **space slider** (separate migration changes).
- `MacShellRootView.swift` — guard baseline **1** — one surface: **root chrome** — the
  visual controls only (collapse/appearance/new-space controls, ghost chrome). Window
  placement (hidden-titlebar, minimum size, traffic-light metrics) is out of scope: it
  stays in the app/window support component (`Support/ShellWindowPlacement.swift`)
  under `macos-app-architecture-maintainability` and is not pulled into the
  component-layer migration.

(The completeness check is the design-token guard baseline file itself: every shell
feature file it lists with a non-zero count must have an owning surface migration, and
a file's entry reaches 0 only once *all* its hosted surfaces are migrated. The
one-surface-per-change rule keeps each PR's screenshot diff isolated even when two
surfaces share a file.)

#### Scenario: A surface migration change is proposed

- **WHEN** a change migrates a shell surface (terminal-pane SwiftUI chrome, settings
  surface, sidebar, space slider, or root chrome) to the component layer
- **THEN** it targets that single surface, not several surfaces — even when two
  surfaces live in the same feature file (e.g. terminal-pane chrome vs settings in
  `TerminalPaneView.swift`, or sidebar vs space slider in `ShellSidebarView.swift`)

#### Scenario: The backlog is checked for completeness

- **WHEN** the migration backlog is reviewed or a "counts reach zero" claim is made
- **THEN** every shell feature file with a non-zero design-token guard baseline has an
  owning surface migration (five surfaces at landing: terminal-pane chrome, settings
  surface, sidebar, space slider, root chrome)
- **AND** the guard baseline file is the reconciliation point, so no file — including
  `MacShellRootView.swift` — is left unowned

#### Scenario: A surface migration change is verified

- **WHEN** a surface migration change is reviewed
- **THEN** the design-token guard's per-file raw-literal count for the targeted surface
  is reduced (via `--update-baseline` after a reviewed reduction) and `ShellPalette`/
  token usage replaces raw literals
- **AND** screenshot comparison shows no unintended visual regression for that surface
- **AND** `just apple-shell-focused-tests`, `just guard-shell-design-tokens`, and the
  UI smoke check pass

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
