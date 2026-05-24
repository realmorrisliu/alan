## ADDED Requirements

### Requirement: Sidebar interactions do not drag the window
The macOS shell SHALL limit primary-window drag movement to explicit blank
titlebar/chrome areas. Sidebar workspace controls SHALL own their own
interactions and MUST NOT move the window when the user drags them.

#### Scenario: Tab row drag reorders without moving window
- **WHEN** the user drags a tab row in the sidebar tab list
- **THEN** alan treats the gesture as a tab selection, reorder, or drop-target
  interaction according to the tab-list contract
- **AND** the primary shell window does not move as part of that drag

#### Scenario: Space controls are interaction surfaces
- **WHEN** the user drags or clicks within the sidebar space switcher, command
  launcher, or sidebar titlebar controls
- **THEN** alan routes the event to the relevant sidebar control
- **AND** the primary shell window does not move because of window-background
  dragging

#### Scenario: Blank chrome still moves the window
- **WHEN** the user drags an empty, non-control area of the top titlebar/chrome
  region
- **THEN** alan moves the primary shell window using the native macOS window
  drag behavior
- **AND** double-clicking the same empty chrome region continues to toggle the
  visible-frame zoom behavior
