## Why

After `remove-daemon-era-contracts`, retaining the daemon server or its Session-centered object
model would leave the rejected architecture in executable form and invite new dependencies on it.
The repository needs a clean break even though that temporarily removes working compatibility
features and persisted continuity.

## What Changes

- Depend on `remove-daemon-era-contracts` and deliver as its immediately following stacked change;
  both changes are reviewed before the contracts change merges.
- **BREAKING** Delete `crates/alan/src/daemon/`, the `alan daemon` command, HTTP/WS session and
  connection APIs, relay/remote control, scheduler/task/session stores, daemon host configuration,
  API manifests, and daemon-only dependencies.
- **BREAKING** Delete daemon consumers from Alan for macOS, including `AlanAPIClient`, daemon DTOs
  and reducers, legacy Console sources, and daemon-backed Settings connection/skill/system rows.
  Do not replace them in this change.
- Remove Session identity beyond the daemon directory: Agent Engine `Session`, runtime
  `session_id`, session binding/resume/fork paths, `SessionMeta`, `EventEnvelope.session_id`, TUI
  `SessionReducer`, session-shaped generated paths, and session-centered Memory Store surfaces.
- Re-home surviving state in existing owners without introducing a replacement center object:
  Process/Agent Process, Agent Machine, Tape, turn, rollout/checkpoint files, and Memory Stores.
- Retain Event/Op types that remain the Agent Execution Engine alphabet, but remove REST/WebSocket,
  session/client transport assumptions and delete unused protocol members.
- Remove or simplify scripts, tests, fixtures, environment variables, help, examples, dependencies,
  and build recipes that reference the deleted architecture; add guards against its return.
- Actively delete recognized stable/dev daemon bindings, session rollouts, generated session memory,
  and related local state as a one-time implementation step. Do not ship a migrator, compatibility
  reader, backup format, cleanup command, or startup-time legacy detector.
- Preserve immutable OpenSpec archive history and unrelated Apple `LaunchDaemon` support.
- Keep Alan for macOS-to-Alan OS attachment design out of scope.

## Capabilities

### New Capabilities

None. This change implements the canonical removals and ownership boundaries established by
`remove-daemon-era-contracts`.

### Modified Capabilities

- `documentation-governance`: Add a durable repository guard that rejects live daemon-era
  contracts, code, commands, configuration, tests, and consumers while excluding immutable
  OpenSpec archive history and unrelated Apple `LaunchDaemon` terminology.

## Impact

- Rust: `crates/alan`, `crates/agent-engine`, `crates/agent-protocol`, `crates/tui`, AgentFS-facing
  adapters, Cargo features/dependencies, tests, and release CLI behavior.
- Apple: daemon services/models/controllers, legacy Console sources, Settings projections, Xcode
  source membership, focused tests, build guards, screenshots, and docs.
- Local state: recognized stable and dev daemon/session state is destructively removed once during
  implementation with no recovery or migration promise.
- Product behavior: daemon HTTP/WS/relay/remote/mobile Console, scheduling extensions, session
  resume/fork, and daemon-backed macOS management temporarily disappear.
- Preserved behavior: file-backed TUI, direct CLI workspace/connection/skill management, Agent
  Execution Engine capabilities expressed through Process/AgentFS surfaces, and the macOS terminal
  workspace independent of daemon data sources.
