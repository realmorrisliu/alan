## MODIFIED Requirements

### Requirement: Visual system follows native material guidance
The default macOS shell SHALL use a light-mode-first native material visual
system that feels calm, precise, and terminal-oriented. It SHALL avoid
card-heavy dashboard composition, decorative gradients, hard-coded dominant
theme panels, and ornamental controls.

#### Scenario: Material sidebar
- **WHEN** the app window is visible in the default light appearance
- **THEN** the unified sidebar column, top Space slider, active-Space tab list,
  and compact sidebar controls use material-backed surfaces, subtle separators
  where useful, and restrained selection states rather than an opaque themed
  sidebar panel, separate Space rail, or bottom Space switcher

#### Scenario: Stable compact controls
- **WHEN** the user hovers, selects, inserts, closes, creates, or switches tabs
  and Spaces
- **THEN** rows, icon controls, counters, and status marks keep stable
  dimensions and do not resize the sidebar or terminal content
- **AND** the top Space slider aligns its rounded track to the sidebar edge
  inset so Space slider targets and tab rows share one optical column
- **AND** the titlebar New Space button directly creates a standard new Space
  instead of opening a menu of Space variants
- **AND** the titlebar New Space button is right-aligned within the sidebar
  titlebar instead of sitting immediately after the pin/unpin and appearance
  controls

#### Scenario: Space slider replaces header and dock
- **WHEN** the default sidebar displays Space navigation
- **THEN** Space navigation is presented as a continuous rounded top track whose
  selected Space is a compact liquid-glass tab inside that track
- **AND** inactive Spaces sit in the shared track background using icon and
  title foreground treatment rather than individual framed pills, cards, or
  dot-only indicators
- **AND** each Space target can show its Space icon with a title when width
  allows and can collapse to an icon-only circular target at minimum width
- **AND** alan does not show a separate bottom Space dock in the default sidebar
- **AND** the top Space slider remains a fixed sidebar-level control while
  Space tab content pages move beneath it during Space paging gestures

#### Scenario: Space profile uses context menu disclosure
- **WHEN** the default sidebar displays the selected Space
- **THEN** alan does not show an always-visible terminal profile selector in the
  Space slider or default sidebar
- **AND WHEN** the user opens a Space context menu from the selected Space title
  or a non-selected Space slider target
- **THEN** terminal profile selection is available as a Space-level context menu
  action

#### Scenario: New Tab belongs to ordinary tab flow
- **WHEN** the active Space contains pinned tabs
- **THEN** alan shows pinned tabs first, then a subtle divider, then the New Tab
  row, then unpinned tabs
- **AND WHEN** the active Space contains no pinned tabs
- **THEN** alan shows the New Tab row before unpinned tabs without a pinned-tab
  divider
- **AND** the New Tab row creates a normal unpinned terminal tab in the current
  Space

### Requirement: Space slider supports adaptive density and scrub navigation
The default macOS shell Space slider SHALL use a continuous rounded track that
adapts Space target widths to available sidebar space, supports every Space
without an arbitrary count cap, and preserves preview-first scrub navigation
without hover-driven geometry changes or cover-flow motion.

#### Scenario: Rounded track owns Space navigation
- **WHEN** the default sidebar displays one or more Spaces
- **THEN** the Space slider renders one rounded track as the shared navigation
  surface
- **AND** each Space is represented by a distinct target inside that track
- **AND** the selected Space uses the strongest selected-state treatment as a
  compact liquid-glass tab inside the track
- **AND** inactive Spaces remain visually embedded in the track background

#### Scenario: Space targets use icon and title when width allows
- **WHEN** a Space target has enough allocated width for its icon and title
- **THEN** alan shows the Space icon followed by a single-line title
- **AND** the title truncates before it wraps, overlaps adjacent targets, or
  changes sidebar width

#### Scenario: Space targets collapse to icon-only minimums
- **WHEN** the track cannot fit readable title labels for every Space
- **THEN** alan progressively truncates lower-priority Space titles
- **AND** alan may collapse any Space target to an icon-only circular target at
  its minimum width
- **AND** the icon-only target remains a distinct click, context-menu,
  keyboard, VoiceOver, and scrub target with its title exposed
  accessibly

#### Scenario: All Spaces participate without a nine-Space cap
- **WHEN** the user creates more than nine Spaces
- **THEN** alan includes every Space in the Space slider model
- **AND** creation affordances continue to produce additional Spaces instead of
  hiding or refusing the tenth Space solely because of slider capacity

#### Scenario: Overflow scrolls horizontally inside the track
- **WHEN** all Space targets are at their icon-only minimum width and the total
  target width still exceeds the available track width
- **THEN** the Space slider content scrolls horizontally within the rounded
  track
- **AND** alan keeps the selected Space visible when selection changes
- **AND** alan does not resize the sidebar, wrap targets to another row, or
  replace overflow Spaces with an unrelated menu

#### Scenario: Hover previews without geometry shifts
- **WHEN** the pointer hovers a non-selected Space in the slider
- **THEN** alan may apply subtle foreground, tint, or focus treatment to that
  Space target
- **AND** alan does not expand the target, scale it, fade neighboring targets,
  switch the selected Space, move the tab pager, or change focused terminal
  content merely because of hover

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

#### Scenario: Scrub uses stable track treatment
- **WHEN** reduced motion is disabled and Space scrub is active
- **THEN** the scrub-focused Space is emphasized through the same stable track
  target language used for hover, keyboard focus, and selected state
- **AND** nearby Spaces do not scale, fade by distance, shift width, or create a
  cover-flow or carousel effect
- **AND** the effect remains bounded inside the sidebar Space slider

#### Scenario: Scrub accounts for horizontal scroll offset
- **WHEN** the Space slider content is horizontally scrolled
- **AND** the user drag-scrubs or wheel-scrubs over the track
- **THEN** alan resolves the scrub-focused Space from the visible target frames
  and current horizontal scroll offset
- **AND** scrub preview, commit, cancel, and selected-Space visibility remain
  consistent with the visible track positions

#### Scenario: Vertical scrolling is preserved
- **WHEN** wheel or trackpad input over the Space slider is vertical or
  ambiguous between horizontal and vertical intent
- **THEN** alan does not enter Space scrub
- **AND** the tab list can continue receiving vertical scrolling behavior

#### Scenario: Space context menus remain available
- **WHEN** the user right-clicks or opens the context menu for a selected,
  hovered, keyboard-focused, or scrub-focused Space target
- **THEN** alan opens the context menu for that Space
- **AND** any active scrub preview is canceled before the context menu action
  changes Space-level settings

#### Scenario: Reduced motion preserves the state model
- **WHEN** reduced motion is enabled
- **THEN** hover and scrub use bounded foreground, tint, outline, or selected
  treatment without width changes, scale, spring, perspective-like motion, or
  cover-flow movement
- **AND** click, hover, scrub preview, commit, cancel, and keyboard behavior
  remain equivalent

#### Scenario: Keyboard and accessibility navigation remain explicit
- **WHEN** keyboard focus or VoiceOver reaches the Space slider
- **THEN** each Space is exposed as a distinct actionable target with its title,
  icon meaning when relevant, selected state, and tab count
- **AND** left and right keyboard navigation can move preview focus without
  immediately switching Spaces
- **AND** Enter commits the focused Space and Escape cancels preview focus

#### Scenario: Track remains borderless relative to the sidebar
- **WHEN** the top Space slider is visible
- **THEN** the track reads as lightweight native sidebar navigation rather than
  a nested card, dashboard section, or detached toolbar
- **AND** selected, hover, focus, and scrub states do not introduce notification
  dots, oversized badges, decorative shadows, or persistent framed cards for
  inactive Spaces
