## REMOVED Requirements

### Requirement: TUI remains daemon-backed
**Reason**: The daemon/session path cannot remain the local terminal contract if
file-backed renderer hosts are expected to reach full parity and replace the
compatibility path.
**Migration**: Local `alan-terminal-ui` behavior moves to mounted
renderer-host/file surfaces; daemon-backed operation remains only as an explicit
compatibility or remote path until it can be removed.

## ADDED Requirements

### Requirement: Local TUI reads full renderer state from mounted agent files
The Rust terminal UI SHALL treat mounted agent files as the full local terminal
contract, not only for `io/output`, `io/input`, `requests/`, `actions/`, and
`ctl`, but also for renderer-visible runtime state such as activity, thinking,
plan snapshots, and notices.

#### Scenario: Local file-backed mode starts without daemon session APIs
- **WHEN** the user launches local `alan-terminal-ui` against a namespace-native
  runtime
- **THEN** it hydrates and tails the required state directly from mounted agent
  files, including renderer-visible runtime UI state
- **AND** it does not create or attach to a daemon session before rendering

#### Scenario: Explicit compatibility or remote mode still uses daemon APIs
- **WHEN** the user explicitly chooses a daemon-backed compatibility or remote
  path during migration
- **THEN** the TUI MAY still use daemon APIs for that path
- **AND** the local terminal architecture target remains direct file reads and
  `ctl` writes

### Requirement: File-backed local mode preserves local interaction parity
The Rust terminal UI SHALL preserve the current local interaction baseline when
running file-backed: pending input surfaces, command and reference completion,
live activity, collapsed thinking, plan visibility, and renderer-visible
warnings or compaction notices SHALL all work without daemon session event
streams.

#### Scenario: Activity and notices stay in the live region
- **WHEN** a local file-backed turn is active or the runtime emits a recoverable
  warning, compaction notice, or memory-flush notice
- **THEN** the TUI renders that state in the bottom live region from mounted
  agent files
- **AND** it does not require daemon event classification to decide what stays
  ephemeral

#### Scenario: Thinking and plan state render from file surfaces
- **WHEN** the runtime exposes renderer-visible thinking or plan updates through
  agent files
- **THEN** the local file-backed TUI renders collapsed thinking and human-readable
  plan state from those files
- **AND** the user can interact with that state without a daemon-backed session
