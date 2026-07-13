## Why

Alan OS still treats a Host directory as a workspace identity that selects
runtime configuration, Tool authority, Agent overlays, memory, rollouts, and
generated `.alan` state. That model conflicts with Process-owned namespaces and
must be removed before Alan OS can become one system-level instance shared by
multiple hosts.

## What Changes

- **BREAKING** Remove `WorkspaceRegistry`, `alan workspace`, workspace
  initialization, `WorkspaceRuntimeConfig`, workspace identity/routing, and
  implicit current-directory boot semantics.
- Replace workspace binding with Process Launch Context: parent namespace,
  explicit mounts and descriptors, credentials, and namespace cwd.
- Make Host OS files invisible by default and admit them only as explicit Host
  Mounts whose namespace access and native sandbox grants share one authority.
- **BREAKING** Stop reading or writing directory-local `.alan` runtime state,
  `~/.alan` runtime/config authority, Host-directory Agent overlays, workspace
  Skills, and workspace-local package/Tool sources.
- Introduce channel-isolated Alan OS System Store backing selected by the Host,
  with durable data partitioned by its owning file-server service.
- Separate Host Command Plane operations from Alan OS Shell commands; running
  `alan` enters Shell and ordinary Agent Processes are spawned inside Alan OS
  with explicit Agent Definition descriptors.
- Perform bounded cleanup: delete recognized generated state, migrate-verify-
  delete legacy connection metadata, and require explicit import for possibly
  user-authored Agent, Skill, policy, persona, and Memory content.
- Block and later rewrite `add-alan-package-management`; preserve no implicit
  Host-directory compatibility providers.

## Capabilities

### New Capabilities

- `process-launch-context`: Process execution context without workspace
  identity.
- `alan-os-system-store`: Host-provided, service-owned durable backing and
  bounded legacy cleanup.
- `host-command-plane`: Separation of Host lifecycle/native operations from
  namespace commands.

### Modified Capabilities

- `agent-namespace-runtime`: Replace workspace-bound runtime assembly with
  Process namespace and descriptor authority.
- `agent-root-layout-contract`: Resolve Agent Definitions explicitly instead of
  Host-directory overlays.
- `governance-tooling-contract`: Remove workspace-local Tool identity, routing,
  cwd, and sandbox authority.
- `host-directory-mounts`: Require explicit Host Mount grants with no implicit
  workspace seed.
- `provider-connection-contract`: Remove Alan-home profile authority and define
  bounded migration toward service-owned metadata and Host-owned secrets.
- `runtime-memory-contract`: Remove workspace-directory memory ownership.
- `skill-system-contract`: Remove workspace, AgentRoot, and user-directory
  implicit Skill discovery.
- `workspace-runtime-state-hygiene`: Retire the workspace state model rather
  than preserving it under new paths.

## Impact

Touches `crates/alan`, `crates/agent-engine`, Tool/sandbox configuration,
Agent/Skill/connection resolution, memory and rollout paths, cleanup tooling,
CLI tests, canonical specs, and the blocked package-management change. This is
the required predecessor of `extract-system-level-alan-os-host`.
