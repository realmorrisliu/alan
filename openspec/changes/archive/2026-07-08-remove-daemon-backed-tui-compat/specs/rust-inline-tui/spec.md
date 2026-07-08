## MODIFIED Requirements

### Requirement: Local TUI reads full renderer state from mounted agent files
The Rust terminal UI SHALL treat mounted agent files as the full terminal UI
contract, not only for `io/output`, `io/input`, `requests/`, `actions/`, and
`ctl`, but also for renderer-visible runtime state such as activity, thinking,
plan snapshots, and notices.

#### Scenario: Local file-backed mode starts without daemon session APIs
- **WHEN** the user launches `alan-terminal-ui` against a namespace-native
  runtime
- **THEN** it hydrates and tails the required state directly from mounted agent
  files, including renderer-visible runtime UI state
- **AND** it does not create or attach to a daemon session before rendering

#### Scenario: No daemon-backed TUI mode remains
- **WHEN** a user runs bare `alan`
- **THEN** the terminal UI launches only through the mounted file-backed
  renderer-host path
- **AND** no daemon-backed compatibility or remote TUI mode is available
