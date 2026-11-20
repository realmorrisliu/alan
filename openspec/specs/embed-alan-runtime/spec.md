# embed-alan-runtime Specification

## Purpose
Define the contract for embedding the alan9 runtime inside a third-party Rust host application: explicit per-instance storage, host-injected model access, zero host filesystem or shell reach by default, and a terminal-free `aP` interaction facade.

## Requirements

### Requirement: Explicit per-instance storage isolation
An embedded Alan runtime SHALL store all durable state strictly under an explicit
host-supplied root directory, or run in `Ephemeral` in-memory mode with no disk
persistence. Embedded boot SHALL NOT use global Alan stores, endpoints or CLI config.

#### Scenario: Embedded boot avoids global state
- **WHEN** an embedded runtime boots
- **THEN** it does not read or write `dirs::data_dir()/Alan` or `ALAN_CONFIG_PATH`
- **AND** it does not run CLI installation validation or acquire global `HostEndpointPaths` locks or sockets
- **AND** `rollouts`, `checkpoints`, `cache`, `tmp`, `metadata`, `definitions`, `memory`, `packages` and optional `connections`/`credentials` live under the supplied root

#### Scenario: Two apps boot concurrently on the same machine
- **WHEN** App A boots an embedded runtime with `StorageMode::Isolated(dir_a)` and App B boots with `StorageMode::Isolated(dir_b)`
- **THEN** all rollouts, checkpoints, memory files, and connection metadata for App A reside strictly inside `dir_a` and for App B strictly inside `dir_b`
- **AND** neither instance touches the global Alan System Store or Host Store

#### Scenario: Two instances boot in the same Rust process
- **WHEN** a single host process boots two `EmbeddedAlan` instances (isolated or ephemeral)
- **THEN** each instance owns an independent `KernelRoot`, `ProcessTable`, `/agent/root`, and `ServiceManager`
- **AND** shutting down or cancelling one instance has no effect on the other

### Requirement: Host-injected LLM and credential boundary
Embedded configuration SHALL accept a host-supplied `Arc<dyn LlmClientFactory>`,
reusable `LlmClient`, or cloneable `SharedLlmProvider` without requiring `HostStore`
credential files on disk. When a host opts into Alan-managed profiles, it SHALL select
either `LlmSource::IsolatedHostStore` (scoped strictly to the instance's isolated root)
or explicit `LlmSource::SharedProductCredentials` (reading the user's `alan connection`
profiles and Host credentials without mutating user rollouts or memory). Single
`LlmClient` and `SharedProvider` sources SHALL be safely reusable across multi-session
spawns and root resets.

#### Scenario: Host injects custom LLM factory without HostStore
- **WHEN** a host boots `EmbeddedAlan` with a custom `LlmClientFactory`, `LlmClient`, or `SharedLlmProvider` and no `HostStore` directory
- **THEN** Root Agent, opened sessions, child sessions, and reset roots resolve model calls cleanly across reuse
- **AND** no `secrets.toml` or `auth.json` is read from or written to the user's global Host Store

#### Scenario: Host explicitly opts into user connection profiles
- **WHEN** a host configures `LlmSource::SharedProductCredentials`
- **THEN** connection metadata resolves from `SystemStorePaths::detect()` and credentials from `HostStorePaths::detect()`
- **AND** rollouts, checkpoints, packages, and memory remain bound to the instance's `StorageMode`

### Requirement: Zero host filesystem and shell access by default
By default, an embedded Alan instance SHALL mount no host filesystem directories
(`HostMount` disabled for `Ephemeral`; native Host Mount adapter inactive until explicit
`host_mount(...)` on `Isolated`) and SHALL register no host shell or host write tools.
Host apps MAY mount custom `FileServer` trees under non-reserved `/mnt/<name>` paths
with `Access::ReadOnly` or `Access::ReadWrite`. Non-reserved `/mnt/<name>` mounts
SHALL preserve caller-specified `Access` on both `local-entry` and Agent Processes while
rejecting reserved Alan OS roots (`connections`, `host-mount`, `llm`, `mem`, `package`,
`route`, `service-manager`, `/mnt`), and `ToolContext::namespace()` SHALL expose the
owning Process `aP` transport to custom `Tool` implementations.

#### Scenario: Default embedded instance attempts host access
- **WHEN** a host boots `EmbeddedAlan` with default capabilities and no explicit `ToolRegistry` or host mounts
- **THEN** the Agent has no `bash` or host filesystem tools and `/mnt` contains no host project directories

#### Scenario: Host mounts writable domain FileServer and calls custom Tool
- **WHEN** a host mounts a writable `FileServer` at `/mnt/my-app` with `Access::ReadWrite` and runs a custom `Tool` via `ToolContext::namespace()`
- **THEN** both `EmbeddedAlan::shell()` and the custom `Tool` can read and write `/mnt/my-app/...`
- **AND** `Access::ReadOnly` mounts continue rejecting writes and reserved `/mnt/*` mounts fail boot

### Requirement: Streaming V2 `aP` facade, prompt/schema controls, sessions, and policy hooks
The embedding crate/module SHALL NOT depend on `alan-terminal-ui`, `ratatui`, or `crossterm`.
It SHALL expose:
- Live incremental V2 `EmbedEvent` streaming from `io/output`, `actions/events`, `requests/events`, `machine/ui/events`, `machine/ui/usage`, `machine/ui/queue`, and `machine/tape` (`TextDelta { submission_id, chunk, is_final }`, `ThinkingDelta`, `ToolStarted`/`ToolProgress`/`ToolFinished`, rich `PendingRequest`, `Usage`, `Error`, `TurnFinished`).
- Per-turn (`submit_text_with_schema`) and default (`EmbeddedConfig::with_response_schema`) JSON Schema constrained `response_schema` propagation and validation with bounded retry recovery.
- `SystemPromptMode` (`Append` | `Replace { prompt }`), `EmbeddedConfig::with_builtin_skills(bool)`, and clean omission of absent/empty Agent Definition persona placeholders.
- Single-instance multi-session lifecycle (`open_session`, `spawn_child_session`, `close_session`, `reset_session`) via `AgentSessionHandle`.
- Synchronous host `ToolPolicyHook` enforcement (`Continue` | `Allow` | `Deny(String)` | `Escalate(String)`) ahead of tool execution.
- `EmbeddedAlan::host_mount` / `host_mount_for` on isolated or host-mount-enabled instances and best-effort cleanup in `Drop` alongside explicit `shutdown()`.
- Additive Cargo features in `alan-llm` (`provider-*`, `all-providers`), `alan-agent-engine` (`tree-sitter-sandbox`), and `alan-os-host` allowing lightweight embeds without unused providers or `tree-sitter-bash`.

#### Scenario: Host streams a turn with policy hooks and multi-session isolation
- **WHEN** a host app opens an `EmbeddedAlan` session with `SystemPromptMode::Replace`, `with_builtin_skills(false)`, a `ToolPolicyHook`, and subscribes to `event_stream()`
- **THEN** the stream emits incremental `TextDelta`, `ThinkingDelta`, `ToolStarted`/`ToolProgress`/`ToolFinished`, `PendingRequest`, `Usage`, and `TurnFinished` events without depending on `alan-terminal-ui`, `ratatui`, or `crossterm`
- **AND** denied tool calls are blocked before execution while sibling sessions and `reset_session` operate independently

#### Scenario: Host requests JSON Schema constrained output
- **WHEN** a host app submits a turn with `submit_text_with_schema` or `EmbeddedConfig::with_response_schema`
- **THEN** the engine forwards the schema constraint to the LLM request, validates the assistant output against the schema with bounded retry recovery, and omits absent persona `[MISSING]` placeholders

