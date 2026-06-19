## ADDED Requirements

### Requirement: Primary Window Summon Targets The Single Primary Shell Window
The Alan for macOS app SHALL expose a macOS-only Primary Window Summon command
that targets the process-scoped primary shell window instead of a detached
terminal panel or shell workspace action.

#### Scenario: Primary window is visible on another Space
- **WHEN** the user invokes Primary Window Summon while the primary shell window
  exists on another macOS Space or display
- **THEN** the app attempts to bring that same primary shell window to the user's
  current active Space and display
- **AND** the app activates and brings the primary shell window forward
- **AND** the app does not create a second primary shell window

#### Scenario: Primary window is closed while app is running
- **WHEN** the app process is running and the primary shell window has been
  closed
- **THEN** Primary Window Summon reopens or creates the one primary shell window
- **AND** the app activates and focuses the reopened primary shell window

#### Scenario: Workspace selection is preserved
- **WHEN** the user invokes Primary Window Summon
- **THEN** the app preserves the current shell Space, Tab, focused PaneSlot, split
  tree, and mounted content runtime identities
- **AND** the app does not create a terminal tab, quick-terminal runtime, or
  detached panel as a side effect

#### Scenario: Selected terminal content receives input focus
- **WHEN** the primary shell window is summoned and the selected content is a
  terminal
- **THEN** the app focuses terminal input through the existing terminal runtime
  focus path after the window is active

#### Scenario: Selected non-terminal content remains selected
- **WHEN** the primary shell window is summoned and the selected content is not
  terminal content
- **THEN** the app focuses the window or selected view without switching to a
  terminal or creating terminal content

#### Scenario: Active Space movement is best-effort
- **WHEN** AppKit or macOS Space behavior prevents the app from proving that the
  primary shell window moved to the current Space
- **THEN** the app still activates, brings the primary shell window forward,
  and preserves shell workspace selection
