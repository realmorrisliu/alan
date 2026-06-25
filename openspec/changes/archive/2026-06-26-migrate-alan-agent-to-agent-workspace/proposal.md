## Why

Alan Agent should remain built in, but it should not be required for agent work.
In the Plan 9-style Alan OS model, Alan Shell and apps can spawn Agent
Executables, inspect `/agent`, answer requests, and watch Agent Process events
directly through files and syscalls. Alan Agent's value is a richer optional
workspace over those same surfaces.

The migration should preserve the working current session experience while
projecting it toward Agent Process files instead of making Alan Agent the
runtime backend.

## What Changes

- Define Alan Agent as a built-in but optional Agent Workspace over Agent
  Processes, requests, actions, IO, machine state, memory, evidence, and
  cross-app work.
- Preserve current compatibility session behavior while Agent Process projection
  is introduced.
- Map current conversations, turns, tool calls, approvals, child agents, rollout
  evidence, memory events, and plans into `/agent/<pid>` workspace projections.
- Keep Alan Shell as the primary OS interaction surface; Alan Agent provides a
  richer workspace, not the only path.
- Keep Alan for macOS aligned with the same file/process surfaces.

## Capabilities

### New Capabilities

- `alan-agent-workspace`: Defines Alan Agent as the built-in optional Agent
  Workspace for inspecting and steering Agent Processes on Alan OS.

### Modified Capabilities

- `alan-agent-adapter-contract`: Compatibility projection maps current sessions
  into Agent Process files.
- `rust-inline-tui`: Alan Shell remains the first compatibility path for Agent
  Process projections.

## Impact

- Affected crates: future `alan-agent` app module, `crates/tui`, compatibility
  session projection code, and semantic Kernel snapshot consumption.
- Affected behavior: existing Alan Agent session UX should remain compatible
  while Agent Process projection is introduced.
- Affected future hosts: Alan for macOS can consume the same Agent Process and
  workspace projections later.
