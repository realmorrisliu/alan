# Proposal: Embeddable Alan Runtime (`embed-alan-runtime`)

## Why

Alan's core substrate (`alan9` Kernel, `aP`, `ServiceManager`, `AgentFS`, and
`AgentMachine`) already isolates each instance in-process without a global
thread or conversation manager. However, host-level entrypoints in
`crates/os-host` still hardcode global product directories (`dirs::data_dir()/Alan`),
CLI installation validation, `ALAN_CONFIG_PATH` environment overrides,
`ProductLlmClientFactory`, and the default 7-tool + `NativeHostMountExportAdapter`
bundle. As a result, third-party applications cannot cleanly embed Alan as an
isolated Agent engine without risking state collision, credential leakage, or
unwanted host filesystem/shell exposure.

## What Changes

- Define a formal Rust in-process embedding surface (`EmbedHost` /
  `EmbeddedAlan` facade, without `alan-terminal-ui` or `ratatui` dependencies)
  backed by `ServiceManager` and `aP` (`InProcessTransport`).
- Decouple store path resolution in `crates/os-host` (`SystemStorePaths` and
  `HostStorePaths`) from global `Alan` directories, supporting explicit per-app
  root directories as well as an ephemeral in-memory mode.
- Bypass CLI installation validation and `ALAN_CONFIG_PATH` environment lookup in
  embedded boot paths; accept all configuration, definitions, and capabilities
  explicitly from the host application.
- Make `LlmClientFactory` injection the primary model/credential boundary for
  embedded hosts, with optional app-scoped `ConnectionService` / `HostStore`.
- Enforce zero host filesystem and shell access by default (`HostMount` and
  `BashTool` disabled unless explicitly registered by the host), while letting
  host apps mount their own `FileServer` trees at `/mnt/<app>`.
- Provide ergonomic `aP` client helpers over `/agent/root` (input submission,
  output/event tailing, yield request responses, interrupt, and graceful
  shutdown).

## Capabilities

### New Capabilities

- `embed-alan-runtime`: Defines the Rust in-process embedding contract,
  per-app storage and runtime isolation, host-injected LLM/tool/mount
  boundaries, and ergonomic `/agent/root` interaction helpers over `aP`.

### Modified Capabilities

- None (canonical `alan` CLI and foreground host behavior remain unchanged).

## Non-Goals (Deferred to Follow-Up Changes)

- C-ABI / UniFFI bindings for non-Rust native hosts (Swift, C++, Electron).
- Cross-process private Unix Socket attachment for non-Rust hosts.
- Cargo feature splitting across `alan-llm` providers (optional optimization).
