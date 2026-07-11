## REMOVED Requirements

### Requirement: Bare alan launches the Rust terminal UI

**Reason**: The explicit-subcommand contract retains `alan daemon status` as a supported management command.

**Migration**: Keep bare `alan` and surviving direct management commands without a daemon command family.

### Requirement: Local TUI reads full renderer state from mounted agent files

**Reason**: The requirement is expressed as a migration away from daemon Session APIs instead of a clean renderer-host contract.

**Migration**: Specify mounted AgentFS and Process files as the complete local input boundary.

### Requirement: File-backed local mode preserves local interaction parity

**Reason**: The requirement defines parity against daemon Session event streams.

**Migration**: Preserve the interaction baseline directly through file snapshots, streams, and control writes.

### Requirement: Pending input surfaces are first-class

**Reason**: The recoverable-error scenario retains generic Session and control-plane vocabulary from the old client path.

**Migration**: Present yields and Process/file-surface recovery states through the AgentFS renderer boundary.

### Requirement: Renderer updates are classified into display tiers

**Reason**: The suppressed-update vocabulary includes Session hydration as a renderer event.

**Migration**: Classify Agent Machine hydration and file-stream lifecycle updates without Session identity.

### Requirement: Composer provides readline editing and persisted history

**Reason**: Submission history is scoped as persistence across Sessions.

**Migration**: Persist command history across TUI launches under channel-scoped user state.

## ADDED Requirements

### Requirement: Bare alan launches the file-backed Rust terminal UI

The `alan` binary SHALL launch its linked Rust terminal UI when invoked without an explicit subcommand. Surviving direct management subcommands SHALL run instead of starting the TUI.

#### Scenario: Bare command enters the TUI

- **WHEN** a user runs `alan` in an interactive terminal
- **THEN** Alan starts the linked Rust terminal UI
- **AND** no separate terminal-UI executable is required on `PATH`

#### Scenario: Direct management command is selected

- **WHEN** a user runs a supported command such as `alan connection list`
- **THEN** Alan executes that command directly
- **AND** it does not start the TUI

### Requirement: Mounted AgentFS files are the complete local TUI contract

The Rust terminal UI SHALL hydrate and update renderer state from mounted `/agent` and `/proc` files, including IO, requests, actions, Agent Machine state, activity, plans, and notices.

#### Scenario: Local renderer starts from a mounted Agent Process

- **WHEN** the TUI receives a mounted namespace and concrete Agent Process path
- **THEN** it reads initial renderer state and tails offset-readable files from that surface
- **AND** user input and control actions are file writes to the mounted Process surfaces

### Requirement: File-backed interaction preserves the terminal baseline

The Rust terminal UI SHALL provide pending input, completion, live activity, collapsed thinking, plan visibility, warnings, and compaction notices from AgentFS snapshots and streams.

#### Scenario: Live state is projected from files

- **WHEN** an Agent Process changes activity, thinking, plan, warning, or compaction state
- **THEN** the TUI updates the appropriate transcript or live region from mounted files
- **AND** display classification does not depend on a client transport event taxonomy

### Requirement: AgentFS yields and recovery states are first-class

The Rust terminal UI SHALL render confirmation requests, structured input, recoverable Process errors, and recoverable file-surface gaps as focused user-facing states.

#### Scenario: Confirmation request is rendered

- **WHEN** AgentFS exposes a pending confirmation request
- **THEN** the TUI presents the action, choices, and default keyboard behavior
- **AND** the answer is written through the request's file control surface

#### Scenario: File stream cannot resume completely

- **WHEN** an offset-readable renderer stream reports that retained data cannot satisfy the last cursor
- **THEN** the TUI shows a concise recoverable state and available recovery actions
- **AND** diagnostic details remain behind an explicit debug surface

### Requirement: Renderer file updates are classified into display tiers

The TUI SHALL classify each renderer-visible file update as permanent transcript content, ephemeral live-region status, or suppressed lifecycle detail.

#### Scenario: Machine hydration is suppressed

- **WHEN** the renderer hydrates Agent Machine state or observes Process attachment lifecycle metadata
- **THEN** it does not print that lifecycle detail into the transcript
- **AND** it MAY retain the detail in tracing output

#### Scenario: Conversational substance is permanent

- **WHEN** AgentFS surfaces user input, assistant output, a completed Tool result, a plan snapshot, or a fatal error
- **THEN** the TUI renders it as permanent transcript content

### Requirement: Composer history persists across launches

The TUI composer SHALL support standard readline editing and SHALL persist prior submissions in channel-scoped user state for recall across launches.

#### Scenario: A later launch recalls history

- **WHEN** a user submits text, exits the TUI, and launches it again in the same channel
- **THEN** history-previous recalls the earlier submission
- **AND** stable and dev installations do not share the history file implicitly
