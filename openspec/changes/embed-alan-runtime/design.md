# Design: Embeddable Alan Runtime (`embed-alan-runtime`)

## Context & Confirmed Decisions (`/grill-me` Q1–Q5)

1. **Q1 — Integration Form (Rust-first)**:
   - V1 targets **Rust in-process embedding** over `InProcessTransport` (`aP`).
   - Non-Rust bindings (C-ABI / UniFFI) and cross-process private sockets are
     explicitly deferred to a follow-up change built on top of this boundary.
2. **Q2 — Storage & Isolation**:
   - Embedded instances MUST isolate all durable state (`SystemStore`: rollouts,
     checkpoints, memory, definitions, packages, connection metadata; and
     optional `HostStore`: secrets, managed auth) under an **explicit
     host-supplied root directory** or run in **ephemeral in-memory mode**.
   - Embedded boot MUST NEVER read or write `dirs::data_dir()/Alan`,
     `ALAN_CONFIG_PATH`, or `HostEndpointPaths` (`host.lock` / `namespace.ap.sock`).
3. **Q3 — LLM & Credential Ownership**:
   - Primary path: host injects its own `Arc<dyn LlmClientFactory>` (or
     pre-built `LlmProvider`), retaining full control over model routing,
     tokens, and billing.
   - Optional path: host enables app-scoped `ConnectionService` + `HostStore`
     under its isolated root.
4. **Q4 — Tools, Host Mounts & Sandboxing Boundary**:
   - **Zero host access by default**: no `HostMount`, no `NativeHostMountExportAdapter`
     grants, and no `BashTool` / host write tools unless the host explicitly
     registers them in `ToolRegistry`.
   - Host apps can mount custom `FileServer` implementations (e.g., `/mnt/<app>`)
     into the embedded `Namespace` before boot.
5. **Q5 — Interaction Surface**:
   - Provide a thin ergonomic Rust facade (e.g., `crates/embed` or `os-host`
     embed module, with zero `alan-terminal-ui` / `ratatui` / `crossterm`
     dependencies) wrapping `ServiceManager` + `alan_shell::Shell`.
   - Expose raw `InProcessTransport` alongside typed helpers for `/agent/root`:
     `submit`, `tail_output`, `tail_events`, `respond_request`, `interrupt`, and
     `shutdown`.

## Architecture Overview

```text
+-------------------------------------------------------------------+
| Host Application (App A / App B in same or separate processes)    |
|  - Supplies: AppStoreRoot (or Ephemeral), LlmClientFactory,       |
|              ToolRegistry, custom /mnt/<app> FileServers          |
+-------------------------------------------------------------------+
         |                                           |
         | EmbeddedAlan::boot(EmbedConfig)           | aP helpers / InProcessTransport
         v                                           v
+-------------------------------------------------------------------+
| alan-embed (thin facade, no TUI deps)                             |
|  - EmbeddedConfig -> ServiceManagerConfig (no global paths/env)   |
|  - EmbeddedAlan { root: InProcessTransport, manager, agent_root } |
+-------------------------------------------------------------------+
         |
         v
+-------------------------------------------------------------------+
| alan-service-manager + alan-kernel + alan-agent-engine            |
|  - Completely instance-local: KernelRoot, ProcessTable, AgentFS   |
|  - Isolated SystemStore / Memory / Rollouts per EmbedConfig       |
+-------------------------------------------------------------------+
```

## Key Refactoring Points in Existing Codebase

1. **`crates/os-host/src/paths.rs`**:
   - Add `SystemStorePaths::from_root(root: &Path)` and
     `HostStorePaths::from_root(root: &Path)` (bypassing `PRODUCT_DIR = "Alan"`
     and `dirs::data_dir()`).
2. **`crates/os-host/src/boot.rs`**:
   - Extract shared `ServiceManagerConfig` assembly from
     `HostBootConfig::product_with_root_resume` so embedded callers do not invoke
     `installation::validate_current_invocation()` or `Config::load_with_metadata()`.
   - Promote the non-product builder from `test-only` `ephemeral` into a first-class
     `EmbedBootConfig` / `EmbeddedConfig` supporting both `IsolatedRoot(PathBuf)`
     and `Ephemeral` storage modes.
3. **`crates/service-manager/src/runtime.rs`**:
   - Ensure `ServiceManager::boot` handles `connection_store: None` and
     `package_store: None` cleanly without falling back to host-global paths,
     and allows custom pre-boot mounts under `/mnt/<app>` in `ProcessLaunchContext`.
4. **New `crates/embed` (or `os-host::embed` feature-gated without CLI/TUI)**:
   - Exposes `EmbeddedConfig`, `StorageMode` (`Isolated(PathBuf)` | `Ephemeral`),
     `EmbeddedAlan`, and `RootAgentClient` (thin `Shell`-backed wrapper over
     `/agent/root` `io/input`, `io/output`, `machine/ui/events`, `requests/`, and
     `machine/ctl`).

## Isolation Guarantees

- **Multi-instance in one process**: Booting `EmbeddedAlan` twice in the same
  Rust process with distinct `StorageMode::Isolated(dir_a)` and `dir_b` (or
  `Ephemeral`) shares zero mutable state (`KernelRoot`, `ProcessTable`,
  `SrvFs`, `AgentRootFs`, `TaskBudget`, `RolloutRecorder`, and `MemoryStore` are
  all instance-owned).
- **Multi-app on one machine**: No shared `host.lock`, no shared
  `namespace.ap.sock`, no shared `~/Library/Application Support/Alan/`.
- **Graceful teardown**: `EmbeddedAlan::shutdown()` shuts down `ServiceManager`
  and `root-agent`, dropping all background supervisor/watcher tasks cleanly.
