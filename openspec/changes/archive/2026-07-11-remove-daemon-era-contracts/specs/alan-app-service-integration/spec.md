## ADDED Requirements

### Requirement: Alan Apps keep domain authority above Kernel
An Alan App SHALL own its domain model, invariants, persistence rules, and product semantics outside
Alan Kernel. Its Alan adapter SHALL project that domain through aP without introducing app-specific
Kernel primitives or a generic Object, Buffer, View, Command, Query, Subscription, or Artifact
ontology into Kernel.

#### Scenario: An app domain is integrated
- **WHEN** an Alan App exposes its domain to Alan OS
- **THEN** the app domain core remains the authority for domain meaning
- **AND** Alan Kernel sees only files, directories, streams, descriptors, namespaces, mounts,
  credentials, and Processes

### Requirement: App and host services expose aP file trees
Every Alan App service or host-backed capability exposed to Alan OS SHALL provide an aP file-server
adapter. Platform frameworks, XPC helpers, device SDKs, databases, or private implementation calls
MAY exist behind that adapter, but SHALL NOT be the app-facing, Tool-facing, Agent Process-facing,
or remote Alan OS contract.

#### Scenario: A host framework backs a service
- **WHEN** a host integration uses a platform framework to implement speech, device, document, or
  other host behavior
- **THEN** authorized Alan OS clients operate that behavior through the service's aP file tree
- **AND** no client needs the private host call shape or an opaque capability token

### Requirement: Services rendezvous under srv and mount under mnt
A service SHALL post an access-filtered handle at `/srv/<service-name>`. Service Manager SHALL mount
its normal client tree at `/mnt/<service-name>` unless an accepted capability-specific contract
defines a different path. App services SHALL NOT add new top-level namespace roots.

#### Scenario: Service Manager starts an app service
- **WHEN** a configured app or host service becomes ready
- **THEN** it posts its handle under `/srv/<service-name>`
- **AND** Service Manager mounts the tree under `/mnt/<service-name>` or the capability's accepted
  alternate path

#### Scenario: A process cannot see a service handle
- **WHEN** a process's filtered `/srv` view omits a service handle
- **THEN** the process cannot mount that service through ambient addressing
- **AND** an opaque service id does not bypass namespace reachability

### Requirement: Service trees use file-native operation semantics
An app or host service SHALL expose inspectable state through files and directories, atomic mutable
documents through write-and-clunk commit, ordered observation through append-only stream files, and
lifecycle control through a `ctl` beside the lifecycle-bearing object. Reusable actions SHALL be
executable files bound into `/bin` when they are Tools or Agent Executables.

#### Scenario: A client observes service activity
- **WHEN** a client needs current service state and subsequent changes
- **THEN** it reads snapshot files and blocks on an offset-resumable events or log stream
- **AND** it does not create a subscription object or poll a side-channel service API

#### Scenario: A client commits a domain mutation
- **WHEN** the service models a mutation as a writable document
- **THEN** the client writes the whole document and clunks to commit it
- **AND** partial writes do not apply partial domain state

#### Scenario: A client controls a running domain object
- **WHEN** a lifecycle-bearing object supports start, stop, cancel, or another owner-defined control
- **THEN** the client writes the command to that object's adjacent `ctl`
- **AND** the service does not expose the control only as a side-channel method

### Requirement: Service trees do not recreate generic operation registries
App and host service contracts SHALL express queries as file reads, observation as blocking stream
reads, mutations as document or owning `ctl` writes, and actions as executable process spawn where
appropriate. They SHALL NOT require a generic command, query, subscription, or method registry as
the canonical operation surface.

#### Scenario: A new app action is designed
- **WHEN** an app needs an operation available to humans, Tools, or agents
- **THEN** the design assigns it to a readable or writable file, an owning `ctl`, or an executable
  file
- **AND** it does not add a side-channel method solely for that operation

### Requirement: Apps pass bounded descriptors and spawn Agent Executables
An Alan App that requests agent work SHALL open only the files, directories, streams, Skills,
Memory Stores, policies, and Tool executables needed for the task, construct a child namespace from
resources it may delegate, and spawn an Agent Executable through normal Process creation. It SHALL
NOT embed an agent engine or use a product-facing method API as the app-to-agent boundary.

#### Scenario: An app asks an agent to review domain work
- **WHEN** an Alan App starts an Agent Process for a bounded review or mutation task
- **THEN** the spawned process receives only the descriptors and mounts required for that task
- **AND** the process is visible through `/proc` and the `/agent` overlay

#### Scenario: The agent returns a result
- **WHEN** the spawned Agent Process produces output or a proposed app mutation
- **THEN** the app reads `io/output`, action records, or app-owned proposal files available in its
  namespace
- **AND** no private result method or globally resolvable app object id is required

### Requirement: UI, Tools, and Agent Processes share the same authority tree
Alan for macOS, other renderers, Tool Processes, and Agent Processes SHALL derive domain behavior
from the same mounted service tree. Host-local view models, caches, and snapshots MAY optimize
presentation but SHALL remain projections and MUST NOT become a second source of domain truth. This
requirement does not choose how any host attaches to Alan OS.

#### Scenario: A UI invokes an app operation
- **WHEN** a user acts through an Alan App surface
- **THEN** the UI ultimately performs the same authorized file write, `ctl` write, or executable
  spawn available to another file client
- **AND** the authoritative result is readable from the service tree

### Requirement: Service owners define durability and retention
The file server that owns an app or host tree SHALL define which files are durable, how writes
commit, how restart reopens the backing tree, and how retention or garbage collection affects
references. Alan Kernel SHALL NOT persist app state.

#### Scenario: Alan OS restarts
- **WHEN** Kernel process, namespace, and fid state is recreated after restart
- **THEN** a durable app service reopens its own backing state and reposts its handle
- **AND** Service Manager remounts the tree without Kernel understanding the app storage format
