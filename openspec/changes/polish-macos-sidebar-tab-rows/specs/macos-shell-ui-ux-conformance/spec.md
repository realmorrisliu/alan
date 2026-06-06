## ADDED Requirements

### Requirement: Sidebar tab rows use compact Arc-like behavior
The default macOS shell sidebar SHALL render New Tab, Clear, and ordinary tab
rows with compact, stable geometry that supports quick scanning without making
the terminal sidebar feel like a dashboard or debug surface.

#### Scenario: New Tab idle state is quiet
- **WHEN** the active Space tab list displays the New Tab row and the pointer
  and keyboard focus are elsewhere
- **THEN** the New Tab row uses muted icon and text treatment without a
  persistent row background
- **AND** the New Tab row shares the same row metric system as ordinary sidebar
  tab rows

#### Scenario: New Tab hover and focus state
- **WHEN** the pointer hovers the New Tab row or keyboard focus reaches it
- **THEN** the New Tab row shows a full-width rounded material hover background
  within the sidebar row bounds
- **AND** the hover or focus state does not select a tab, scroll the list, or
  preview a Space

#### Scenario: New Tab creates ordinary unpinned tab
- **WHEN** the user activates the New Tab row
- **THEN** alan creates a normal unpinned terminal tab in the current Space
- **AND** the new tab becomes selected through the existing tab creation
  behavior

#### Scenario: Ordinary tab row uses single-line layout
- **WHEN** an ordinary sidebar tab has no meaningful subtitle
- **THEN** the row displays the title as a single line vertically centered in
  the compact tab row
- **AND** the row does not reserve visible subtitle space for fallback or
  duplicate metadata

#### Scenario: Ordinary tab row uses two-line layout
- **WHEN** an ordinary sidebar tab has meaningful secondary metadata such as
  task status, activity, branch, folder, process, or content kind
- **THEN** the row displays the title and secondary metadata as two lines within
  the compact tab row system
- **AND** the row does not resize the sidebar or shift adjacent rows during
  hover, selection, close-affordance display, or activity progress updates

#### Scenario: Clear appears only for eligible temporary tabs
- **WHEN** the active Space has at least one unpinned tab that is not selected
  and whose active task state does not protect it from pruning
- **THEN** alan shows a subtle Clear affordance in the divider/control row above
  New Tab
- **AND** the Clear affordance remains secondary to the New Tab row and ordinary
  tab rows

#### Scenario: Clear is hidden when no tab can be cleared
- **WHEN** the active Space has no eligible inactive unpinned tabs
- **THEN** alan does not show a disabled or persistent Clear affordance in the
  default sidebar

#### Scenario: Clear closes only inactive temporary tabs
- **WHEN** the user activates Clear
- **THEN** alan closes eligible inactive unpinned tabs in the current Space as a
  single cleanup operation
- **AND** alan keeps pinned tabs, the selected tab, tabs in other Spaces, and
  tabs whose active task state protects them from pruning
- **AND** alan preserves valid selected tab and pane focus after cleanup

#### Scenario: Drag insertion follows compact row geometry
- **WHEN** the user drags a tab over the compact sidebar tab list
- **THEN** insertion target calculation uses the compact row midpoint rather
  than the former taller-row midpoint
- **AND** hover, selected, close, and progress states do not change the
  insertion hit geometry

#### Scenario: Tab drag carries source identity through the drop session
- **WHEN** the user starts dragging a sidebar tab row
- **THEN** the drag session carries the dragged tab identity and source
  organization location as part of the drag payload or an equivalent
  session-scoped source record
- **AND** the drop target does not depend solely on transient hover or row
  gesture state that can be cleared before the drop is performed

#### Scenario: Dropping a tab reorders the sidebar
- **WHEN** the user drops a dragged tab onto a valid pinned or unpinned sidebar
  insertion target in the current Space
- **THEN** alan routes the drop through the same host reorder operation used by
  command and automation paths
- **AND** the tab order, pin state, selected tab, and pane identity match the
  requested insertion target

#### Scenario: Invalid tab drop leaves order unchanged
- **WHEN** the user drops a tab payload with a missing tab, stale source
  location, incompatible section, or invalid target index
- **THEN** alan rejects the drop without changing tab order, pin state, selected
  tab, or pane identity
- **AND** alan clears any insertion preview state after the rejected drop
