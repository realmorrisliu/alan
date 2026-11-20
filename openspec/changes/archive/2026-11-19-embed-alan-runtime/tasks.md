# Tasks: Embeddable Alan Runtime (`embed-alan-runtime`)

## 1. Path & Boot Decoupling (`crates/os-host`, `crates/service-manager`)

- [x] 1.1 Add `SystemStorePaths::from_root(root: &Path)` and `HostStorePaths::from_root(root: &Path)` in `crates/os-host/src/paths.rs` without appending `Alan` product dir.
- [x] 1.2 Refactor `crates/os-host/src/boot.rs` so `ServiceManagerConfig` assembly is shared and embedded boot skips `installation::validate_current_invocation()` and `Config::load_with_metadata()` / `ALAN_CONFIG_PATH`.
- [x] 1.3 Verify `ServiceManager::boot` in `crates/service-manager/src/runtime.rs` supports `connection_store: None`, `package_store: None`, and host-supplied `/mnt/<app>` mounts cleanly.

## 2. Embedding Facade (`crates/embed` or `alan-os-host::embed`)

- [x] 2.1 Define `EmbeddedConfig` with `StorageMode::Isolated(PathBuf)` and `StorageMode::Ephemeral`, explicit `AgentProcessConfig`, `Arc<dyn LlmClientFactory>`, `ToolRegistry` (empty by default), and custom namespace mounts.
- [x] 2.2 Implement `EmbeddedAlan::boot(config)` returning the handle with `root_transport() -> InProcessTransport` and `shutdown()`.
- [x] 2.3 Implement thin `RootAgentClient` helpers over `alan_shell::Shell` (`/agent/root`): `submit`, `tail_output`, `tail_ui_events`, `respond_request`, and `interrupt`.
- [x] 2.4 Verify the embedding crate compiles without `alan-terminal-ui`, `ratatui`, or `crossterm` in its dependency tree.

## 3. Isolation & Lifecycle Verification

- [x] 3.1 Add integration test: two `EmbeddedAlan` instances booted concurrently in the same process with separate `Isolated` roots do not share `ProcessTable`, `AgentFS`, `MemoryStore`, or `Rollout` state.
- [x] 3.2 Add integration test: `Ephemeral` embedded instance writes nothing to `dirs::data_dir()/Alan` or temp product stores, completes a turn + yield/resume cycle, and shuts down cleanly.
- [x] 3.3 Run `just quality` and `cargo test --workspace` to ensure no regressions in CLI or kernel invariants.
