## MODIFIED Requirements

### Requirement: Terminal Activity Is Normalized

Alan SHALL normalize terminal progress, foreground command state, command completion, bell attention, and supported CLI coding-agent process state into a pane-scoped terminal activity model before projecting that state into UI, accessibility, persistence, or control surfaces.

#### Scenario: Ghostty progress is normalized

- **WHEN** a terminal pane receives a Ghostty progress report
- **THEN** Alan records a pane-scoped activity state with progress kind, optional percentage, freshness timestamp, source kind, and attention priority

#### Scenario: Command completion is normalized

- **WHEN** a terminal pane receives a command-finished event with exit status or duration metadata
- **THEN** Alan records a command completion activity with success or failure status, last command metadata when available, and a bounded freshness window

#### Scenario: Agent state is normalized

- **WHEN** a supported CLI coding agent emits a reliable running, blocked, complete, or error signal
- **THEN** Alan records agent activity using the same pane-scoped activity model instead of introducing a separate sidebar or notification state path

### Requirement: CLI Coding-Agent Status Is Ingested Conservatively

Alan SHALL ingest CLI coding-agent lifecycle state from reliable structured signals, documented notification hooks, or explicit terminal integration adapters, and SHALL fall back to generic terminal activity when no reliable signal is available.

#### Scenario: Structured agent event arrives

- **WHEN** a supported agent event identifies the agent kind, process or invocation identity, cwd or project, and lifecycle transition
- **THEN** Alan maps it to pane-scoped agent activity
- **AND** default UI surfaces retain only user-facing safe metadata

#### Scenario: Agent support is partial

- **WHEN** Alan can detect a likely coding-agent process but cannot determine whether it is running, blocked, complete, or errored
- **THEN** Alan surfaces generic foreground-command or unknown-agent activity rather than claiming a precise lifecycle state

#### Scenario: Agent event is unsafe or malformed

- **WHEN** an agent integration emits malformed, untrusted, or overly detailed status payloads
- **THEN** Alan drops or sanitizes the payload for default UI
- **AND** raw diagnostics remain available only through explicit debug surfaces

#### Scenario: Initial Codex adapter

- **WHEN** Codex emits a reliable notification or structured lifecycle signal
- **THEN** Alan maps it into the same pane-scoped activity model used by terminal, command, and progress sources
