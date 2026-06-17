## ADDED Requirements

### Requirement: Shell persistence does not block the main thread
The macOS shell SHALL NOT perform any synchronous main-thread disk write on the
terminal metadata or runtime callback path. Every state file it persists on that
path — the workspace manifest, the shell-state snapshot, the control-plane
`state.json` mirror, and the control-plane change-event log — SHALL have its
encode + write deferred to a debounced flush and/or run off the main thread.

#### Scenario: High-output terminal does not stall the UI
- **WHEN** one or more terminals produce sustained high-frequency output
- **THEN** alan does not perform a synchronous main-thread disk write of the workspace manifest, the shell-state snapshot, or the control-plane state file on the terminal metadata or runtime callback path

#### Scenario: Encode and write run off the main thread
- **WHEN** alan persists the workspace manifest or the control-plane shell-state file
- **THEN** the JSON encode and atomic file write run on a background executor rather than blocking the main actor

#### Scenario: Control-plane in-memory publication stays prompt
- **WHEN** shell state changes on the terminal callback path
- **THEN** alan publishes the in-memory control-plane state promptly without waiting on a disk write

### Requirement: Workspace persistence cadence is separated by durability class
The macOS shell SHALL persist workspace state on cadences matched to each class
of state rather than rewriting and disk-writing every file on every runtime event:
- **Structural state** (Spaces, Tabs, order, pin state, pin snapshots, selected
  Space/Tab) SHALL be persisted when its mutation is accepted.
- **Restore content and runtime snapshot** (per-Tab terminal transcript snapshots
  in the manifest, and the control-plane shell-state file) driven by terminal
  callbacks SHALL be persisted on a bounded debounced cadence and SHALL be
  force-flushed on app background/resign-active and on quit.
- A change to transient runtime state (such as a Tab's active-task state) SHALL
  NOT by itself trigger a synchronous disk write.

#### Scenario: Structural mutation persists promptly
- **WHEN** the user creates, closes, reorders, pins, unpins, or moves a Tab or Space
- **THEN** alan persists the structural change for that mutation

#### Scenario: Active-task change is not a write trigger
- **WHEN** a Tab's terminal-aware active-task state changes
- **THEN** alan does not write the workspace manifest solely because of that change

#### Scenario: Transcript is flushed on background and quit
- **WHEN** Alan for macOS resigns active, is backgrounded, or is asked to quit
- **THEN** alan force-flushes pending transcript snapshots to disk before completing the transition

#### Scenario: Recent transcript persists within the bounded window
- **WHEN** a terminal's transcript changes and the app keeps running
- **THEN** alan persists the latest transcript snapshot within the configured debounce window
- **AND** a hard crash may lose at most that window of the most recent scrollback
