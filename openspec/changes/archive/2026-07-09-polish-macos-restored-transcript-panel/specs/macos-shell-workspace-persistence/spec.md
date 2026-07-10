## MODIFIED Requirements

### Requirement: Terminal transcript snapshots restore the prior visible session context
The macOS shell workspace manifest SHALL persist terminal transcript snapshots
as bounded session-continuity state that can present prior visible terminal
context after app restart without claiming that the prior PTY or child process
survived.

#### Scenario: App closes with visible terminal output
- **WHEN** Alan closes or quits while a retained terminal ContentInstance has visible output and restorable transcript history
- **THEN** the workspace manifest stores a bounded transcript snapshot for that terminal content
- **AND** the snapshot preserves enough text, dimensions, title, cwd, focus, and viewport context to show the prior terminal state after restart

#### Scenario: App restarts after transcript snapshot
- **WHEN** Alan restores a terminal ContentInstance from a workspace manifest that contains a terminal transcript snapshot
- **THEN** Alan materializes the terminal with the saved transcript context before or during new runtime startup
- **AND** the restored terminal remains usable by starting a new shell in the restored cwd
- **AND** the normal terminal UI may show the restored transcript in a distinct restored-context panel when that panel is quiet, bounded, and terminal-aligned
- **AND** the UI does not present the restored transcript as a warning banner or claim that the prior process is still running

#### Scenario: Restored transcript is cleared
- **WHEN** the user invokes a supported terminal or Alan clear action for a terminal ContentInstance with a restored transcript snapshot
- **THEN** Alan removes the restored transcript snapshot from in-memory shell state for that content
- **AND** the next persisted workspace manifest no longer contains that restored transcript snapshot for the content
- **AND** subsequent tab switches, pane remounts, or app relaunches do not re-show the cleared restored transcript

#### Scenario: Transcript snapshot is too large
- **WHEN** terminal history exceeds the configured row or encoded-byte snapshot limit
- **THEN** Alan stores a bounded tail snapshot and records truncation metadata
- **AND** manifest persistence remains bounded in size and time
