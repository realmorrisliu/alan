## ADDED Requirements

### Requirement: Root shell backing uses an opaque native base
The default macOS shell SHALL paint its primary root backing surface with an
opaque adaptive native base color before content-specific surfaces are rendered.

#### Scenario: Light appearance root backing
- **WHEN** the default macOS shell window is visible in light appearance
- **THEN** the root backing surface uses `rgb(1,1,1)` as its base color
- **AND** the root backing surface does not depend on wallpaper blending,
  `NSVisualEffectView`, root-level transparency, or a root-level gradient wash

#### Scenario: Dark appearance root backing
- **WHEN** the default macOS shell window is visible in dark appearance
- **THEN** the root backing surface uses a solid adaptive dark base color
- **AND** light-mode tint, root-level material wash, and wallpaper-dependent
  transparency do not determine the dark appearance backing color

#### Scenario: Root backing is separate from future material surfaces
- **WHEN** sidebar, floating overlay, command palette, or content-specific
  material treatments are evaluated
- **THEN** their material behavior remains owned by their own surface roles
- **AND** the root backing surface does not force those surfaces to use root
  material, root transparency, or root gradient treatment

### Requirement: Empty Spaces render as workspace placeholders
The default macOS shell SHALL render a selected Space with no mounted content as
a generic workspace placeholder rather than as an empty terminal surface.

#### Scenario: Empty Space placeholder in light appearance
- **WHEN** the selected Space has no selected tab, pane tree, or mounted content
  in light appearance
- **THEN** alan shows an empty workspace placeholder with adaptive light-mode
  text and control treatment
- **AND** the placeholder is not painted with the terminal dark canvas,
  terminal rim, or terminal surface shadow

#### Scenario: Empty Space placeholder in dark appearance
- **WHEN** the selected Space has no selected tab, pane tree, or mounted content
  in dark appearance
- **THEN** alan shows the same empty workspace placeholder using adaptive
  dark-mode text and control treatment
- **AND** the placeholder remains readable without reusing terminal-only color
  assumptions

#### Scenario: Empty Space primary action remains terminal-first
- **WHEN** the user activates the empty Space `New Tab` action
- **THEN** alan creates a normal terminal tab in the current Space
- **AND** the new tab becomes selected through the existing tab creation path

### Requirement: Terminal surface styling is content-scoped
The default macOS shell SHALL apply terminal canvas and terminal-surface chrome
only to terminal content rendering, not to the generic workspace canvas or
non-terminal content.

#### Scenario: Terminal content owns the terminal surface
- **WHEN** a mounted content leaf has render kind `terminal`
- **THEN** alan renders the terminal dark canvas and terminal-specific chrome
  for that terminal content
- **AND** terminal input, search, paste, runtime metadata, and terminal lifecycle
  behavior remain scoped to the terminal content instance

#### Scenario: Markdown and settings do not inherit terminal styling
- **WHEN** a mounted content leaf has render kind `markdown` or `settings`
- **THEN** alan renders that content through its bounded non-terminal content
  surface
- **AND** alan does not create a terminal runtime or apply terminal-only dark
  canvas assumptions to that content leaf

#### Scenario: Workspace canvas does not imply terminal content
- **WHEN** the shell renders the shared workspace canvas, split layout bounds,
  or an unavailable non-terminal placeholder
- **THEN** alan does not treat that workspace canvas as terminal content
- **AND** terminal-specific styling appears only after a terminal content leaf
  is mounted or restored
