## ADDED Requirements

### Requirement: Performance diagnostics are opt-in Settings controls
The macOS shell SHALL expose performance diagnostics through Settings as a
default-off, progressively disclosed control rather than default shell chrome.

#### Scenario: Diagnostics default off
- **WHEN** a user opens Alan for macOS with no prior diagnostics preference
- **THEN** performance diagnostics are disabled
- **AND** the default shell, sidebar, terminal panes, and toolbar do not show
  persistent diagnostics chrome

#### Scenario: Diagnostics enabled from Settings
- **WHEN** the user enables `Performance Diagnostics` in Settings
- **THEN** Alan begins collecting recent local performance diagnostics
- **AND** the Settings surface communicates that diagnostics are local and
  intended for performance investigation

#### Scenario: Recent diagnostics exported
- **WHEN** diagnostics are enabled and the user invokes `Export Recent Diagnostics`
- **THEN** Alan exports the currently retained local diagnostics bundle without
  requiring the user to manually start, stop, or mark a capture window

#### Scenario: Diagnostics disabled
- **WHEN** the user disables `Performance Diagnostics`
- **THEN** Alan stops collecting diagnostics
- **AND** unexported in-memory diagnostics are cleared
- **AND** already exported local bundles remain under user control

#### Scenario: Settings remains shell-native
- **WHEN** the diagnostics control is visible in Settings
- **THEN** it uses compact Settings row treatment
- **AND** it does not introduce a dashboard, inspector, timeline viewer, or
  debug-heavy panel into the default shell
