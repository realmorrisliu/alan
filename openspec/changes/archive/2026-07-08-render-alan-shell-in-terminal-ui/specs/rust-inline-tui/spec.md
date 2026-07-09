## REMOVED Requirements

### Requirement: TUI remains daemon-backed
**Reason**: The daemon/session path is still useful during migration, but it is
no longer the durable terminal contract for `alan-terminal-ui`.
**Migration**: Keep the daemon-backed path as a compatibility mode while adding
file-backed renderer-host modes that read `/agent` and write `ctl`.

## ADDED Requirements

### Requirement: TUI supports a file-backed renderer-host mode
The Rust terminal UI SHALL support a local file-backed mode that renders from a
mounted Alan OS namespace. In that mode it SHALL converse with an agent by
tailing `<agent>/io/output`, writing `<agent>/io/input`, and writing generic
process control to `/proc/<pid>/ctl`.

#### Scenario: Local file-backed mode starts without daemon session APIs
- **WHEN** the user launches the file-backed TUI mode against a local
  namespace-native runtime
- **THEN** the TUI starts from a mounted aP root plus a concrete agent path
- **AND** it does not create or attach to a daemon session before rendering the
  conversation surface

#### Scenario: File-backed mode submits input through agent files
- **WHEN** the user submits a message in file-backed mode
- **THEN** the TUI writes the message to the agent's `io/input` file
- **AND** assistant output is rendered from the tailed `io/output` stream

#### Scenario: File-backed mode interrupts through process control
- **WHEN** the user presses the interrupt key while file-backed mode is running
- **THEN** the TUI writes `interrupt` to the concrete agent process's
  `/proc/<pid>/ctl`
- **AND** it does not route the interrupt through daemon session APIs

### Requirement: TUI keeps a daemon-backed compatibility mode during migration
The Rust terminal UI SHALL keep the current daemon/session path available during
the migration to file-backed rendering, but that path SHALL be treated as a
compatibility mode instead of the terminal architecture target.

#### Scenario: Compatibility mode still uses daemon sessions
- **WHEN** the user launches the daemon-backed compatibility mode
- **THEN** the TUI may still use daemon APIs for session lifecycle, history
  hydration, event streaming, and protocol submissions
- **AND** that path coexists with the file-backed renderer-host mode during the
  migration period
