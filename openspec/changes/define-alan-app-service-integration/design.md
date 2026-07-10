## Context

ADR-0024 reduces Alan Kernel to namespace, mounts, files, descriptors,
credentials, one Process category, `/proc`, and `/srv`. ADR-0025 maps every
mountable tree to a file-server owner, while ADR-0027 requires every extension
to be expressible as a file operation or namespace interposition. Alan Apps are
expected to keep app-owned domain cores and integrate through Alan adapters.

Several older open changes predate that model. Groove Master describes an
environment object/buffer/view/query framework; Alan Voice routes typed intents
through session/runtime bridges; the Matter spike points toward a typed RPC/tool
provider; proactive memory makes runtime the storage authority; cognitive model
routing extends session and daemon metadata. They need one shared replacement
boundary rather than five local reinterpretations.

The common shape is:

```text
app or host domain core
        │ private implementation calls
        ▼
aP file-server adapter ──posts──> /srv/<service-name>
        │
        └──mounted by Service Manager──> /mnt/<service-name>
                                          │
                    ┌─────────────────────┼─────────────────────┐
                    ▼                     ▼                     ▼
              Alan for macOS        Tool Process         Agent Process
              file client           /bin executable      bounded namespace
```

## Goals / Non-Goals

**Goals:**

- Give every Alan App and host-backed capability an explicit domain owner and
  file-server adapter boundary.
- Standardize `/srv` rendezvous and `/mnt` mount placement without creating new
  top-level namespace roots.
- Make state, operations, observation, and lifecycle control available through
  files, streams, executable files, and owning `ctl` surfaces.
- Make bounded descriptors plus Agent Executable spawn the only canonical app-to-
  agent integration path.
- Permit platform-specific implementation mechanisms behind the adapter without
  exposing them as Alan OS APIs.
- Make temporary compatibility bridges visible and deletion-bound.

**Non-Goals:**

- Add Alan App semantics to Alan Kernel.
- Require every app domain core to be written in Rust.
- Define one universal schema for all app data.
- Turn `/mnt` into a package manager or persistent object database.
- Require Alan for macOS to finish its full file-client migration in this
  change; ADR-0025 keeps that host line parked.
- Define product behavior for Groove Master, UPDF, Alan Voice, Matter, memory,
  or cognitive routing.

## Decisions

### 1. Domain cores own meaning; adapters own namespace projection

An Alan App owns its domain model, invariants, persistence rules, and product
language. Its Alan adapter exposes that domain as a file tree and translates aP
operations into domain operations. Domain cores should remain file-unaware where
practical; the adapter depends on aP plus its own backend and does not move app
semantics into Alan Kernel.

Alternative considered: standardize all apps on generic Object, Buffer, View,
Command, and Query types. Rejected: ADR-0024 retired those as Kernel concepts,
and a universal semantic object framework would recreate the architecture the
Plan 9 model replaced.

### 2. Services rendezvous under `/srv` and mount under `/mnt`

The default service name is a stable kebab-case identifier. A running service
posts an access-filtered handle at `/srv/<service-name>`. Service Manager mounts
the tree at `/mnt/<service-name>` unless a capability-specific accepted contract
defines another path, such as `/mnt/llm` or `/mnt/mem`. App packages and manuals
remain under `/lib` and `/man`; no app receives a new top-level root.

`/srv` is discovery, not authority. A process can mount a posted handle only when
that handle is visible with sufficient rights in its own `/srv` view. Normal app
use occurs through the mounted `/mnt` tree.

Alternative considered: reserve `/app` as a new root. Rejected: the Standard
Namespace deliberately keeps top-level roots small and places service trees
under `/mnt`.

### 3. Host-backed integrations are file servers at the Alan OS boundary

A service may call Apple frameworks, XPC helpers, device SDKs, speech providers,
or other host-local mechanisms internally. Those calls are implementation
details behind the adapter. To Alan OS clients, Tools, and Agent Processes, the
service speaks aP and exposes a file tree. An internal XPC or RPC hop does not
become a public Alan API, capability token, or agent integration surface.

Alternative considered: expose a typed local RPC provider and wrap it with Tools
later. Rejected: it makes RPC the real authority and files a secondary veneer,
violating the namespace iron law.

### 4. File shape follows the owning semantic

Services expose readable state as files and directories. Mutable documents use
write-and-clunk commit semantics. Ordered observation uses append-only stream
files with offsets and blocking reads. Lifecycle-bearing objects place a `ctl`
beside their state, and `ctl` accepts only commands owned by that object. Reusable
actions are Tool executables bound into `/bin`; Agent Executables are separate
spawn targets in the `/bin` union.

A service must not recreate generic command/query/subscription registries.
Queries are reads; observation is a blocking stream read; commands are document
writes, owning `ctl` writes, or spawned executables.

### 5. App-to-agent integration is descriptor passing plus spawn

An app that needs agent work opens only the files, directories, streams, Skills,
Memory Stores, policy files, and Tool executables required by the task. It then
constructs a bounded child namespace and spawns an Agent Executable through the
normal process-creation path. The Agent Process receives descriptors and mount
visibility, not an app API token or opaque globally resolvable object id.

Results return through the spawned process's `io/output`, action records, and
files the app deliberately made writable. App-specific review workflows may
stage proposed changes in app-owned files before a human commits them.

Alternative considered: embed an agent engine in each app or call a daemon
session API. Rejected: both create a second agent platform and bypass AgentFS,
namespace governance, and normal process inspection.

### 6. Humans, Tools, and agents consume the same authority tree

Alan for macOS and other renderers may build host-local view models and cached
snapshots for presentation, but those are derived from the app/service tree and
must not become a second domain source of truth. A UI action ultimately performs
the same file write, `ctl` write, or executable spawn available to another
authorized client.

This does not force every current macOS surface to become file-native
immediately. A compatibility adapter may translate current host actions to the
canonical tree while the host line remains parked, subject to Decision 8.

### 7. Durability belongs to the service that owns the files

The Kernel persists no app state. A service declares which files are durable,
their retention and transaction rules, and how restart reopens the same backing
tree. Host framework storage, Application Support, databases, or content-
addressed blocks may back the tree, but clients observe ordinary file semantics.

### 8. Compatibility bridges are named and deletion-bound

A bridge that still accepts a daemon DTO, host callback, or legacy
`ContentInstance` action must be named as compatibility code, translate into the
canonical file operation, and identify the condition that deletes it. New
capabilities cannot add behavior available only through the bridge, and remote
clients cannot depend on it.

## Risks / Trade-offs

- [Risk] The default `/srv/<name>` and `/mnt/<name>` convention collides with a
  future system service → Mitigation: service names are stable capability ids;
  capability-specific specs may reserve a more precise accepted path before
  implementation.
- [Risk] A file tree becomes a thin wrapper over an RPC-shaped API → Mitigation:
  specs must define inspectable state, document commit, stream observation, and
  executable semantics rather than one request file per method.
- [Risk] Host processes retain authority beyond the mounted namespace →
  Mitigation: treat host confinement and secret storage as a second enforcement
  layer; do not claim mount visibility alone constrains privileged platform code.
- [Risk] Descriptor assembly becomes repetitive across apps → Mitigation: later
  helpers may construct namespaces, but the resulting mounts and descriptors
  remain explicit and inspectable.
- [Risk] Compatibility bridges linger → Mitigation: every bridge names its
  consumer, canonical replacement, and deletion gate in its owning change.

## Migration Plan

1. Land this shared contract without changing Kernel primitives.
2. Rewrite proactive memory and cognitive routing against their existing
   `/mnt/mem` and `/mnt/llm` service owners.
3. Rewrite Groove Master as an Alan App with an app-owned domain tree and
   descriptor-spawn producer agent.
4. Rewrite Alan Voice and Matter as host-backed file-server adapters while
   preserving their platform-specific product and spike goals.
5. Align component-system and UPDF wording; keep any parked macOS bridge
   explicitly transitional.
6. Sync the accepted capability into `openspec/specs/` before dependent changes
   archive.

## Open Questions

- Whether a future app registry should provide discoverable metadata under an
  existing `/lib` package tree; it is deliberately not required for service
  operation in this change.
- Whether Swift-hosted aP adapters share one reusable bridge library or use a
  narrow Rust host process; this is an implementation choice for the first
  host-backed service change.
