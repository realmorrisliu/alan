## ADDED Requirements

### Requirement: Space Selection Restores Space-Local Tab Focus
The macOS shell SHALL remember the last selected Tab for each Space and SHALL
restore that Space-local Tab selection when the Space becomes selected again.
This behavior SHALL apply to sidebar Space clicks, bottom Space switcher
actions, committed sidebar swipe gestures, keyboard shortcuts, menu commands,
command routing, and control-plane Space selection. When the remembered Tab
contains multiple PaneSlots, alan SHALL prefer the last focused PaneSlot in that
Tab when it is still valid and SHALL otherwise focus the first valid PaneSlot in
the remembered Tab.

#### Scenario: Returning to a Space restores its selected Tab
- **WHEN** the user selects the second Tab in Space A
- **AND** the user switches to Space B
- **AND** the user switches back to Space A
- **THEN** alan selects the second Tab in Space A
- **AND** alan does not fall back to the first Tab in Space A
- **AND** terminal focus and render priority follow the restored Tab's focused PaneSlot when that PaneSlot mounts terminal content

#### Scenario: Keyboard Space switch uses remembered Tab
- **WHEN** Space A remembers a non-first selected Tab
- **AND** the user switches away from Space A with a keyboard Space command
- **AND** the user returns to Space A with a keyboard Space command
- **THEN** alan selects the remembered Tab in Space A through the same shell controller focus path used by sidebar selection

#### Scenario: Empty Space stays tabless
- **WHEN** the user selects a Space whose Tabs have all been closed or retired
- **THEN** alan selects that Space with no selected Tab
- **AND** alan does not fabricate a Tab, PaneSlot, or terminal runtime for that empty Space

#### Scenario: Invalid remembered Tab repairs on selection
- **WHEN** a Space remembers a Tab that no longer exists in that Space
- **AND** the Space still has at least one remaining Tab
- **THEN** selecting the Space focuses the first remaining Tab in that Space
- **AND** alan updates the Space-local remembered Tab to that remaining Tab

#### Scenario: Context target resolution does not mutate remembered selection
- **WHEN** the user opens a context menu for a background Tab
- **THEN** alan resolves the context action against that Tab without first making it the remembered selected Tab for its Space
- **AND** alan updates remembered selection only if the executed action has focus-changing semantics
