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
- **THEN** rows, icon controls, dots, counters, and status marks keep stable
  dimensions and do not resize the sidebar or terminal content
- **AND** the top Space slider aligns its visible controls to the sidebar edge
  inset so the title, dots, and tab rows share one optical column
- **AND** the titlebar New Space button directly creates a standard new Space
  instead of opening a menu of Space variants
- **AND** the titlebar New Space button is right-aligned within the sidebar
  titlebar instead of sitting immediately after the pin/unpin and appearance
  controls

#### Scenario: Space slider replaces header and dock
- **WHEN** the default sidebar displays Space navigation
- **THEN** the selected Space is represented by its title in the top Space
  slider, non-selected Spaces are represented by dots, and no Space icon is
  shown in the slider
- **AND** alan does not show a separate bottom Space dock in the default sidebar
- **AND** the top Space slider remains a fixed sidebar-level control while
  Space tab content pages move beneath it during Space paging gestures

#### Scenario: Space profile uses context menu disclosure
- **WHEN** the default sidebar displays the selected Space
- **THEN** alan does not show an always-visible terminal profile selector in the
  Space header or Space slider
- **AND WHEN** the user opens a Space context menu from the selected Space title
  or a non-selected Space dot
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
