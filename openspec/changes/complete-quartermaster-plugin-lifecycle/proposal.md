# Change: Complete Quartermaster Plugin Lifecycle and Conversational Onboarding

## Why

After shipping `enable-self-iteration-loop` (Stage 1 `/issue`, Stage 2 `alan workflow`, Stage 3 `fileserver` + `githubfs`, and Stage 4 release preparation), three UX and architecture gaps remained between `alan q install` and daily personal usage:

1. **Separated knowledge and runtime packages**: Non-`skill` Quartermaster packages (`fileserver`, `tool`, `mcp`, `provider`) cannot export Companion Skills (`skills/`), forcing users to install multiple separate packages (`github-fileserver`, `alan-self-iterate`, `alan-release-prep`) before Alan knows how to operate or configure a plugin.
2. **Unconnected operator-installed packages**: Installing a package via `alan q install` records it in the Package Store catalog, but `alan` Root Agent startup only projects preinstalled built-in packages unless custom test hooks call `reference_package`. Meanwhile, `FileserverManager` is not wired into `ServiceManager` boot/shutdown or `alan workflow`.
3. **High manual CLI setup friction**: Operators have to manually read docs and assemble `alan host grant save`, `alan workflow init-self-iterate`, `alan workflow run`, and `githubfs` watcher orchestration instead of installing a single plugin package and letting Alan guide setup conversationally.

This change introduces Companion Skills on all Quartermaster package kinds, separates read-only knowledge attachment (`SkillsOnly`) from explicit runtime activation (`Active`) via System Store Root Package Bindings (`alan q enable` / `disable`, `--package` / `--fileserver`), wires `FileserverManager` cleanly into foreground `alan` and workflow execution, adds an explicit foreground `alan workflow watch <id>` event loop runner, and bundles generic onboarding and self-iteration Companion Skills inside `github-fileserver`.

## What Changes

- **Companion Skills (`package-management-contract`)**: Non-`skill` packages (`tool`, `mcp`, `provider`, `fileserver`) MAY bundle read-only Companion Skills under `skills/` (either `skills/<id>/SKILL.md` or `skills/<name>.md`), installed and retired atomically with the owning package revision.
- **Root Package Bindings (`SkillsOnly` vs `Active`)**: Add a durable, schema-versioned `root-bindings.json` store owned by `PackageService`. `alan q install` attaches a package's read-only Companion Skills to Root (`SkillsOnly`) by default (opt-out via `--catalog-only`), while keeping runtime executables, MCP servers, network grants, and fileserver processes disabled until explicit `alan q enable <pkg>` or explicit `--package` / `--fileserver` CLI invocation flags.
- **Foreground & Workflow Runtime Wiring (`package-management-contract`, `file-server-plugin-contract`)**: Load Root bindings during `HostBootConfig::product*` boot, apply `SkillsOnly` and `Active` projections in `ServiceManager` (mounting active `fileserver`s into both Root Agent and `LocalEntryService` so TUI `/issue` Tier 1 works immediately), cleanly stop all active fileservers on `ServiceManager::shutdown`, and extend `WorkflowManifest` + `build_alan_args` with explicit per-run `packages` and `fileservers` isolation.
- **Explicit Event Loop Runner (`file-server-plugin-contract`)**: Add `alan workflow watch <id>` composing `FileserverManager` (`github-fileserver`), `RouteFs`, and `GithubEventDispatcher` into an explicit Ctrl+C / SIGTERM-bounded foreground loop without leaving standing watchers behind bare `alan`.
- **Single-Package Self-Iteration UX**: Bundle generic `skills/github-setup/SKILL.md`, `skills/self-iterate/SKILL.md`, and `skills/release-prep/SKILL.md` into `crates/service-manager/packages/github-fileserver` and update `/man` (`1/alan`, `1/q`) so installing one package enables conversational setup inside Alan.

## Impact

- Affected specs: `package-management-contract`, `file-server-plugin-contract`
- Affected crates: `alan-service-manager`, `alan-os-host`, `alan`
- Backwards compatibility: Package Store catalog remains backwards-compatible with existing preinstalled and four/five-kind packages; `PackageCommand::Install` stays purely catalog-scoped while Root binding management sits explicitly above it in `PackageService` + `alan q`.
