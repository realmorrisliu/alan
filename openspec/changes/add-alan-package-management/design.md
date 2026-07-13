# Design: Package Service and System Store

## Context

The old change predated the current Alan OS ownership model. It treated package
management as a Host-side resolver named Quartermaster, stored content under
Alan home directories, registered workspace and AgentRoot directories as
providers, and fed resolved paths directly into Agent Execution Engine. That
would recreate the daemon-era split between Host paths and Alan OS authority.

The accepted architecture now has the necessary owners:

- Service Manager owns service lifecycle.
- File-Server Services own durable domain state and expose aP trees.
- the channel System Store provides opaque Host backing to those services.
- Standard Namespace provides `/srv`, `/mnt`, `/lib`, and `/bin`.
- Processes receive bounded namespace entries and descriptors.
- Skills enter through installed packages or explicit descriptors; Host
  directories are never implicit providers.

This change supplies the missing package owner without adding another resolver,
profile model, or Host API.

## Goals

- Put installed package lifecycle under one Alan OS File-Server Service.
- Keep package persistence channel-isolated and service-owned.
- Make install and management explicit, transactional, and inspectable.
- Project package content through Alan OS namespaces without leaking Host paths.
- Preserve the distinction between installed content and Process authority.
- Delete the Quartermaster and implicit-source model.

## Non-goals

- A universal package ecosystem, registry, dependency solver, or trust network.
- Source-control operations or foreign-format conversion in Package Service.
- Ambient package discovery from Host directories.
- Installing services, boot units, apps, models, or MCP servers in this slice.
- Giving every Process every installed package.

## Decisions

### D1. Package Service is the sole installed-package owner

Package Service owns installed package ids, validated content, computed content
digests, provenance, active revisions, and lifecycle events. Provenance records
the manifest schema, declared package version, content digest, operation, and
time; it never persists the source namespace path. No Host object,
Agent Execution Engine registry, or second package catalog may mutate or resolve
installed state.

Service Manager starts Package Service as a required boot unit and supervises
it like every other File-Server Service. The service publishes `packages` at
`/srv/packages`; Service Manager mounts the management tree at
`/mnt/packages`.

The Package Service executable and `/bin/pkg` Tool are base-system boot
artifacts. This narrow bootstrap does not create an alternate installed-package
catalog. First-party Skill packages are ordinary Package Service installations.

### D2. The System Store is backing, not a public package path

The Host grants Package Service one channel-specific backing binding under its
System Store service subtree. Package Service alone defines the subtree format.
Clients cannot name packages by `~/.alan`, `~/.alan-dev`, a workspace, or the
raw System Store path.

Stable and dev therefore remain isolated without inventing package profiles.
Live transactions are reopened or failed by Package Service after restart;
Process namespaces and descriptors are rebuilt rather than persisted.

### D3. Install input is an explicit Alan package tree

`pkg install <path>` accepts only a directory readable in the caller's Alan OS
namespace. The directory contains a declarative `alan-package.yaml` with:

- a schema version;
- one package id;
- zero or more Skill export paths;
- zero or more explicit Tool exports mapping a command name to an executable
  path; and
- an optional human-facing version.

The service computes the content digest; it does not trust a caller-supplied
digest as evidence. Export paths must be relative, canonical, inside the package
tree, and free of escaping symlinks. Skill exports must satisfy
`skill-system-contract`; Tool command names and executables must satisfy the
existing `/bin` Tool contract.

This is intentionally not a Git installer. A Host checkout must first be
mounted explicitly, and foreign content must first be adapted into an Alan
package tree outside Package Service. Remote fetching and adapters can be added
as separate Tools later without changing package ownership.

### D4. Lifecycle is staged and atomic

The `pkg` Tool copies the explicit source into a service-owned transaction area.
The service validates the manifest, content, exports, package-id ownership, and
namespace collisions before activation. Committing a transaction atomically
switches the package id to the new content digest.

The management tree is file-native:

```text
/mnt/packages/
├── catalog/<package-id>/
│   ├── manifest
│   ├── digest
│   ├── provenance
│   └── state
├── transactions/<transaction-id>/
│   ├── manifest
│   ├── content/
│   ├── ctl
│   └── status
└── events
```

`manifest`, `digest`, `provenance`, and `state` are snapshots. `events` is an
offset-resumable append-only stream. A transaction commits or aborts through
its adjacent `ctl`; partial upload never changes the active catalog.

`update` is the same transaction with an existing package id and expected
active digest. `remove` atomically removes the catalog entry and Package
Service-owned content. It never deletes the original mounted source.

### D5. Installation and Process exposure are separate

Installed content is not ambient authority. Namespace assembly explicitly
selects package ids for a child Process:

- selected package content is mounted read-only at
  `/lib/pkg/<package-id>`;
- only explicitly exported and selected Tools are bound at `/bin/<tool>`;
- package-local `bin/` and `scripts/` files remain ordinary package content;
- unselected installed packages have no `/lib/pkg` or `/bin` entry in that
  Process.

Skill resolution asks Package Service to open a declared Skill export and
passes the resulting bounded descriptor to an Agent Process. The agent runtime
does not recursively scan `/lib/pkg` and does not receive all installed Skills
merely because their content exists in the store.

This permits shared package resources under `/lib/pkg/<package-id>` while
keeping Skill activation and Tool execution explicit.

### D6. Management belongs in Alan Shell

The base-system `/bin/pkg` Tool implements:

- `pkg install <namespace-path>`
- `pkg list`
- `pkg show <package-id>`
- `pkg update <package-id> <namespace-path>`
- `pkg remove <package-id>`

The Tool reads and writes `/mnt/packages`; it has no private Host call and no
second state store. `alan q`, `q`, and a package-management mode on Alan OS Host
are removed from the design.

Alan for macOS needs no package-specific bridge. A future UI may invoke the
same Tool or operate on the same mounted tree after attachment design is ready.

### D7. First-party packages use the ordinary path

Required first-party Skill packages ship as canonical Alan package artifacts.
During Package Service bootstrap, missing or changed required packages are
installed transactionally into its System Store subtree before the service is
ready. They use the same manifest, catalog, projections, and Skill descriptors
as third-party packages. Required packages may reject removal, but they do not
gain a second discovery mechanism or privileged prompt behavior.

### D8. Migration deletes the old model

Implementation removes Quartermaster naming, `q` commands, Host package stores,
provider registries, workspace/AgentRoot/`.agents` source scanning, direct
engine package resolution, and compatibility overlays. Existing authored Host
content is reported for explicit import and is never silently copied or
deleted, following `alan-os-system-store`.

ADR-0030 is marked superseded by ADR-0052 and this change. The historical
rationale remains in git history; current docs do not preserve its rejected
runtime model as an alternative path.

## Failure behavior

- Invalid manifests, escaped paths, duplicate ids, duplicate Tool names, and
  unsupported exports fail before activation.
- A required Package Service boot failure fails boot; a later crash invalidates
  `/srv/packages` and follows Service Manager restart policy.
- Interrupted transactions never replace active content.
- A requested package, Skill export, or Tool export that is absent or not
  selected fails closed; callers do not fall back to Host scanning.
- Package backing corruption is reported by digest mismatch and the affected
  package is unavailable until reinstalled.

## Deferred

Registry lookup, network fetch, signatures, dependencies, lockfiles, garbage
collection beyond unreachable transaction cleanup, package-provided boot units,
Agent Executables, apps, models, MCP servers, knowledge packs, and foreign
format adapters each require separate pressure and contracts.
