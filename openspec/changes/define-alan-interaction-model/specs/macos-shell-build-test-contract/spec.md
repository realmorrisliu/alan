## MODIFIED Requirements

### Requirement: UI smoke coverage is repeatable
The Apple client SHALL provide a repeatable UI smoke or screenshot flow for
launch, space/tab switching, split creation, command UI, pane-scoped Find
behavior, and basic terminal input when terminal runtime is available.

#### Scenario: Launch smoke
- **WHEN** the UI smoke flow starts the macOS app
- **THEN** it verifies that the default light-mode window shows the unified sidebar column, top Space slider, active-space tab list, the workspace home content area as the selected content, and no persistent inspector pane or toggle

#### Scenario: Split smoke
- **WHEN** the UI smoke flow creates a split
- **THEN** it verifies that multiple panes are visible and no raw pane IDs or debug labels dominate the default UI

#### Scenario: Find smoke
- **WHEN** the UI smoke flow opens pane-scoped Find for the focused terminal pane
- **THEN** it verifies that a focused text field appears in pane chrome, result feedback is visible without raw debug labels, and dismissing Find returns focus to the owning terminal surface
