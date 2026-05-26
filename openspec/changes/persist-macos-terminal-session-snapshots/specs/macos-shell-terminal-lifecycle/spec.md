## ADDED Requirements

### Requirement: Destructive terminal close requests are guarded
The macOS shell host SHALL guard destructive pane, tab, window, app, and Quick
Terminal close requests before mutating authoritative shell state or releasing
terminal ContentInstance runtimes.

#### Scenario: Closing a pane with active work
- **WHEN** the user requests close for a PaneSlot that mounts terminal content with a foreground command, running alan session, pending yield, or unknown live active-task state
- **THEN** Alan asks for confirmation before removing the PaneSlot or finalizing the terminal ContentInstance runtime
- **AND** cancelling the confirmation leaves shell state, workspace manifest state, and terminal runtime state unchanged

#### Scenario: Closing an idle terminal pane
- **WHEN** the user requests close for a PaneSlot whose terminal content is an idle shell prompt or an exited process
- **THEN** Alan may close the PaneSlot without an active-work confirmation

#### Scenario: Closing a tab with multiple terminal panes
- **WHEN** the user requests close for a tab containing multiple terminal ContentInstances and at least one has active work
- **THEN** Alan presents at most one confirmation for the tab close request
- **AND** the tab is removed only after the user confirms

#### Scenario: Closing a window or quitting the app
- **WHEN** the user requests window close or app quit while any affected terminal ContentInstance has active work
- **THEN** Alan presents at most one confirmation for that requested close scope
- **AND** no affected terminal runtime is finalized until the user confirms

#### Scenario: Quick Terminal close has active work
- **WHEN** the user requests Quick Terminal close while its terminal content has active work
- **THEN** Alan applies the same close guard semantics used for regular shell terminal panes

### Requirement: Confirmed close captures terminal session snapshots
The macOS shell host SHALL attempt to capture bounded terminal transcript
snapshots for affected live terminal ContentInstances after a destructive
terminal close request is confirmed and before finalizing their runtimes.

#### Scenario: Confirmed pane close captures a snapshot
- **WHEN** the user confirms closing a terminal PaneSlot with restorable terminal history
- **THEN** Alan captures a bounded transcript snapshot for the mounted terminal ContentInstance before invoking runtime finalization
- **AND** the snapshot is associated with the terminal ContentInstance identity and close reason

#### Scenario: Snapshot capture fails after confirmation
- **WHEN** the user has confirmed a destructive close and snapshot capture or persistence fails
- **THEN** Alan records a diagnostic for debugging
- **AND** Alan may continue the confirmed close instead of trapping the user in the closing surface

#### Scenario: App restart restores history but not process continuity
- **WHEN** Alan restarts after terminal ContentInstances were closed or interrupted in the prior app instance
- **THEN** restored terminal panes may present saved transcript history from the prior session
- **AND** Alan creates new terminal runtimes and child processes instead of claiming continuity with the prior app instance's PTYs, child processes, or Ghostty surfaces
