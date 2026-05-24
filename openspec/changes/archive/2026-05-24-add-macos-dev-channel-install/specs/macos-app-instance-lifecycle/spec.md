## ADDED Requirements

### Requirement: macOS app singleton is channel-scoped
The macOS app singleton, support identity, and shell-control namespace SHALL be
scoped by install channel so stable Alan and Alan Dev can run at the same time.

#### Scenario: Stable and dev apps launch together
- **WHEN** `Alan.app` is already running and the user launches `Alan Dev.app`
- **THEN** the dev app acquires a dev-channel singleton lock
- **AND** it does not activate or terminate the stable app instance
- **AND** it creates only dev-channel shell windows, support paths, runtime registries, and shell-control sockets

#### Scenario: Duplicate dev app launches
- **WHEN** a dev-channel app instance is already running and the user launches `Alan Dev.app` again
- **THEN** the existing dev app instance is activated
- **AND** no second dev app process remains running
- **AND** the stable app singleton state is not consulted as the owner for this decision

#### Scenario: Duplicate stable app launches while dev is running
- **WHEN** `Alan Dev.app` is running and the user launches `Alan.app` again
- **THEN** the existing stable app instance is activated if stable is already running
- **AND** the dev app instance is not treated as the stable singleton owner

### Requirement: Channel identity is visible in local diagnostics
Local diagnostics, logs, capture helpers, and shell-control paths SHALL expose
enough channel identity to distinguish stable Alan from Alan Dev.

#### Scenario: Logs are inspected
- **WHEN** maintainers inspect unified logs for both running apps
- **THEN** stable logs use the stable subsystem identity
- **AND** dev logs use a dev-channel subsystem identity such as `app.alanworks.macos.dev`
- **AND** filtering for one channel does not require excluding events from the other channel by process name alone

#### Scenario: Shell-control paths are inspected
- **WHEN** shell-control sockets or binding files are created
- **THEN** stable paths use the stable shell-control namespace
- **AND** dev paths use a distinct dev shell-control namespace
- **AND** both namespaces can exist simultaneously under the system temporary directory without one app reading or overwriting the other's control files

#### Scenario: Capture helper targets an app
- **WHEN** a capture or debugging helper targets an Alan channel
- **THEN** it can target the stable bundle identifier or the dev bundle identifier explicitly
- **AND** the default stable capture behavior remains compatible with `app.alanworks.macos`
