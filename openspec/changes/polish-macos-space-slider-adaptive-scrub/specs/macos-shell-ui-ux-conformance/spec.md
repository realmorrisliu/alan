## ADDED Requirements

### Requirement: Space slider supports adaptive density and scrub navigation
The default macOS shell Space slider SHALL adapt its visual density to the
number of Spaces and SHALL support deliberate preview-first scrub navigation
without making the default sidebar visually noisy or unstable.

#### Scenario: Low-density Space slider shows full titles
- **WHEN** the default sidebar has 1 to 3 Spaces
- **THEN** the Space slider shows every Space as a named, single-line
  Safari-like tab or pill control
- **AND** the selected Space has the strongest material and text treatment
- **AND** inactive Spaces remain readable without using Space icons

#### Scenario: Mid-density Space slider shows active and short titles
- **WHEN** the default sidebar has 4 to 6 Spaces
- **THEN** the Space slider shows the selected Space with its full title
- **AND** every inactive Space remains visible as a compact short-title control
- **AND** inactive Space titles truncate on one line before they resize the
  sidebar, wrap text, or overlap adjacent controls

#### Scenario: High-density Space slider uses active title and indicators
- **WHEN** the default sidebar has 7 to 9 Spaces
- **THEN** the Space slider shows the selected Space as a title control
- **AND** inactive Spaces render as compact indicators in the default idle state
- **AND** hovered or scrub-focused inactive indicators can expand into short
  title controls without changing slider height or sidebar width

#### Scenario: Space count is capped at nine
- **WHEN** the user attempts to create Spaces from the default macOS shell
- **THEN** alan allows at most 9 Spaces in the Space slider model
- **AND** creation affordances do not produce a tenth visible Space in the
  default sidebar

#### Scenario: Hover previews without switching
- **WHEN** the pointer hovers a non-selected Space in the slider
- **THEN** alan locally highlights or expands that Space according to the active
  density tier
- **AND** alan does not switch the selected Space, move the tab pager, or change
  focused terminal content merely because of hover

#### Scenario: Click switching remains immediate
- **WHEN** the user clicks a non-selected Space in the slider
- **THEN** alan immediately selects that Space
- **AND WHEN** the user clicks the selected Space
- **THEN** alan keeps the current selection unchanged

#### Scenario: Scrub previews before commit
- **WHEN** the user press-drags horizontally on the Space slider or sends clear
  horizontal wheel or trackpad input while hovering the slider
- **THEN** alan enters a scrub preview state with a focused target Space
- **AND** alan distinguishes the scrub focus from the currently selected Space
  when they differ
- **AND** alan does not commit Space selection until drag release or a short
  dwell after wheel or trackpad input stops

#### Scenario: Scrub uses lightweight cover-flow motion
- **WHEN** reduced motion is disabled and Space scrub is active
- **THEN** the scrub-focused Space is emphasized as the largest title control
- **AND** nearby Spaces scale or fade by distance to communicate direction
- **AND** the effect remains bounded inside the sidebar Space slider rather than
  introducing a large carousel or decorative overlay

#### Scenario: Vertical scrolling is preserved
- **WHEN** wheel or trackpad input over the Space slider is vertical or
  ambiguous between horizontal and vertical intent
- **THEN** alan does not enter Space scrub
- **AND** the tab list can continue receiving vertical scrolling behavior

#### Scenario: Space context menus remain available
- **WHEN** the user right-clicks or opens the context menu for a selected,
  hovered, or scrub-focused Space target
- **THEN** alan opens the context menu for that Space
- **AND** any active scrub preview is canceled before the context menu action
  changes Space-level settings

#### Scenario: Reduced motion preserves the state model
- **WHEN** reduced motion is enabled
- **THEN** hover and scrub use bounded highlight, opacity, and width changes
  instead of cover-flow scale, springy movement, or perspective-like motion
- **AND** click, hover, scrub preview, commit, cancel, and keyboard behavior
  remain equivalent

#### Scenario: Keyboard and accessibility navigation remain explicit
- **WHEN** keyboard focus or VoiceOver reaches the Space slider
- **THEN** each Space is exposed as a distinct actionable target with its title,
  selected state, and tab count
- **AND** left and right keyboard navigation can move preview focus without
  immediately switching Spaces
- **AND** Enter commits the focused Space and Escape cancels preview focus
