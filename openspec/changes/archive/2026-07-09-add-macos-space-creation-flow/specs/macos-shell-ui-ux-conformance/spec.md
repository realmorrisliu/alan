## MODIFIED Requirements

### Requirement: Space slider supports adaptive density and scrub navigation
The default macOS shell Space slider SHALL use a continuous rounded track that
adapts Space target widths to available sidebar space, supports every Space
without an arbitrary count cap, and preserves preview-first scrub navigation
without hover-driven geometry changes or cover-flow motion. Manual Space
creation SHALL use a deliberate in-sidebar creation form so a Space is named
and given an identity at birth; programmatic creation SHALL remain instant with
a name derived from its working directory.

#### Scenario: Titlebar New Space opens the creation form
- **WHEN** the user activates the titlebar New Space control
- **THEN** the Space slider stays visible with a new selected draft target
  appended at the end, whose glyph reflects the live monogram of the typed name
  or the chosen curated symbol
- **AND** only the tab-list region below the slider (not the whole sidebar)
  hosts the creation form: a required name field, an inline curated icon
  selection (defaulting to the name monogram), and a terminal profile selector
- **AND** the draft target is display-only: it does not create a real Space or
  terminal, and slider tap/scrub/keyboard selection does not move off it while
  the form is open
- **AND** the form does not offer a menu of Space variants or types
- **AND** Create is unavailable until a non-empty name is entered
- **AND** Cancel or Escape removes the draft target and restores the prior
  selection without creating a Space

#### Scenario: Programmatic creation stays instant and self-named
- **WHEN** a Space is created programmatically (CLI, worktree, or API) with a
  working directory and no explicit title
- **THEN** alan creates the Space immediately without showing the form
- **AND** the Space name is derived from the working directory leaf rather than
  a generic "Space N" label, falling back to the indexed label only when no
  working directory is available
