## Why

The current `alan` process privately boots Kernel, file servers, Root Agent,
Agent Execution Engine, and renderer together. Alan OS needs one durable system
authority per user/device/install channel that CLI, macOS, and future hosts can
attach without owning or duplicating its lifecycle.

## What Changes

- Add a dedicated Alan OS Host process with at most one active instance per
  user, device, and install channel; stable and dev remain isolated.
- Move fixed system boot composition out of the CLI and Agent Execution Engine
  into the Host as a temporary composition owner until Service Manager lands.
- Export the ready Standard Namespace root over the existing aP wire protocol
  on a user-protected, channel-specific Unix domain socket.
- Authenticate the local peer at the Host boundary without projecting Host OS
  UID, home, or directory identity into Alan OS.
- Define boot identity and readiness: required system trees and `/agent/root`
  must be readable before the Host accepts attachments.
- Make `alan` a Host Command Plane bootstrap/attach client and Alan Shell entry;
  app-owned product runtimes are forbidden. Explicit ephemeral Hosts remain
  test-only.
- On Host restart create a new Process table and Root Agent Process; continuity
  comes from durable stores, never Process deserialization.

## Capabilities

### New Capabilities

- `alan-os-host-lifecycle`: Singleton Host ownership, boot identity, readiness,
  shutdown, and test-only ephemeral hosting.
- `local-alan-os-attachment`: Unix-socket discovery, peer authorization, aP
  namespace export/import, disconnect, and reattachment semantics.

### Modified Capabilities

- `ap-wire-transport`: Apply the existing generic byte transport to the local
  Alan OS namespace root.
- `alan-shell`: Make `alan` boot/attach the system Host and enter Shell rather
  than privately booting an Agent renderer runtime.
- `agent-namespace-runtime`: Move system composition ownership out of Agent
  Execution Engine while retaining file-native runtime behavior.
- `macos-app-instance-lifecycle`: Reserve independent stable/dev Alan OS Host
  identities instead of app-owned system instances.

## Impact

Adds a Host executable/library and platform lifecycle adapters; changes CLI
startup, runtime assembly, socket discovery, install-channel paths, process
tests, and packaging. Depends on `remove-workspace-runtime-model`; precedes
`implement-minimal-service-manager`.
