## MODIFIED Requirements

### Requirement: Core shell actions have App Intents
alan's macOS app SHALL provide App Intents for creating terminal tabs, splitting
panes, focusing panes, closing panes or tabs, sending text, reading pane
summaries, and opening attention items.

#### Scenario: Create terminal tab intent
- **WHEN** the user runs the create terminal tab intent
- **THEN** alan creates a terminal tab through the shell controller and returns the created tab summary

#### Scenario: Split pane intent
- **WHEN** the user runs a split pane intent with a direction and target pane
- **THEN** alan performs the same split mutation as the native command path and returns the resulting focused pane

#### Scenario: Send text intent
- **WHEN** the user runs a send text intent for a target pane
- **THEN** alan routes delivery through the terminal runtime service and reports accepted, queued, or rejected state truthfully

#### Scenario: Open attention item intent
- **WHEN** the user runs an intent for an attention item
- **THEN** alan activates the owning window, space, tab, and pane without exposing raw debug identifiers in the result text

## ADDED Requirements

### Requirement: Create Alan Tab automation is absent
The macOS shell automation surface SHALL NOT expose first-party alan tab
creation through App Intents, automation helpers, or default automation
metadata.

#### Scenario: App Intents are inspected
- **WHEN** App Intent metadata is generated or inspected
- **THEN** it does not include Create Alan Tab or a first-party alan tab launch
  target

#### Scenario: Automation helpers are enumerated
- **WHEN** automation helper APIs and intent routers are inspected
- **THEN** they do not provide `createAlanTab` or equivalent first-party alan
  tab creation helpers
