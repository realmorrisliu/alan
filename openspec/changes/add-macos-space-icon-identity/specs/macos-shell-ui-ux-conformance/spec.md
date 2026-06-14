## MODIFIED Requirements

### Requirement: Space slider supports adaptive density and scrub navigation
The default macOS shell Space slider SHALL use a continuous rounded track that
adapts Space target widths to available sidebar space, supports every Space
without an arbitrary count cap, and preserves preview-first scrub navigation
without hover-driven geometry changes or cover-flow motion. Each Space target
SHALL carry a per-Space icon identity so targets are tellable apart, including
at icon-only minimum width: a title-derived monogram by default, or a
user-chosen curated SF Symbol when set.

#### Scenario: Space target shows a monogram by default
- **WHEN** a Space has no user-set presentation icon
- **THEN** its slider target renders a monogram derived from the Space title's
  first character (Latin letters uppercased; other scripts use the first
  grapheme), in the same metrics and foreground treatment as a symbol icon
- **AND** the monogram is icon-foreground only, with no filled pill or added
  background, so the slider stays quiet
- **AND** a Space with an empty or untitled name falls back to a neutral
  symbol rather than an empty target

#### Scenario: User sets a Space icon
- **WHEN** the user chooses an icon for a Space from the Space context menu
- **THEN** the slider target renders the chosen curated SF Symbol in place of
  the monogram
- **AND** the choice persists with the Space through the workspace manifest
- **AND** choosing the default entry clears the icon back to the monogram

#### Scenario: Icon identity yields to action signals
- **WHEN** a Space whose attention requires the user is shown in the slider
- **THEN** the action color takes precedence over the icon-foreground identity
  treatment, consistent with the signal semantics

#### Scenario: Icon picker is curated and restrained
- **WHEN** the Space context menu exposes icon selection
- **THEN** it offers a curated set of workspace-relevant SF Symbols plus a
  default (monogram) entry, rather than an unbounded symbol browser
- **AND** the active choice is marked consistently with other Space-level
  context-menu selections
