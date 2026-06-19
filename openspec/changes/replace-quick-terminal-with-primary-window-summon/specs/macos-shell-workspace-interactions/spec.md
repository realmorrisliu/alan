## REMOVED Requirements

### Requirement: Quick Terminal Summon And Dismiss Are Shell Commands
**Reason**: Quick Terminal is removed, and the replacement is an app/window
command that summons the primary shell window without mutating shell workspace
state.

**Migration**: Remove quick-terminal shell workspace commands and route the
former global shortcut through Primary Window Summon in the macOS app command
layer.

#### Scenario: Quick terminal command opens
- **WHEN** the user invokes the former quick-terminal shortcut, menu item, or
  supported command surface
- **THEN** Alan invokes Primary Window Summon instead of a shell workspace
  quick-terminal command

#### Scenario: Quick terminal global shortcut toggles
- **WHEN** the primary shell window is already visible and the user invokes the
  former quick-terminal shortcut again
- **THEN** the app focuses or summons the primary shell window and does not hide a
  detached presentation

#### Scenario: Quick terminal does not use Escape as hide
- **WHEN** the primary shell window owns focus and the user presses `Esc`
- **THEN** normal selected-content input handling applies, with no
  quick-terminal hide behavior

#### Scenario: Quick terminal close is explicit
- **WHEN** the user closes terminal content in the primary shell window
- **THEN** the app uses normal pane, tab, window, or app close semantics, with no
  quick-terminal close scope
