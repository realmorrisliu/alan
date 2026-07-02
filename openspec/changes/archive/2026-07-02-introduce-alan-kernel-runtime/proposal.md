## Why

This change was originally framed around an older kernel ontology (a first-class
`Agent Process` kernel category, typed opaque ids, an Activity Ledger / Kernel
Journal, and Object / Buffer / View / Command / Query / Subscription / Task /
Artifact / Evidence surfaces) and a renderer model that pulled "semantic view
snapshots" from a core. ADR-0024 retires that ontology in favor of a Plan 9
substrate (`define-plan9-kernel-substrate`) and an agent file-layout convention
(`define-agent-file-layout-contract`).

This change is therefore re-scoped as the **migration slice**: it maps the
current Agent Execution Engine and session protocol onto the new agent process
file surfaces, as a user-space file server above the substrate, while the kernel
ontology itself is owned by the substrate change. It also **absorbs and
supersedes `add-agent-runtime-service-filesystem`**, whose AgentFS projection it
now owns (via `alan-agent-adapter-contract`); that change is removed to avoid two
owners of the same projection.

As of `refactor-engine-namespace-native`, the remaining engine-native rewrite
work (live namespace wiring, `io/input` resume, overlay usage, file-backed LLM
generation, tools, and direct agent-file writes) is superseded by that change.
This change remains historical context for the ADR-0024 migration and should not
receive new engine-runtime implementation tasks.

## What Changes

- Re-own the kernel ontology: the `alan-kernel-contract` capability is reduced to
  a superseded pointer; the durable kernel contract lives in
  `define-plan9-kernel-substrate`.
- Retire the semantic-snapshot renderer model: the `alan-renderer-host-contract`
  capability is reduced to a superseded pointer; renderer hosts are clients that
  read `/proc` and `/agent` files and write `ctl`.
- Keep and realign the compatibility projection (`alan-agent-adapter-contract`):
  create each session's backing process via `/proc/clone` (kernel-rendered in
  `/proc`) and serve its agent surfaces under `/agent`, conversation → `io/`, tape →
  `machine/`, yields →
  `requests/`, tool calls → `actions/`, recovery → `machine/` checkpoints.
- Keep the current `crates/tui` compatibility transport working throughout, with
  its target being direct file reading rather than a private application model.

## Capabilities

### New Capabilities

- `alan-agent-adapter-contract`: the compatibility projection that maps the
  current Agent Execution Engine and session protocol onto the agent file-layout
  contract, without making `alan-kernel` depend on `alan-protocol`, transport
  clients, providers, memory stores, or sandbox backends.

### Modified Capabilities

- None.

### Retired / Superseded In This Change

- `alan-kernel-contract`: superseded by `define-plan9-kernel-substrate`
  (ADR-0024). Reduced to a pointer.
- `alan-renderer-host-contract`: the "semantic view snapshot" pull model is
  retired; renderer hosts read files and write `ctl`. Reduced to a pointer.

## Impact

- `alan-kernel` (the crate) is created for the substrate only (namespace engine,
  fid / file-server contract, process table, `/proc`, `/srv`); there is no current
  `alan-kernel` crate to clean up (the V1 one was removed). Any compatibility code
  that must move comes from the actual current owners (`alan-runtime`,
  `alan-protocol`, `crates/alan`, `crates/tui`) into the projection or
  `alan-compat`, not the kernel.
- `crates/tui` remains on the current compatibility transport and migrates toward
  reading agent files and writing `ctl`.
- The current Agent Execution Engine, session/transport implementation, and
  `alan-protocol` remain intact during migration as internal compatibility
  details behind the projection file server.
- Roadmap: this slice proves the engine → file-surface migration; broader Alan
  Shell, macOS host, Groove Master, and UPDF work follows once file parity holds.
- ADRs: implements the migration parts of ADR-0024; depends on
  `define-plan9-kernel-substrate` and `define-agent-file-layout-contract`.
