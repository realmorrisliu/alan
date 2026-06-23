## REMOVED Requirements

### Requirement: Quick Terminal Uses Normal Terminal Runtime Ownership
**Reason**: Standalone Quick Terminal is removed. The former shortcut now
summons Alan's primary shell window rather than presenting a detached terminal
runtime.

**Migration**: Remove Quick Terminal runtime slot, show/hide/close/promote
operations, and Peak presentation wiring. Use Primary Window Summon for global
window access.

#### Scenario: Quick terminal runtime ownership is removed
- **WHEN** the implementation is updated for Primary Window Summon
- **THEN** Alan no longer creates, hides, closes, preserves, or promotes a
  Quick Terminal runtime

### Requirement: Quick Terminal Has One Global Instance
**Reason**: The product no longer has a global quick-terminal instance.
Summoning targets the single primary shell window.

**Migration**: Remove global quick-terminal identifiers, cwd selection, hidden
state, and promotion semantics. Preserve normal workspace selection on summon.

#### Scenario: Global quick terminal identity is removed
- **WHEN** shell state is created, restored, or mutated
- **THEN** it contains no global quick-terminal instance alongside normal
  Spaces, Tabs, PaneSlots, or ContentInstances

### Requirement: Quick Terminal Peak uses a dedicated presentation boundary
**Reason**: The detached Peak presentation boundary is no longer part of the
macOS product.

**Migration**: Delete Quick Terminal Peak presenter/window/content boundaries
and route the shortcut through the macOS primary window presenter instead.

#### Scenario: Peak presentation boundary is removed
- **WHEN** the app owner initializes the shell host
- **THEN** it does not install a Quick Terminal Peak presenter or subscribe to
  quick-terminal presentation state

### Requirement: Peak panel presentation precedes terminal surface attachment
**Reason**: There is no Peak panel or quick-terminal surface attachment after
Quick Terminal removal.

**Migration**: Keep normal terminal surface attachment for selected terminal
content in the primary shell window.

#### Scenario: Peak attachment sequence is removed
- **WHEN** the former shortcut is invoked
- **THEN** the app does not order a Peak panel or attach a separate
  quick-terminal terminal surface

### Requirement: Quick Terminal Peak uses narrow terminal-first content
**Reason**: The dedicated Quick Terminal content view is removed with the
standalone Peak feature.

**Migration**: The summoned primary shell window renders the normal Alan shell
workspace without duplicate Quick Terminal chrome.

#### Scenario: Quick Terminal content view is removed
- **WHEN** Alan renders the primary shell window
- **THEN** it uses the normal workspace content tree and no Quick Terminal Peak
  content view

### Requirement: Quick Terminal promotion remains a runtime move
**Reason**: Promotion only existed to move a detached quick-terminal runtime
into a normal tab. There is no detached runtime to promote.

**Migration**: Remove `Open in Space` promotion and related target-space logic.
Users organize existing normal tabs and panes through normal workspace actions.

#### Scenario: Promotion action is removed
- **WHEN** menus, control commands, action descriptors, or UI affordances are
  enumerated
- **THEN** they do not expose Quick Terminal promotion
