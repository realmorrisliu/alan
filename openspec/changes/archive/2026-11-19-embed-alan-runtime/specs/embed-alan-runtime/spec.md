## ADDED Requirements

### Requirement: Explicit per-instance storage isolation
An embedded Alan runtime SHALL store all durable state (`rollouts`, `checkpoints`,
`cache`, `tmp`, `metadata`, `definitions`, `memory`, `packages`, and optional
`connections` / `credentials`) strictly under an explicit host-supplied root
directory, or operate in `Ephemeral` in-memory mode with no disk persistence.
Embedded boot SHALL NOT read or write `dirs::data_dir()/Alan`, SHALL NOT read
`ALAN_CONFIG_PATH`, SHALL NOT run CLI installation validation, and SHALL NOT
acquire global `HostEndpointPaths` locks or sockets.

#### Scenario: Two apps boot concurrently on the same machine
- **WHEN** App A boots an embedded runtime with `StorageMode::Isolated(dir_a)` and App B boots with `StorageMode::Isolated(dir_b)`
- **THEN** all rollouts, checkpoints, memory files, and connection metadata for App A reside strictly inside `dir_a` and for App B strictly inside `dir_b`
- **AND** neither instance touches the global Alan System Store or Host Store

#### Scenario: Two instances boot in the same Rust process
- **WHEN** a single host process boots two `EmbeddedAlan` instances (isolated or ephemeral)
- **THEN** each instance owns an independent `KernelRoot`, `ProcessTable`, `/agent/root`, and `ServiceManager`
- **AND** shutting down or cancelling one instance has no effect on the other

### Requirement: Host-injected LLM and credential boundary
Embedded configuration SHALL accept a host-supplied `Arc<dyn LlmClientFactory>`
(or single `LlmClient` adapter) as its primary model connection provider without
requiring `HostStore` credential files on disk. When a host opts into Alan-managed
profiles, `ConnectionService` and `HostStore` SHALL be scoped strictly to the
instance's isolated root.

#### Scenario: Host injects custom LLM factory without HostStore
- **WHEN** a host boots `EmbeddedAlan` with a custom `LlmClientFactory` and no `HostStore` directory
- **THEN** Root Agent and child Agent Processes resolve model calls through the host's factory
- **AND** no `secrets.toml` or `auth.json` is read from or written to the user's global Host Store

### Requirement: Zero host filesystem and shell access by default
By default, an embedded Alan instance SHALL mount no host filesystem directories
(`HostMount` disabled) and SHALL register no host shell or host write tools
(`BashTool`, `WriteFileTool`, `EditFileTool` omitted). Tools and host mounts
SHALL be active only when explicitly supplied by the host application. Host apps
MAY mount custom `FileServer` trees under `/mnt/<name>` into the root namespace.

#### Scenario: Default embedded instance attempts host access
- **WHEN** a host boots `EmbeddedAlan` with default capabilities and no explicit `ToolRegistry` or host mounts
- **THEN** the Agent has no `bash` or host filesystem tools and `/mnt` contains no host project directories

#### Scenario: Host mounts domain FileServer
- **WHEN** a host mounts a custom `FileServer` at `/mnt/my-app` in `EmbeddedConfig` and supplies domain tools referencing `/mnt/my-app`
- **THEN** Agent Processes within that instance can read and write `/mnt/my-app` through `aP` subject to the mount's `Access` mode

### Requirement: Ergonomic `aP` interaction facade without TUI dependencies
The embedding crate/module SHALL NOT depend on `alan-terminal-ui`, `ratatui`, or
`crossterm`. It SHALL expose the root `InProcessTransport` alongside thin async
helpers over `/agent/root` for submitting input, tailing assistant output and UI
events, responding to pending yield requests, interrupting active turns, and
performing graceful shutdown.

#### Scenario: Embedded turn, yield response, and graceful shutdown
- **WHEN** a host submits a task through the embedded `/agent/root` helper and tails output/events
- **THEN** it receives streamed output and structured events over `aP`
- **AND** if a yield request is emitted under `/agent/<pid>/requests/<id>`, the host can write its response and resume execution
- **AND** calling `shutdown()` terminates the root agent and `ServiceManager` cleanly
