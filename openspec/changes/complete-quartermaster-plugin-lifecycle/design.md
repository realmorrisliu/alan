# Design: Quartermaster Companion Skills and Root Package Bindings

## Context

`package-management-contract` rightly forbids installing a package from silently spawning background daemons, mounting host directories, or injecting network/tool execution authority. However, conflating **read-only documentation (`SKILL.md`)** with **executable runtime capabilities (`fileserver`, `tool`, `mcp`)** created a gap:

- Installing `github-fileserver` gave Alan zero knowledge that the package even existed, because `runtime.rs` only projected embedded preinstalled packages on Root Agent boot.
- Separately installing `alan-self-iterate` also did not project its `SKILL.md` into interactive `alan` sessions.
- `WorkflowManifest` had no structured way to grant ephemeral workflow runs access to specific installed packages or active `fileserver` mounts.

## Architecture Decisions

### 1. Orthogonal Separation: Catalog State vs Knowledge Attachment vs Runtime Activation

| Layer | Owner | Effect on Root Agent | Runtime Authority |
| --- | --- | --- | --- |
| **Catalog** (`catalog.json`) | `PackageService` | None | None |
| **Root Binding: `SkillsOnly`** (`root-bindings.json`) | `PackageService` | Mounts `/lib/pkg/<id>` RO + loads `record.exports` skills | **Zero** processes, **zero** `/mnt/*` service mounts, **zero** `/bin/*` tool/MCP projections |
| **Root Binding: `Active`** (`root-bindings.json`) | `PackageService` + `ServiceManager` | Same `/lib/pkg/<id>` skills **plus** runtime projection (`fileserver` started & mounted at `/mnt/<sid>`, `tool`/`mcp` projected to `/bin`) | Strictly bounded by explicit `FileserverMountAccess` (`ReadOnly` default), `allow_network` (`false` default), and OS sandbox posture |

This preserves **ADR-0061** ("Installing a package grants no runtime authority") while making `alan q install <pkg>` immediately teach Root Agent what plugins exist and how to help the user configure them safely.

### 2. Companion Skills Inside Non-`skill` Packages

- In `materializer_exports.rs`, `Tool`, `Mcp`, `Provider`, and `Fileserver` packages also run a `skills/`-only scanner (`materialize_companion_skills_in_map`):
  - Only accepts `skills/<skill-dir>/SKILL.md` (with sibling assets) or command-style `skills/<name>.md`.
  - Explicitly ignores top-level `README.md` or unrelated package files to prevent accidental markdown ingestion as skills.
  - Populates `PackageRecord.exports` cleanly without changing `PackageExportKind`.
- Skill ID collision checks run at bind/projection time (`bind_for_root` against preinstalled packages and other bound Root packages, plus existing launch-time checks in `ResolvedAgentDefinition`).

### 3. Per-Invocation Isolation for `--ephemeral` and `alan workflow`

- Global Root bindings live in `<package_store>/root-bindings.json` and load automatically in `HostBootConfig::product_with_root_resume`.
- CLI adds repeatable `--package <id>` and `--fileserver <id>[:ro|rw][:net]`.
- When `--ephemeral` is combined with explicit `--package` or `--fileserver` flags (as used by `alan workflow run`), global Root bindings are replaced by ONLY those explicit flags so background automation retains strict least privilege regardless of what interactive plugins the user enabled for personal chat.

### 4. Foreground Fileserver Lifecycle and `alan workflow watch`

- `ServiceManager` owns an `Arc<FileserverManager>`, starts `Active` fileservers during `assemble_environment`, mounts `/mnt/<service_id>` into both `system_namespace` (visible to `LocalEntryService` and TUI `/issue`) and Root Agent's `launch_context`, and calls `fileserver_manager.stop_all().await` during `ServiceManager::shutdown()`.
- Bare `alan` still starts zero hidden watchers. When an operator wants continuous GitHub event monitoring, they invoke `alan workflow watch <id>` explicitly in a foreground terminal (or `render-schedule --target launchd|cron`), where `watch` starts `github-fileserver` under confinement, composes `/mnt/github/events` with `RouteFs` and `GithubEventDispatcher`, dispatches allowlisted items to bounded ephemeral workflow runs, and stops cleanly on Ctrl+C / SIGTERM (or after one pass with `--once`).
