## ADDED Requirements

### Requirement: Presentational primitives live in a single design-system home

The macOS SwiftUI client SHALL keep reusable presentational primitives — surfaces,
controls, rows, indicators, and labels — together in one design-system home under
`clients/apple/alan-macos/Views/Shell/Components/`, absorbing the existing
`Views/Shell/Controls/` library. Feature surfaces SHALL compose these primitives
rather than redefining equivalent presentational structs locally.

#### Scenario: A reusable presentational primitive is added

- **WHEN** a developer adds a button, field, row, card, chip, badge, tile, or
  section-label primitive intended for reuse across surfaces
- **THEN** it is defined in the design-system home under `Views/Shell/Components/`
- **AND** it is `internal` or `public` (not `private` to a feature file) so other
  surfaces can compose it

#### Scenario: A feature file declares a presentational struct

- **WHEN** a feature surface file (e.g. sidebar, console, settings, workspace) needs
  a row/card/chip/badge/field/button treatment
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
color/number tuples or reference `ShellPalette.*` directly. Feature surfaces SHALL
consume semantic tokens or primitives instead of raw styling values.

#### Scenario: A feature surface needs a color or shape metric

- **WHEN** a feature surface needs a background color, selection tint, corner radius,
  spacing, or border treatment
- **THEN** it obtains it by composing a design-system primitive or referencing a
  semantic token exposed for that purpose
- **AND** it does not read a raw `(Double, Double, Double)` tuple or reach into
  `ShellPalette.*` directly

#### Scenario: A new token is needed by a feature

- **WHEN** an existing semantic token does not cover a feature's need
- **THEN** the new value is added to the design-system token layer with a semantic
  name (e.g. `sidebarSelection`, not `blue500`)
- **AND** the feature consumes the named token rather than inlining the literal value

### Requirement: Style is separated from structure

Reusable appearance behavior SHALL be expressed as `ButtonStyle`,
`TextFieldStyle`, `LabelStyle`, or `ViewModifier` types owned by the design-system
layer — this covers press feedback, hover state, focus ring, and field chrome.
Feature surfaces SHALL NOT
define their own ad-hoc style or modifier types for these concerns, and primitives
SHALL share one canonical style per concern rather than duplicating it.

#### Scenario: Press or hover feedback is needed

- **WHEN** a control needs press-scale, hover, or focus feedback
- **THEN** it applies the shared design-system style/modifier for that concern
- **AND** the codebase does not contain multiple near-duplicate `ButtonStyle`
  implementations for the same press/hover behavior

#### Scenario: Field chrome is needed

- **WHEN** a text-entry control needs border/background/focus chrome
- **THEN** it uses the canonical design-system field primitive or its shared style
- **AND** it does not introduce a parallel field-style type that duplicates the
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

### Requirement: Surfaces adopt the component layer via measurable strangler-fig migration

Existing surfaces SHALL be migrated to the component layer one surface at a time, as
separate changes, with each migration reducing inline styling in the targeted
surface and preserving its visual behavior. The component layer SHALL NOT be adopted
through a single big-bang rewrite of multiple surfaces at once.

#### Scenario: A surface migration change is proposed

- **WHEN** a change migrates a surface (e.g. sidebar, space slider, console, settings
  panel, terminal-pane SwiftUI chrome) to the component layer
- **THEN** it targets a single surface, not several unrelated surfaces in one change

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
