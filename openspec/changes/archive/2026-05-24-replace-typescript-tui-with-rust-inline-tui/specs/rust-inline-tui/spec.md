## ADDED Requirements

### Requirement: Bare alan launches the Rust terminal UI
The `alan` binary SHALL launch the Rust terminal UI when invoked without an
explicit subcommand, and this terminal UI SHALL be linked into the `alan` binary
rather than shipped as a separate executable.

#### Scenario: Bare command enters TUI
- **WHEN** a user runs `alan` in an interactive terminal
- **THEN** alan starts the Rust terminal UI
- **AND** no `alan-tui` executable is required on `PATH`

#### Scenario: Explicit subcommands remain available
- **WHEN** a user runs an explicit supported management subcommand such as
  `alan connection list` or `alan daemon status`
- **THEN** alan runs that subcommand instead of starting the TUI

#### Scenario: Noninteractive terminal is rejected truthfully
- **WHEN** a user runs bare `alan` without an interactive terminal
- **THEN** alan exits with a clear terminal capability error
- **AND** it does not attempt to launch a TypeScript or `alan-tui` fallback

### Requirement: Legacy TUI entrypoints are removed
alan SHALL remove the TypeScript/Bun/Ink TUI, the `alan-tui` shipped executable,
the `ALAN_TUI_PATH` override, and the public `alan chat` and `alan ask`
commands.

#### Scenario: Legacy commands are unavailable
- **WHEN** a user runs `alan chat` or `alan ask`
- **THEN** alan reports the command as unsupported or unknown
- **AND** it does not delegate to the old TypeScript TUI

#### Scenario: Legacy TUI fallback is unavailable
- **WHEN** `ALAN_TUI_PATH` is set in the environment
- **THEN** bare `alan` ignores it or reports it as unsupported
- **AND** no production code loads a TypeScript TUI bundle from that path

#### Scenario: Release artifacts omit alan-tui
- **WHEN** release artifacts are assembled
- **THEN** they include the `alan` executable for terminal use
- **AND** they do not include, sign, link, or install an `alan-tui` executable

### Requirement: TUI remains daemon-backed
The Rust TUI SHALL use alan daemon/session APIs for workspace resolution,
connection profile resolution, session lifecycle, event streaming, submissions,
resume operations, interrupts, compaction, rollback, and persisted history.

#### Scenario: Local daemon starts before session APIs
- **WHEN** a user runs bare `alan` with the default local daemon configuration
  and no healthy daemon is reachable
- **THEN** the Rust TUI starts the local daemon before creating or attaching to a
  session
- **AND** it waits for the daemon health endpoint to report readiness before
  calling session APIs

#### Scenario: Existing local daemon is reused
- **WHEN** a user runs bare `alan` and the configured local daemon is already
  healthy
- **THEN** the Rust TUI reuses that daemon
- **AND** it does not stop the daemon on exit unless the TUI started that daemon
  instance for this run

#### Scenario: Remote daemon override is respected
- **WHEN** a user runs bare `alan` with an explicit remote daemon URL such as
  `ALAN_AGENTD_URL`
- **THEN** the Rust TUI connects to that configured daemon
- **AND** it does not start, stop, or otherwise manage a local daemon process

#### Scenario: Daemon startup failure is actionable
- **WHEN** the configured local daemon cannot be started or does not become
  healthy before the startup timeout
- **THEN** the Rust TUI reports an actionable startup error before attempting to
  create or attach to a session

#### Scenario: Session starts through daemon APIs
- **WHEN** the Rust TUI starts a new conversation for a workspace
- **THEN** it creates or attaches to a daemon-backed session using the public
  session APIs
- **AND** it renders the resolved profile, provider, model, and durability state
  from daemon responses when those fields are available

#### Scenario: Event stream resumes after reconnect
- **WHEN** the TUI reconnects to an existing daemon-backed session
- **THEN** it reads persisted history or reconnect snapshot state before
  consuming new events
- **AND** it detects event gaps using daemon cursor metadata

#### Scenario: User input uses protocol operations
- **WHEN** the user submits a message, approval response, structured input,
  interrupt, rollback, or compaction request
- **THEN** the TUI sends the corresponding alan protocol operation through the
  daemon session APIs

### Requirement: Codex-like terminal interaction baseline
The first Rust TUI SHALL provide a Codex-like terminal interaction baseline:
explicit terminal mode ownership, ratatui-style frame rendering, a bottom
composer, inline viewport rendering, terminal scrollback transcript insertion,
typed transcript cells, resize reflow, and frame coalescing.

#### Scenario: Streaming assistant output renders incrementally
- **WHEN** daemon events stream thinking, text, tool, plan, warning, or error
  updates
- **THEN** the TUI updates typed transcript cells without rebuilding the entire
  transcript as plain strings
- **AND** it coalesces redraws so high-frequency deltas do not overwhelm the
  terminal

#### Scenario: Completed content enters terminal scrollback
- **WHEN** visible transcript content is committed beyond the active viewport
- **THEN** the TUI inserts committed lines into terminal scrollback
- **AND** the active inline viewport remains focused on current interaction

#### Scenario: Resize preserves readable state
- **WHEN** the terminal is resized during a turn or while editing input
- **THEN** transcript cells, the active viewport, and the bottom composer reflow
  without corrupting input or losing streamed content

### Requirement: Pending input surfaces are first-class
The Rust TUI SHALL render runtime yields such as confirmation requests,
structured user input, and recoverable interruptions as first-class terminal UI
states rather than raw JSON or debug text.

#### Scenario: Confirmation yield is shown
- **WHEN** the runtime emits a confirmation yield
- **THEN** the TUI presents a focused approval surface with the relevant action,
  choices, and default keyboard behavior
- **AND** the response is submitted as a protocol resume operation

#### Scenario: Structured input yield is shown
- **WHEN** the runtime emits a structured input yield
- **THEN** the TUI presents fields or choices that match the yielded schema
- **AND** it validates the response before submitting it to the daemon

#### Scenario: Recoverable runtime error is shown
- **WHEN** the daemon reports a recoverable session or stream error
- **THEN** the TUI renders a concise user-facing state with available recovery
  actions
- **AND** raw diagnostic details remain behind an explicit debug surface

### Requirement: Terminal behavior has focused verification
The Rust TUI SHALL include focused automated verification for terminal behavior,
including snapshots or vt100-style tests for transcript rendering, scrollback,
resize, composer editing, streaming deltas, pending yield surfaces, and
noninteractive startup failures.

#### Scenario: Terminal snapshots cover core cells
- **WHEN** typed transcript cell rendering changes
- **THEN** snapshot tests cover assistant text, thinking, tool calls, plans,
  warnings, errors, and pending yields

#### Scenario: Scrollback behavior is tested
- **WHEN** transcript viewport or scrollback insertion behavior changes
- **THEN** terminal behavior tests verify committed history, active viewport
  content, and resize reflow

#### Scenario: Legacy fallback cannot pass tests
- **WHEN** a production fallback path to `clients/tui`, Bun, Ink, or `alan-tui`
  is reintroduced
- **THEN** focused TUI or packaging contract checks fail
