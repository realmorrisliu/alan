# add-alan-package-management — design

## Context

The accepted Alan OS model now has a Standard Namespace, one Process ontology,
Service Manager, explicit Host Mounts, channel System Stores, descriptor-passed
Agent Definitions and Skills, and no workspace runtime identity. Package
management must compose with those owners.

Current code still has one package bypass: `ResolvedCapabilityView` appends
compiled-in first-party packages through `builtin_capability_packages()`.
There is otherwise no installed-package service, durable catalog, command
surface, or `/lib/pkg` lifecycle.

The prior package-management draft predates the system model. Its implicit
AgentRoot/workspace/`~/.agents` providers, `~/.alan/pkg` backing, Host-side Q
resolver, and special helper execution path are rejected by ADR-0052 and are
not migration inputs for this design.

## Goals

- Give installed packages one system owner and one durable lifecycle.
- Make install inputs and runtime package access explicit namespace authority.
- Remove the compiled-in first-party Skill resolution bypass.
- Provide a useful v0 for installing, inspecting, upgrading, referencing, and
  uninstalling Skill distribution packages.
- Preserve Alan Kernel, Service Manager, Agent Runtime Service, AgentFS, Host
  Mount, and System Store ownership boundaries.

## Non-goals

- Ambient Host-directory discovery or compatibility overlays.
- Remote URL fetching, Git credentials, registries, signing, lockfiles,
  dependency solving, or semantic-version selection.
- Tool/binary, MCP, service, model, workflow, or knowledge package types.
- Executing package-local helper scripts through a special Host resolver.
- Mutating the package set of an already-running Agent Process.
- Making all installed packages visible to every Process.
- Replacing explicit Skill or Agent Definition descriptors.

## Decisions

### D1. Package Service is the sole installed-package authority

The formal component is **Package Service**; **Quartermaster** is its product
name and `q` is its command. Service Manager starts it from `/lib/boot`,
supervises its Process, and publishes its mountable handle at `/srv/package`.

The service owns:

- package ids and catalog records;
- imported immutable revisions and content fingerprints;
- materialization manifests and generated adapter files;
- explicit revision references;
- transaction staging, publication, retirement, and deletion.

Alan Kernel knows none of these concepts. The Host only supplies the service's
private System Store backing and native storage adapter.

### D2. `q` is a Process over files, not a Host management authority

`/bin/q` is mounted in the Shell namespace. Alan Shell resolves and spawns it
through `/proc/clone`, waits on `/proc/<pid>`, and renders its output like any
other command. The shell remains a generic aP client; it contains no
package-specific management logic.

The v1 same-address-space implementation may host the `q` Process image beside
Package Service, but the image communicates through the service's aP file
surface. Tests exercise the same serialized commands and results exposed at
`/srv/package`.

The stable service tree is:

```text
/srv/package/
  catalog       # read-only JSON snapshot
  status        # readiness and last transaction summary
  ctl           # commit-on-clunk command input
  result        # bounded result records keyed by request id
```

Shell-facing `q` verbs are `install`, `list`, `upgrade`, and `uninstall`.
Internal reference acquisition/release operations use the same service surface
but are not presented as a second end-user package manager.

### D3. Sources are namespace trees, never Host identities

`q install <namespace-path>` and
`q upgrade <package-id> <namespace-path>` require an absolute readable Alan OS
path. A Host directory must first be authorized and projected by Host Mount,
normally under `/mnt`. The `q` Process walks and reads the source through aP and
submits an imported snapshot to Package Service.

Package ids are 1–64 ASCII bytes matching
`[a-z0-9]+(?:-[a-z0-9]+)*`. They are compared byte-for-byte without case
folding or Unicode normalization. `q install` either validates an explicit
`--name` exactly or derives the source leaf and rejects it when that leaf is not
already canonical.

The snapshot contains normalized relative paths, file bytes, and executable
metadata where portable. It contains no absolute source path. Package Service
rejects `.`/`..`, absolute entries, duplicate normalized paths, special files,
escaping symlinks, and bounded-input violations before staging any revision.
VCS control directories are excluded.

Remote URLs are not accepted in v0. A later network-fetch service may produce a
namespace tree and pass it through this same import boundary; `q` must not gain
ambient Host network or credential authority to implement fetching itself.

### D4. System Store data is service-private and revisioned

Package Service owns the `services/packages` subtree of the active channel's
System Store. A logical layout contains a catalog, transaction staging, and
immutable revisions, but filenames and serialization below the service root
are implementation detail.

Each installed package record contains:

- canonical package id;
- current content fingerprint revision;
- install kind (`preinstalled` or `installed`);
- materializer version;
- exported Skill roots and typed availability issues;
- creation/update metadata without Host paths or credentials.

Publication is stage → validate → fsync/close → atomic catalog switch. A failed
transaction leaves the prior catalog and revision readable. Store corruption
fails closed and is reported; it is not silently reconstructed from an ambient
source.

### D5. A distribution package may export multiple ordinary Skill packages

The install unit is one source snapshot. Materialization finds supported Skill
inputs and emits zero or more ordinary directory-backed Skill package roots:

- a directory containing `SKILL.md` is copied without rewriting its canonical
  instruction file;
- an explicitly named portable Skill root containing a valid root `SKILL.md`
  needs no Alan-specific package manifest and is adopted as one distribution
  package without mutating the source;
- supported command-style `skills/*.md` files become one Skill package each,
  preserving the body verbatim after a versioned Alan adapter preamble;
- duplicate canonical Skill ids, invalid metadata, escaping links, and
  ambiguous overlapping inputs reject the transaction.

The materialization manifest lists every exported relative path and every
generated file. Shared assets may remain readable package content, but v0 does
not turn scripts or `bin/` entries into Tools. A Skill that requires an absent
Tool or runtime capability is installed but reported unavailable by the
existing Skill availability machinery.

### D6. Runtime access is by explicit immutable package reference

Installing a package changes the Package Service catalog only. It does not add
authority to a running Process.

At Process creation, the parent or Boot Unit may pass explicit installed
package references. Package Service resolves each reference to an immutable
revision handle. The spawner projects those handles read-only at
`/lib/pkg/<package-id>` and passes the manifest-selected Skill roots to Agent
Runtime Service. Agent Execution Engine builds its capability view from those
roots plus explicit Skill/Agent Definition descriptors.

Rules:

- an unreferenced installed package is absent from the Process namespace;
- a reference identifies a package id and immutable revision, not a backing
  path;
- package projection is read-only;
- Process launch rejects duplicate runtime Skill ids across the complete
  selected package-reference and descriptor set before capability assembly;
- a child inherits only the parent's referenced mounts/descriptors unless the
  parent explicitly narrows or adds authority through the normal launch path;
- removing or upgrading a catalog entry does not rewrite a running Process's
  namespace snapshot.

The v1 Host adapter may translate a resolved revision handle to private backing
for the existing path-based Skill loader. That translation is derived only
from Package Service's explicit reference and must not become a general Host
directory scanner or Agent-visible identity.

### D7. First-party Skills are preinstalled packages

Alan's embedded first-party Skill trees seed deterministic preinstalled
Package Service revisions. Seeding is idempotent by package id, materializer
version, and content fingerprint. A product update may publish a new revision;
running Processes keep their prior immutable handle.

The Root Agent Process Boot Unit explicitly references the first-party package
set. Agent Execution Engine no longer appends
`builtin_capability_packages()`. `builtin` remains a provenance/precedence tier
for Skill resolution, not a separate injection path.

### D8. Lifecycle is exact and conservative

`q install`:

- derives a canonical id from the source directory name or validates `--name`;
- rejects a collision before publication unless the id and fingerprint are
  already identical, in which case it is an idempotent success;
- validates and materializes the full snapshot before the catalog changes.

`q list` reports installed id, current revision, kind, exported Skills,
reference count, and availability issues from Package Service state.

`q upgrade` requires a new explicit namespace source. Identical content is an
idempotent no-op. Changed content publishes a new immutable revision and moves
the catalog's current pointer; existing references retain the old revision.

`q uninstall` removes the package from future resolution immediately. If live
references remain, its immutable revisions are retained and the command
reports `retiring`; final deletion occurs after the last reference is released.
Preinstalled packages cannot be uninstalled by v0 operator commands.

### D9. Failure remains visible and typed

Package Service rejects malformed snapshots, invalid ids, broken manifests,
and store-integrity failures before publication. Materialization compatibility
gaps that do not corrupt the package are recorded as typed availability issues
and flow into existing Skill exposure reports. No unsupported foreign
capability is silently rewritten into a weaker Alan behavior.

## Rejected alternatives

- **Scan workspace, AgentRoot, `~/.agents`, or Alan home directories.** This
  recreates ambient Host authority and contradicts ADR-0052.
- **Store packages in `~/.alan/pkg`.** System services own channel System Store
  subtrees; an Alan home is not the product model.
- **Let Agent Execution Engine own package lookup.** It would mix lifecycle
  authority into the transition loop and preserve the current bypass.
- **Expose all installed packages at `/lib/pkg`.** Installation is not Process
  authority.
- **Execute helper paths through a Host resolver.** Tools execute from `/bin`
  under Process authority; package distribution cannot add a parallel path.
- **Fetch Git URLs inside `q`.** Network and credentials require a separate
  adapter/service boundary and are not necessary to prove package ownership.

## Risks and mitigations

- **The current Skill loader is Host-path based.** Keep the adapter narrow and
  reference-derived, test that no raw path enters records/output, and replace
  it when the loader becomes fully aP-native.
- **Large source snapshots can exhaust memory.** Bound file count, per-file
  size, and total import bytes; stage incrementally where the backing adapter
  supports it.
- **Package id collisions can confuse provenance.** Reject different content
  under an occupied id unless the operator uses a distinct explicit name.
- **Concurrent commands can race.** Serialize catalog commits and use request
  ids with bounded retained results.

## Verification strategy

- Unit tests for ids, snapshot validation, materialization, fingerprints,
  atomic catalog updates, lifecycle, and restart recovery.
- File-server contract tests for `/srv/package` commit-on-clunk behavior.
- Service Manager boot/readiness/restart tests for the Package Service Process.
- Process tests proving referenced packages appear read-only at `/lib/pkg` and
  unreferenced packages do not.
- Agent-engine tests proving built-ins arrive only through explicit package
  roots and descriptor-passed Skills still work.
- Shell tests proving a generic `/bin/q` Process returns output through
  `/proc/<pid>/io/output`.
- Synthetic dogfood fixture with multiple Skills, shared assets, and one
  unsupported capability; no third-party content is committed.
