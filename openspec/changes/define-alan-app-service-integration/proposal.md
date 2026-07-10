## Why

Alan Apps and host-backed capabilities currently have no shared Alan OS
integration contract, so older changes independently invent environment objects,
session APIs, typed RPC providers, or direct runtime bridges. The Plan 9-like
architecture needs one durable rule: app and host domain behavior enters Alan OS
as mountable file trees, and Agent Processes receive bounded descriptors and are
created by spawn.

## What Changes

- Add a shared integration contract for Alan Apps with app-owned domain cores and
  aP file-server adapters.
- Define the service rendezvous and mount convention: a service posts an
  access-filtered handle under `/srv/<service-name>` and Service Manager mounts
  its user-facing tree under `/mnt/<service-name>` unless a capability-specific
  contract defines a more precise path.
- Define host-backed services as ordinary Alan OS file servers even when their
  implementation calls platform frameworks, XPC helpers, device SDKs, or other
  host-local mechanisms behind the adapter.
- Require app state, operations, observation, and lifecycle control to use files,
  streams, executable files, and owning `ctl` surfaces rather than app-facing or
  agent-facing RPC/session APIs.
- Require Alan Apps to give agents bounded context by opening descriptors,
  assembling a namespace, and spawning an Agent Executable. Apps do not call an
  embedded chatbot or agent RPC API.
- Define app and host UI clients as file clients over the same tree used by
  Tools and Agent Processes; UI snapshots may be derived projections but never a
  second source of domain truth.
- Make compatibility bridges explicit, named, and deletion-bound when a current
  host cannot yet consume the file tree directly.

## Capabilities

### New Capabilities

- `alan-app-service-integration`: Defines app-owned domain authority,
  host-backed file-server adapters, `/srv` rendezvous, `/mnt` mounts, file and
  stream operation semantics, descriptor passing, Agent Executable spawn, and
  compatibility-bridge constraints for Alan Apps.

### Modified Capabilities

None. This contract consumes the accepted `plan9-kernel-substrate`,
`agent-file-layout-contract`, and target crate architecture without changing
their requirements.

## Impact

- Future Alan App changes such as Groove Master and UPDF must identify their
  domain core, file-server adapter, mounted tree, executable surfaces, and agent
  spawn boundary.
- Host-backed capabilities such as Alan Voice and Matter control must present aP
  file trees to Alan OS even when platform-specific work remains in Alan for
  macOS.
- Memory and model-routing changes must stop extending daemon/session APIs and
  instead compose with Memory Store and LLM file-server trees.
- The `alan` binary and Service Manager own service startup, handle posting, and
  mount assembly; Alan Kernel remains unaware of app and host domains.
