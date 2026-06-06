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
- **WHEN** an ordinary sidebar tab has required or useful secondary metadata
  such as actionable status, activity, branch, folder, process, or content kind
- **THEN** the row displays the title and secondary metadata as two lines within
  the compact tab row system
- **AND** the row does not resize the sidebar or shift adjacent rows during
  hover, selection, close-affordance display, or activity progress updates

#### Scenario: Provided task title identifies agent work
- **WHEN** an unlocked sidebar tab has a terminal-provided or agent-provided
  task title that describes the work being done
- **THEN** alan uses that task title as the primary row title instead of falling
  back to the repository, directory, process, or agent name
- **AND** state labels such as running, thinking, failed, or input needed do not
  replace the task title

#### Scenario: User-edited title is locked
- **WHEN** the user manually edits a sidebar tab title
- **THEN** alan treats the title as locked
- **AND** terminal, agent, activity, repository, process, or status updates do
  not overwrite that locked title

#### Scenario: Subtitle is required for actionable state
- **WHEN** a tab has an actionable or exceptional state such as input needed,
  failed, paused, exited, renderer failed, read-only, starting, or high-priority
  activity in another pane
- **THEN** alan shows a subtitle with the actionable state as the first token
- **AND** the subtitle may include context tokens after the state when space
  allows

#### Scenario: Subtitle disambiguates provided task titles
- **WHEN** a tab title is a terminal-provided or agent-provided task title and
  context is available
- **THEN** alan shows a subtitle that starts with stable project, repository,
  worktree, directory, or branch context
- **AND** the subtitle may include agent, process, command, progress, or split
  context after the stable location token

#### Scenario: Subtitle is hidden for fallback metadata
- **WHEN** a tab has no actionable state and its secondary metadata is only a
  fallback type, default shell process, duplicate directory, or otherwise
  non-disambiguating label
- **THEN** alan renders the row in single-line mode without a visible subtitle

#### Scenario: Leading split indicator remains structural
- **WHEN** a tab has one or more panes
- **THEN** the leading sidebar indicator continues to represent pane topology
  and focused-pane interaction
- **AND** alan does not replace the leading split indicator with agent, process,
  status, or content-type glyphs

#### Scenario: Trailing accessory shows state until close is needed
- **WHEN** a tab has a non-idle state that is useful for scanning
- **THEN** alan shows a compact state glyph or progress affordance in the
  trailing accessory slot while the row is not hovered
- **AND WHEN** the pointer hovers the row or keyboard focus reaches it
- **THEN** the trailing accessory can become the close button without removing
  required state text from the subtitle or accessibility label

#### Scenario: Idle trailing accessory is quiet
- **WHEN** a tab has no useful scanning state
- **THEN** alan keeps the trailing accessory slot visually quiet until hover or
  keyboard focus reveals the close button

#### Scenario: Pinned state is conveyed by section position
- **WHEN** a tab is pinned in the default sidebar
- **THEN** alan displays it in the pinned tab section above the New Tab row and
  divider
- **AND** alan does not show a separate inline pin glyph in the tab row title or
  trailing accessory area
- **AND** pin and unpin actions remain available through existing command and
  context-menu surfaces

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
