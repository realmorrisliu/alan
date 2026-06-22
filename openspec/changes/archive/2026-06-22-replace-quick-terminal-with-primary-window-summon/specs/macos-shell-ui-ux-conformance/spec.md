## ADDED Requirements

### Requirement: Primary Window Summon Is Native And Non-Disruptive
Primary Window Summon SHALL feel like native macOS window activation for the app's
single primary shell window and SHALL preserve the existing terminal-first shell
composition.

#### Scenario: Primary shell window is summoned
- **WHEN** the user invokes Primary Window Summon
- **THEN** the app brings the normal primary shell window forward with its existing
  sidebar, selected Space, selected Tab, selected PaneSlot, and content area
- **AND** the app does not present a detached Peak, overlay terminal, duplicate
  sidebar, dashboard header, or floating-card terminal surface

#### Scenario: Summon happens from another macOS Space
- **WHEN** the user invokes Primary Window Summon from another macOS Space or
  display
- **THEN** the app attempts to bring the primary shell window to the current active
  desktop context
- **AND** if that movement cannot be guaranteed, the app still activates and
  foregrounds the primary shell window without changing shell workspace
  selection

#### Scenario: Existing shell geometry is preserved
- **WHEN** Primary Window Summon completes
- **THEN** the app preserves sidebar state, tab rows, split geometry, pane zoom
  state, terminal runtime identity, and content runtime identity

## REMOVED Requirements

### Requirement: Quick Terminal Presentation Is Native And Lightweight
**Reason**: Quick Terminal Peak presentation is removed from the product.
Primary Window Summon uses the normal primary shell window instead.

**Migration**: Delete detached Peak UI, Quick Terminal content view, hide/show
presentation behavior, `Open in Space` affordance, and hidden quick-terminal
activity presentation.

#### Scenario: Quick terminal appears
- **WHEN** the former quick-terminal shortcut is invoked
- **THEN** the app does not present a focused detached terminal surface

#### Scenario: Quick terminal appears outside the main window
- **WHEN** the former quick-terminal shortcut is invoked from any macOS Space
- **THEN** Alan summons the primary shell window instead of presenting a Peak
  outside the main window

#### Scenario: Quick terminal hides
- **WHEN** the former quick-terminal shortcut is invoked while the app is visible
- **THEN** the app does not hide a detached quick-terminal presentation

#### Scenario: Terminal keys remain terminal keys
- **WHEN** selected terminal content owns focus in the primary shell window
- **THEN** terminal key handling follows the normal terminal-host contract
  without quick-terminal escape policy

#### Scenario: Focus changes outside the Peak
- **WHEN** the app loses focus
- **THEN** there is no Quick Terminal Peak whose visibility must be preserved
  or hidden

#### Scenario: Quick terminal can become a normal tab
- **WHEN** Quick Terminal UI affordances are enumerated
- **THEN** the app does not expose an `Open in Space` promotion affordance

#### Scenario: Quick terminal activity exists
- **WHEN** terminal activity exists in the app
- **THEN** it is associated with normal terminal content and surfaces through
  normal activity policy, not hidden Quick Terminal state
