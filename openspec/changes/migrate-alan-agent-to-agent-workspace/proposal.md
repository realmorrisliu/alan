## Why

Alan Agent should become the built-in Agent Workspace inside Alan OS: the
place users inspect, steer, and organize agent sessions, Agent Runs,
supervisor-raised tasks, memory, evidence, and cross-app work. Today it is still
shaped around daemon sessions, protocol events, and TUI transcript projection.
The migration should preserve that working product while projecting it into the
new Alan OS model.

## What Changes

- Define Alan Agent as a built-in Alan App / Agent Workspace over Agent Runs,
  Agent Capability Service, memory layers, evidence, and task projections.
- Preserve current daemon-backed session behavior as a compatibility authority
  while semantic Agent Workspace projection is introduced.
- Map existing conversations, turns, tool calls, approvals, child runs,
  rollout evidence, memory events, and plans into Agent Workspace objects,
  buffers, views, commands, tasks, forms, evidence, and audit surfaces.
- Use Alan TUI as the first host to render Agent Workspace projections, with
  Alan for macOS following the same host contract later.

## Capabilities

### New Capabilities

- `alan-agent-workspace`: Defines the built-in Alan Agent App as the
  user-visible Agent Workspace for inspecting and steering agent work on Alan
  OS.

### Modified Capabilities

- `alan-agent-adapter-contract`: Alan Agent projection becomes the first
  concrete Agent Workspace path.
- `rust-inline-tui`: Alan TUI becomes the first compatibility host for Agent
  Workspace projections.

## Impact

- Affected crates: future `alan-agent` app module, `crates/tui`, daemon client
  and session projection code, and semantic Kernel snapshot consumption.
- Affected behavior: existing Alan Agent session UX should remain compatible
  while semantic projection is introduced.
- Affected future hosts: Alan for macOS can consume the same Agent Workspace
  semantic projections later.
