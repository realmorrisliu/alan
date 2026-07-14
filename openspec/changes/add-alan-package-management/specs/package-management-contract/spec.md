# package-management-contract Specification

## Purpose

Define Package Service as Alan OS's installed-package authority, including
explicit namespace source intake, System Store lifecycle, immutable revisions,
Process-scoped `/lib/pkg` projection, and the Quartermaster `q` command.

## ADDED Requirements

### Requirement: Package Service is a supervised system service

Alan OS SHALL run Package Service as a required File-Server Service started and
supervised by Service Manager. Its Process SHALL publish the mountable
`/srv/package` handle and SHALL report readiness through `/proc` and `/srv`.

#### Scenario: Alan OS boots successfully

- **WHEN** Service Manager completes required Boot Units
- **THEN** the Package Service Process is running under Service Manager
- **AND** `/srv/package` is present before dependent Shell or Agent Processes
  are declared ready

#### Scenario: Package Service exhausts restart policy

- **WHEN** Package Service repeatedly exits beyond its bounded restart budget
- **THEN** Service Manager reports the required unit failed
- **AND** Alan OS does not claim full system readiness

### Requirement: Package Service owns its durable state

Package Service SHALL own the package subtree of the active install-channel
System Store, including catalog records, staged transactions, immutable
revisions, materialization manifests, and reference-retirement state. No raw
backing path SHALL be part of package identity or an Agent-visible record.

#### Scenario: Stable and dev install the same package id

- **WHEN** stable and dev Package Services install the same package id
- **THEN** each service updates only its own channel System Store subtree
- **AND** neither channel can resolve the other channel's revision

#### Scenario: Agent inspects package metadata

- **WHEN** an Agent reads Package Service catalog data or projected content
- **THEN** it sees package ids, revisions, exports, and Alan OS paths
- **AND** it does not see the System Store backing path

### Requirement: Package Service exposes one file-native control surface

Package Service SHALL expose `catalog`, `status`, `ctl`, and bounded
request-keyed `result` data through `/srv/package`. Mutating commands SHALL
commit on clunk and malformed or unauthorized commands SHALL make the clunk
fail without a partial catalog change.

#### Scenario: Valid command commits

- **WHEN** a caller writes one valid request document to `/srv/package/ctl` and
  clunks the fid
- **THEN** Package Service commits exactly one transaction
- **AND** the matching result can be read by request id

#### Scenario: Invalid command is written

- **WHEN** a caller writes an unknown, malformed, oversized, or duplicate
  request document
- **THEN** clunk fails
- **AND** catalog and revision state remain unchanged

### Requirement: Install sources are explicit namespace snapshots

`q install` and `q upgrade` SHALL accept source content only from an absolute
readable path in the invoking Process namespace. The `q` Process SHALL import
normalized relative paths and bytes through aP. Package Service MUST NOT scan
workspace, AgentRoot, `.agents`, Alan home, or any other Host directory and
MUST NOT persist a raw Host path, URL credential, or VCS control directory.

#### Scenario: Host content is installed

- **WHEN** a user authorizes a Host Mount at `/mnt/import` and runs
  `q install /mnt/import/package`
- **THEN** `q` reads the source through its namespace
- **AND** Package Service persists an imported revision without the raw Host
  path

#### Scenario: Ambient Host directory contains Skills

- **WHEN** a workspace, AgentRoot, `.agents`, or Alan home directory contains
  `SKILL.md` but is not explicitly mounted and passed to `q`
- **THEN** Package Service does not inspect or install it

#### Scenario: Remote URL is supplied

- **WHEN** `q install` receives an HTTP, SSH, or Git URL instead of a namespace
  path
- **THEN** v0 rejects the source as unsupported
- **AND** `q` does not fetch through ambient Host network or credentials

### Requirement: Source snapshots are bounded and confined

Package Service SHALL reject absolute entries, parent traversal, duplicate
normalized paths, special files, excessive file counts, excessive per-file
size, and excessive total bytes before publication.
Snapshot entries SHALL represent regular files only. A source File-Server
adapter SHALL reject walking or reading a symbolic link without following its
target, and `q` SHALL abort the import on that rejection rather than serialize
dereferenced target bytes. Snapshot import SHALL preserve executable metadata
reported by the source File-Server. VCS control metadata SHALL not enter an
installed revision.

#### Scenario: Source symlink escapes its tree

- **WHEN** the selected source tree contains a symbolic link, whether its
  target is inside or outside the selected subtree
- **THEN** the source adapter does not expose target bytes through that entry
- **AND** `q` fails the import before catalog publication

#### Scenario: Source includes a Git control directory

- **WHEN** an imported tree contains `.git` metadata
- **THEN** that metadata is excluded from the snapshot and projection
- **AND** clone-local credentials cannot enter package content

### Requirement: Package ids and revisions are deterministic

Package ids SHALL be 1–64 ASCII bytes matching
`[a-z0-9]+(?:-[a-z0-9]+)*` and SHALL be compared byte-for-byte without case
folding or Unicode normalization. `q` SHALL derive the default id from the
source leaf name or validate an explicit `--name` without normalizing an
invalid value. A revision SHALL be a deterministic fingerprint of normalized
imported bytes, metadata, and materializer version.

#### Scenario: Source leaf is not a canonical package id

- **WHEN** `q install` derives a leaf containing uppercase, Unicode,
  separators, repeated dashes, or more than 64 bytes
- **THEN** installation fails before publication
- **AND** the diagnostic asks for a canonical explicit `--name`

#### Scenario: Different source occupies the derived id

- **WHEN** install derives an id already occupied by different content
- **THEN** install fails before write and asks for another explicit name

#### Scenario: Identical source is installed twice

- **WHEN** package id and fingerprint already match the current revision
- **THEN** install succeeds idempotently
- **AND** no duplicate revision or catalog mutation is created

### Requirement: Materialization exports ordinary Skill packages

A v0 distribution package SHALL export zero or more ordinary directory-backed
Skill packages. Package Service SHALL preserve native `SKILL.md` instruction
content, convert supported command-style Markdown with a versioned Alan adapter
preamble, reject ambiguous or duplicate Skill ids, and record all generated
and exported files in its materialization manifest.

An explicitly named portable Skill directory with a valid root `SKILL.md`
SHALL be installable without an Alan-specific manifest. Package Service SHALL
adopt the complete confined directory as one distribution package, derive the
existing normalized runtime Skill id from the source directory name, and SHALL
NOT mutate the source or recursively infer additional exports beneath that
portable root.

#### Scenario: Native Skill package is imported

- **WHEN** a confined source subtree contains a valid `SKILL.md`
- **THEN** its Skill package is exported without rewriting `SKILL.md`
- **AND** its relative package assets remain readable in the installed revision

#### Scenario: Bare portable Skill is installed

- **WHEN** the selected source root has one valid root `SKILL.md` and no Alan
  package manifest
- **THEN** Package Service installs it as one native Skill export
- **AND** no Alan-specific file is required in or written to the source

#### Scenario: Command-style Skill is imported

- **WHEN** a supported `skills/<name>.md` source is materialized
- **THEN** Package Service emits one directory-backed Skill package
- **AND** the original body follows a versioned Alan adapter preamble verbatim

#### Scenario: Package contains helper scripts

- **WHEN** a materialized Skill references package-local scripts or `bin/`
  content
- **THEN** the files may remain readable package assets
- **AND** v0 does not register or execute them as Alan OS Tools

### Requirement: Compatibility gaps remain visible

Package Service SHALL record unsupported Tool and runtime-capability
requirements as typed availability issues consumed by the existing Skill
resolution machinery. It MUST NOT silently emulate, delete, or weaken an
unsupported requirement.

#### Scenario: Foreign web capability is required

- **WHEN** a materialized Skill declares a web capability unavailable in its
  Agent Process
- **THEN** the Skill remains installed but unavailable
- **AND** package and Skill inspection identify the missing capability

### Requirement: Installed content is projected by explicit reference

Installing a package SHALL NOT itself grant Process access. At Process
creation, an explicit package reference SHALL resolve to one immutable revision
handle projected read-only at `/lib/pkg/<package-id>`. Unreferenced installed
packages SHALL be absent from that Process namespace. Before capability-view
assembly, Process launch SHALL reject duplicate runtime Skill ids across the
complete selected installed-package and explicit-descriptor set. Packages with
colliding Skill ids MAY remain installed when they are not selected together.

#### Scenario: Selected packages export the same runtime Skill id

- **WHEN** one Process launch selects two package or descriptor roots whose
  Skill ids normalize to the same runtime id
- **THEN** launch fails before Agent capability assembly
- **AND** neither package is made ambient or silently given precedence

#### Scenario: Process receives a package reference

- **WHEN** a parent or Boot Unit creates a Process with a valid installed
  package reference
- **THEN** the referenced revision is readable at `/lib/pkg/<package-id>`
- **AND** only manifest-selected Skill roots enter Agent capability resolution

#### Scenario: Process omits a package reference

- **WHEN** a package exists in the channel catalog but is not referenced at
  Process creation
- **THEN** the Process cannot read `/lib/pkg/<package-id>`
- **AND** its Agent capability view does not contain that package's Skills

#### Scenario: Process attempts to write package content

- **WHEN** a Process opens a projected package file for write
- **THEN** the namespace denies the operation

### Requirement: Package references are immutable Process authority

A resolved package reference SHALL identify an immutable revision. Upgrade or
uninstall SHALL affect future resolution but SHALL NOT rewrite an already
running Process namespace or capability view.

#### Scenario: Package is upgraded while an Agent runs

- **WHEN** Package Service publishes a new current revision
- **THEN** the running Agent keeps its referenced old revision
- **AND** a newly created Agent can resolve the new current revision

#### Scenario: Child inherits package authority

- **WHEN** a parent Process creates a child without changing package mounts or
  descriptors
- **THEN** the child inherits the parent's package-reference snapshot
- **AND** it gains no other installed package

### Requirement: First-party Skills are preinstalled packages

Alan OS SHALL seed first-party Skill trees as deterministic, ordinary
preinstalled Package Service packages. The Root Agent Process Boot Unit SHALL
reference them explicitly. Agent Execution Engine MUST NOT append a separate
compiled-in built-in package set during capability resolution.

#### Scenario: Empty channel boots after installation

- **WHEN** Package Service opens an empty channel store
- **THEN** it seeds the current first-party package revisions idempotently
- **AND** Root Agent references resolve through the ordinary package path

#### Scenario: Capability view is assembled

- **WHEN** Agent Execution Engine builds the Root Agent capability view
- **THEN** every first-party Skill came from an explicit Package Service
  reference
- **AND** no `builtin_capability_packages()` bypass adds another copy

### Requirement: Quartermaster is an Alan OS command

Alan OS SHALL bind `q` at `/bin/q`. Alan Shell SHALL launch it as an ordinary
Process through `/proc/clone` and render its output from
`/proc/<pid>/io/output`.
Package-specific command semantics SHALL stay out of Alan Shell.

#### Scenario: User lists packages in Alan Shell

- **WHEN** the user runs `q list`
- **THEN** Alan Shell resolves `/bin/q`, spawns a Process, and waits for its exit
- **AND** the output is derived from Package Service catalog data

#### Scenario: Package Service is unavailable

- **WHEN** `/bin/q` cannot reach the Package Service handle
- **THEN** the `q` Process exits non-zero with a bounded diagnostic
- **AND** Alan Shell remains attached to Alan OS

### Requirement: Package lifecycle is atomic and exact

`q install`, `q upgrade`, and `q uninstall` SHALL stage and validate all state
before atomically changing the current catalog. Upgrade SHALL require a new
explicit namespace source. Uninstall SHALL remove future resolution
immediately, retain revisions while live references exist, and delete managed
content after the final reference is released. Preinstalled packages SHALL not
be operator-uninstallable in v0.

#### Scenario: Upgrade source is unchanged

- **WHEN** `q upgrade` imports the current fingerprint
- **THEN** it reports an idempotent no-op
- **AND** the current revision and catalog generation do not change

#### Scenario: Upgrade fails validation

- **WHEN** a new source snapshot fails materialization or integrity checks
- **THEN** the previous current revision remains resolvable
- **AND** no partial new revision is published

#### Scenario: Installed package has live references

- **WHEN** `q uninstall` removes a package that running Processes reference
- **THEN** future resolution fails and the package reports `retiring`
- **AND** referenced immutable content remains until the last reference closes

#### Scenario: Preinstalled package is uninstalled

- **WHEN** an operator invokes `q uninstall` for a first-party preinstalled
  package
- **THEN** the command fails without changing the catalog
