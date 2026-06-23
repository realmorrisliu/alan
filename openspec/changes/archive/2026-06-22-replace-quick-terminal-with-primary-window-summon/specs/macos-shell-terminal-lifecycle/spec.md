## MODIFIED Requirements

### Requirement: Destructive terminal close requests are guarded
The macOS shell host SHALL guard destructive pane, tab, window, and app close
requests before mutating authoritative shell state or releasing terminal
ContentInstance runtimes.

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
