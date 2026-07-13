## ADDED Requirements

### Requirement: App lifetime does not own Alan OS lifetime
Alan for macOS startup, window closure, app termination, crash, and update SHALL
not shut down the dedicated Alan OS Host or its Processes. The app SHALL
release only its own connections, fids, views, and Host adapter work.

#### Scenario: App quits with active Agent Processes
- **WHEN** the app terminates normally
- **THEN** the Alan OS Host and Agent Processes continue
- **AND** a later app launch can reattach by Process Reference
