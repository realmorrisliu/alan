## ADDED Requirements

### Requirement: Apple quality gate includes focused macOS shell tests
The Apple client SHALL provide repeatable commands for focused macOS shell tests
covering shell model mutations, runtime service fakes, control-plane command
execution, App Intent routing, and UI smoke flows.

#### Scenario: Developer runs Apple shell tests
- **WHEN** a developer runs the documented focused Apple shell test command
- **THEN** model, fake runtime, control-plane, and intent-routing tests run without requiring the full app UI

#### Scenario: Ghostty artifacts absent
- **WHEN** Ghostty artifacts are absent and the focused test command runs
- **THEN** tests that do not require real Ghostty run normally and Ghostty integration tests are skipped or fail with documented setup instructions

#### Scenario: Ghostty artifacts present
- **WHEN** Ghostty artifacts are prepared and the integration lane is requested
- **THEN** the macOS app builds with Ghostty and runs terminal-host integration checks

### Requirement: UI smoke coverage is repeatable
The Apple client SHALL provide a repeatable UI smoke or screenshot flow for
launch, space/tab switching, split creation, command UI, pane-scoped Find
behavior, and basic terminal input when terminal runtime is available.

#### Scenario: Launch smoke
- **WHEN** the UI smoke flow starts the macOS app
- **THEN** it verifies that the default light-mode window shows the unified sidebar column, active-space tab list, bottom space switcher, terminal content area, and no persistent inspector pane or toggle

#### Scenario: Split smoke
- **WHEN** the UI smoke flow creates a split
- **THEN** it verifies that multiple panes are visible and no raw pane IDs or debug labels dominate the default UI

#### Scenario: Find smoke
- **WHEN** the UI smoke flow opens pane-scoped Find for the focused terminal pane
- **THEN** it verifies that a focused text field appears in pane chrome, result feedback is visible without raw debug labels, and dismissing Find returns focus to the owning terminal surface

### Requirement: Test fixtures share production command paths
Apple tests SHALL exercise shell mutations through the same controller command
interfaces used by menus, command UI, App Intents, and control-plane handlers.

#### Scenario: Split command fixture
- **WHEN** a test invokes split through the shared command interface
- **THEN** the resulting shell state matches the state produced by native command and control-plane paths

#### Scenario: Close command fixture
- **WHEN** a test invokes close pane through the shared command interface
- **THEN** shell state updates and runtime fake finalization are both asserted

### Requirement: Build and test documentation stays current
Apple build/test scripts, `just` commands, and `clients/apple/README.md` SHALL
document local dependency setup, focused test commands, UI smoke commands, and
Ghostty integration prerequisites.

#### Scenario: Command added
- **WHEN** a new Apple shell test or smoke command is added
- **THEN** the README and just/script references are updated in the same change

#### Scenario: Dependency changes
- **WHEN** Ghostty or App Intent test prerequisites change
- **THEN** the documented setup and failure messages are updated together
