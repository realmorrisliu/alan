# macos-app-instance-lifecycle Specification

## Purpose
Define the native macOS app singleton and primary shell window lifecycle so
launch, reopen, duplicate-process, and New Window paths preserve one alan app
instance and one shell control plane per user session.
## Requirements
### Requirement: Native macOS launches use one app instance
The Alan for macOS app bundle SHALL allow only one running alan app instance for
the current user and bundle identifier.

#### Scenario: Initial launch
- **WHEN** no Alan for macOS app instance is running and the user launches the
  app
- **THEN** one alan app process starts and acquires the singleton app lock

#### Scenario: Repeated normal launch
- **WHEN** an Alan for macOS app instance is already running and the user
  launches the app through normal Finder, Dock, Spotlight, or `open` behavior
- **THEN** the existing app instance is activated and no additional alan app
  process remains running

#### Scenario: Forced duplicate launch
- **WHEN** an Alan for macOS app instance is already running and a second app
  process is forced with `open -n` or direct executable launch
- **THEN** the second process activates the existing app and terminates before
  creating a SwiftUI scene, shell window context, control socket, or terminal
  runtime

#### Scenario: Quit releases singleton ownership
- **WHEN** the running alan app quits normally
- **THEN** the singleton app lock is released so the next launch can become the
  owner

#### Scenario: Crashed owner does not block relaunch
- **WHEN** a prior alan app process exits without a clean quit
- **THEN** stale singleton state does not prevent a later launch from acquiring
  ownership

### Requirement: Native macOS presents one primary shell window
The Alan for macOS app SHALL present at most one primary terminal workspace
window, and all launch, reopen, activation, and New Window paths SHALL focus or
reopen that window instead of creating another primary terminal workspace
window.

#### Scenario: First owned launch creates primary window
- **WHEN** the owned alan app instance completes launch
- **THEN** exactly one primary terminal workspace window is presented without
  requiring a Dock icon click, application reopen, or other secondary
  activation step

#### Scenario: New Window command
- **WHEN** the user invokes the New Window menu item or presses `Command-N`
- **THEN** no additional primary terminal workspace window is created and the
  existing primary window is focused or reopened

#### Scenario: Activation while primary window is visible
- **WHEN** the user activates alan while the primary terminal workspace window
  is already visible
- **THEN** the existing primary terminal workspace window becomes key without
  allocating another shell window

#### Scenario: Reopen after closing primary window
- **WHEN** the alan app is still running after the primary terminal workspace
  window has been closed
- **THEN** Dock or application reopen presents one primary terminal workspace
  window and does not create more than one terminal workspace window

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
- **AND** the app does not create a terminal tab, detached terminal runtime, or
  separate terminal panel as a side effect

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

### Requirement: Primary shell owner is process scoped
The macOS app SHALL own the primary shell context at app-process scope so scene
or root-view recreation does not allocate competing shell hosts while the app
process remains running.

#### Scenario: Root view recreated
- **WHEN** SwiftUI recreates the primary shell root view for the same running app process
- **THEN** the view reuses the process-scoped shell owner instead of creating a fresh shell window identity

#### Scenario: Primary scene reopened
- **WHEN** the primary window scene is reopened in the existing app process
- **THEN** the shell owner remains singular and no additional terminal runtime registry is created for a duplicate window

#### Scenario: Duplicate process exits early
- **WHEN** a second app process fails singleton lock acquisition
- **THEN** it exits without creating shell persistence files, control-plane sockets, or runtime registries

### Requirement: Singleton behavior has focused verification
The Apple client SHALL include focused automated checks or documented manual
verification for macOS process singleton, primary-window singleton, command
routing, reopen, and lock-release behavior.

#### Scenario: Lock behavior tested
- **WHEN** singleton lock code changes
- **THEN** tests verify first acquisition, rejected second acquisition, release, and owner-exit recovery

#### Scenario: Window singleton verified
- **WHEN** macOS scene or command behavior changes
- **THEN** tests, local scripts, or manual notes verify initial launch through the local app runner, `Command-N`, Dock reopen, close/reopen, repeated `open`, and forced `open -n`

#### Scenario: Documentation updated
- **WHEN** singleton behavior changes the shell window lifecycle contract
- **THEN** Apple-client README or related developer docs no longer describe multiple independent macOS windows as the supported default model

### Requirement: Native macOS app identity uses Alan for macOS naming
The native macOS app SHALL align bundle, display, singleton, logging, capture,
and persisted support identities with the `Alan` product brand and
`Alan for macOS` platform label, while preserving lowercase command and machine
identifiers where required for compatibility.

#### Scenario: App metadata is generated
- **WHEN** the macOS app bundle is built
- **THEN** the generated app product is `Alan.app`
- **AND** `CFBundleDisplayName` and macOS product name are `Alan`
- **AND** the default bundle identifier is `app.alanworks.macos`

#### Scenario: Singleton and support paths are created
- **WHEN** singleton lock files or App Support persistence paths are created
- **THEN** they use the current Alan for macOS identity
- **AND** they do not create new paths named `AlanNative`

#### Scenario: Logs and capture helpers identify the app
- **WHEN** maintainers inspect logs, run capture helpers, or filter running app
  instances by bundle identifier
- **THEN** defaults use `app.alanworks.macos` and compatibility-safe lowercase
  command/system identifiers where paths or process names require them
- **AND** `com.realmorrisliu.AlanNative` and `dev.alan.native` are not active
  defaults

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

### Requirement: App channel and Alan OS Host channel are paired
Alan for macOS stable and dev SHALL discover only the matching Alan OS Host
endpoint and System Store identity. App singleton lifetime SHALL remain
separate from Alan OS Host singleton lifetime.

#### Scenario: Stable app exits
- **WHEN** the stable app terminates
- **THEN** the stable Alan OS Host remains running
- **AND** the dev channel is unaffected

### Requirement: App lifetime does not own Alan OS lifetime
Alan for macOS startup, window closure, app termination, crash, and update SHALL
not shut down the dedicated Alan OS Host or its Processes. The app SHALL
release only its own connections, fids, views, and Host adapter work.

#### Scenario: App quits with active Agent Processes
- **WHEN** the app terminates normally
- **THEN** the Alan OS Host and Agent Processes continue
- **AND** a later app launch can reattach by Process Reference
