## ADDED Requirements

### Requirement: Package Service is the installed-package authority
Alan OS SHALL use one Package Service as the sole owner of installed package
identity, content, provenance, catalog state, and lifecycle. Host, Agent
Execution Engine, Alan for macOS, and other services MUST NOT maintain a second
installed-package registry or mutate package state outside Package Service.

#### Scenario: Client resolves an installed package
- **WHEN** a client requests an installed package by id
- **THEN** the result comes from Package Service
- **AND** no Host directory or engine-local registry is consulted

### Requirement: Package Service exposes a file-native service tree
Package Service SHALL publish an access-filtered handle at `/srv/packages`.
Service Manager SHALL mount its management tree at `/mnt/packages`. The tree
SHALL expose catalog snapshots, service-owned transaction directories with
adjacent `ctl` and `status` files, and an offset-resumable event stream.

#### Scenario: Client inspects installed packages
- **WHEN** an authorized client lists `/mnt/packages/catalog`
- **THEN** it can read each installed package's manifest, digest, provenance,
  and state
- **AND** inspection does not require a private method or Host API

#### Scenario: Client commits an install
- **WHEN** the client finishes staging a transaction and writes `commit` to the
  transaction's `ctl`
- **THEN** Package Service validates and atomically activates the package
- **AND** partial transaction content never becomes active

### Requirement: Alan packages use a bounded declarative manifest
An installable Alan package SHALL be a directory containing a versioned,
declarative `alan-package.yaml`. The manifest SHALL contain one package id of
1–64 ASCII characters matching `[a-z0-9]+(?:-[a-z0-9]+)*`. Package ids SHALL be
compared byte-for-byte without case folding or Unicode normalization. The
manifest MAY declare relative Skill export paths, explicit Tool exports, and a
human-facing version. It MUST NOT contain executable
manifest logic.

Every export SHALL canonicalize inside the package tree. Skill exports SHALL
satisfy `skill-system-contract`. A Tool export SHALL map one valid command name
to one executable inside the package and SHALL satisfy the existing `/bin` Tool
contract. Merely placing files under `bin/` or `scripts/` MUST NOT export them.

#### Scenario: Package exports multiple Skills
- **WHEN** one valid manifest lists several Skill package roots
- **THEN** Package Service records one installed package with those declared
  Skill exports
- **AND** it does not recursively discover undeclared Skill roots

#### Scenario: Package id is not canonical
- **WHEN** a manifest id contains uppercase letters, Unicode, `.`, `..`, `_`,
  repeated separators, or more than 64 characters
- **THEN** validation rejects the transaction before catalog or namespace use

#### Scenario: Export escapes the package
- **WHEN** an export is absolute, traverses outside the package, or resolves
  through an escaping symlink
- **THEN** validation rejects the transaction before activation

#### Scenario: Package contains an undeclared executable
- **WHEN** installed content contains `bin/helper` without a Tool export
- **THEN** the file remains package-local content
- **AND** it is not bound into `/bin`

### Requirement: Package input is explicit Alan OS namespace content
`pkg install` and `pkg update` SHALL accept only an explicitly named source tree
readable through the caller's Alan OS namespace. Package Service MUST NOT scan
workspace, AgentRoot, `.agents`, Alan home, raw System Store, or other Host
directories as package providers. Host content SHALL first enter through an
explicit Host Mount.

#### Scenario: Mounted Host repository contains Skills
- **WHEN** a Host repository is visible at `/mnt/host/repository` but no install
  command names it
- **THEN** Package Service does not discover or install it
- **AND** its Skills do not enter an Agent Process implicitly

#### Scenario: Source is a Host path
- **WHEN** a caller supplies `~/.alan/pkg` or another Host-only path
- **THEN** the command rejects it as outside the Alan OS namespace
- **AND** no compatibility path resolver is used

### Requirement: Package lifecycle is transactional and exact
Package Service SHALL copy explicit input into service-owned staging, compute a
content digest, validate all content and exports, and atomically activate one
revision per package id. Install SHALL reject an owned id. Update SHALL require
the expected active digest and atomically replace that revision. Remove SHALL
delete only Package Service-owned catalog and content for the named package.

Interrupted, aborted, or invalid transactions MUST NOT change active package
state. The original source tree MUST NOT be modified or removed.

#### Scenario: Install succeeds
- **WHEN** a staged package passes validation and its id is unowned
- **THEN** one atomic commit makes its catalog entry and content active
- **AND** the event stream records the new digest

#### Scenario: Concurrent update wins first
- **WHEN** an update's expected digest no longer matches the active digest
- **THEN** Package Service rejects the stale update
- **AND** the newer active package remains unchanged

#### Scenario: Remove succeeds
- **WHEN** an authorized client removes an installed non-required package
- **THEN** its catalog entry and service-owned content disappear atomically
- **AND** the mounted source used for installation remains untouched

### Requirement: Package content is projected through bounded namespaces
For each Process, namespace assembly SHALL project only explicitly selected
installed packages read-only at `/lib/pkg/<package-id>`. Unselected installed
packages MUST NOT appear in that Process's `/lib/pkg` view. Host backing paths
MUST NOT appear in package manifests, descriptors, reports, or namespace paths.

#### Scenario: Process receives one package
- **WHEN** a child Process is launched with package `research` selected
- **THEN** it can read `/lib/pkg/research`
- **AND** another installed but unselected package is absent

#### Scenario: Package content is mutated
- **WHEN** a Process attempts to write through `/lib/pkg/<package-id>`
- **THEN** the write is denied
- **AND** active Package Service content remains unchanged

### Requirement: Tool exports enter bin only by explicit selection
Namespace assembly SHALL bind a package Tool at `/bin/<tool>` only when the
manifest explicitly exports it and the launching authority selects that export
for the Process. Tool name collisions SHALL fail namespace assembly rather than
apply precedence. Package-local helpers MUST remain reachable only through the
selected package tree unless separately exported.

#### Scenario: Selected Tool is launched
- **WHEN** a selected package explicitly exports Tool `analyze`
- **THEN** the child Process receives `/bin/analyze`
- **AND** normal Process creation and descriptor rights govern its execution

#### Scenario: Two selected packages export one name
- **WHEN** both packages export `analyze`
- **THEN** namespace assembly rejects the collision
- **AND** it does not silently choose one package

### Requirement: Installed Skills are passed by descriptor
Package Service SHALL resolve only manifest-declared Skill exports. An Agent
Process SHALL receive a selected installed Skill through a bounded descriptor;
installation or a `/lib/pkg` mount MUST NOT cause recursive or ambient Skill
discovery.

#### Scenario: Agent receives one exported Skill
- **WHEN** launch selects one Skill export from an installed package
- **THEN** Package Service opens that export and the launcher passes its
  descriptor to the Agent Process
- **AND** sibling Skills are not activated implicitly

#### Scenario: Skill export is unavailable
- **WHEN** the package, declared export, or required projection is absent
- **THEN** launch fails closed with an inspectable availability error
- **AND** it does not scan Host directories for a substitute

### Requirement: Package management is an Alan Shell Tool
Alan OS SHALL provide the base-system `/bin/pkg` Tool with `install`, `list`,
`show`, `update`, and `remove` operations. The Tool SHALL operate through
`/mnt/packages` and normal namespace reads. Alan OS Host SHALL NOT require a
package-management startup mode or Host-side package catalog.

#### Scenario: Operator lists packages
- **WHEN** the operator runs `pkg list` in Alan Shell
- **THEN** the Tool reads Package Service catalog files
- **AND** it does not call a private Host method

#### Scenario: Package Service is unavailable
- **WHEN** `/mnt/packages` is absent or its handle is invalid
- **THEN** `pkg` reports the service as unavailable
- **AND** it does not fall back to Host state

### Requirement: First-party Skill packages use the ordinary lifecycle
First-party Skill packages SHALL use the same `alan-package.yaml`, Package
Service catalog, System Store transaction, namespace projection, and Skill
descriptor rules as third-party packages. Package Service SHALL install or
update required first-party package artifacts before reporting ready. Required
status MAY prevent removal but MUST NOT create a second discovery path or
privileged prompt behavior.

#### Scenario: Empty System Store boots
- **WHEN** Package Service starts with no installed first-party Skill packages
- **THEN** it transactionally installs the required base artifacts before ready
- **AND** later Skill discovery resolves them through Package Service

### Requirement: Package failures are fail-closed and inspectable
Package Service SHALL report invalid manifests, duplicate package ids, export
collisions, digest mismatch, corrupt backing content, and unsupported kinds as
readable
transaction or catalog status and MUST NOT create partial projections. Package
Service SHALL NOT execute install hooks or foreign conversion code.

#### Scenario: Active content fails digest verification
- **WHEN** Package Service detects that active content no longer matches its
  recorded digest
- **THEN** it marks that package unavailable and emits an event
- **AND** namespace assembly does not project the corrupted content

#### Scenario: Foreign source lacks an Alan manifest
- **WHEN** an operator installs a source tree without `alan-package.yaml`
- **THEN** validation rejects it with a manifest error
- **AND** Package Service does not guess or convert the foreign layout
