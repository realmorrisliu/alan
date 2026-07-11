## MODIFIED Requirements

### Requirement: Active tasks prevent unpinned Tab retirement

The macOS shell SHALL protect Unpinned Tabs from lifecycle retirement when terminal-aware metadata indicates that user work is actively running or waiting for input.

#### Scenario: Foreground command is running

- **WHEN** an Unpinned Tab contains a terminal pane with an active foreground command
- **THEN** Alan treats that Tab as having an active task
- **AND** lifecycle pruning does not retire it solely because its TTL anchor is older than 12 hours

#### Scenario: Supported CLI agent is active

- **WHEN** an Unpinned Tab contains a supported CLI coding-agent process that is running, waiting for input, or reporting a pending user action through terminal activity metadata
- **THEN** Alan treats that Tab as having an active task
- **AND** lifecycle pruning does not retire it solely because its TTL anchor is older than 12 hours

#### Scenario: Shell is idle

- **WHEN** an Unpinned Tab contains only an idle shell prompt
- **THEN** Alan does not treat `processExited == false` by itself as an active task
- **AND** the Tab can be retired after TTL expiry
