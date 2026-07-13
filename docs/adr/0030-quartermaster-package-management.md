# Quartermaster: Package Service as an Alan OS Organ

Status: Accepted. Extends [ADR-0024](0024-plan9-kernel-model.md) and
[ADR-0027](0027-north-star-capability-map.md). The normative v0 behavior lives
in OpenSpec `package-management-contract`. ADR-0052 supersedes this ADR's
original Host-directory source model.

## Context

Alan needs a supported way to adopt external Skill repositories, preserve their
provenance and shared assets, and reproduce the capability set passed to an
Agent Process. Earlier package-management design predated the system-level Alan
OS architecture and proposed `~/.alan/pkg`, implicit workspace/AgentRoot/
`~/.agents` providers, and a Host-side resolver. Those are incompatible with
the accepted System Store, explicit Host Mount, Service Manager, Standard
Namespace, and descriptor-only Skill model.

The missing organ is still real, but its owner and boundaries are now clear.

## Decisions

### D1. Package management is an Alan OS system service

The formal component is **Package Service**. It is a required File-Server
Service started and supervised by Service Manager. It publishes a mountable
handle at `/srv/package` and owns its install-channel subtree of the System
Store.

The subsystem's product name remains **Quartermaster** and its command remains
`q`. Package Service owns package identity, immutable revisions, materialized
exports, provenance, references, transactions, retirement, and deletion. Alan
Kernel owns none of those concepts.

### D2. `q` is an Alan OS Process

`q` is bound at `/bin/q` and launched from Alan Shell through `/proc/clone`.
It communicates with Package Service over its aP file surface and returns
ordinary Process output. Package management is not selected when Alan OS boots,
and the Host `alan` CLI is not a second package authority.

The command vocabulary is intentionally plain:

- `q install <namespace-path>`
- `q list`
- `q upgrade <package-id> <namespace-path>`
- `q uninstall <package-id>`

### D3. Sources enter through namespace authority

Package Service never scans Host directories. A source must already be readable
in the invoking Process namespace, normally under `/mnt` after Host Mount
authorization. `q` imports a confined snapshot through aP; persisted package
records contain normalized relative content and fingerprints, never raw Host
paths or credentials.

Package ids are 1–64 ASCII bytes matching
`[a-z0-9]+(?:-[a-z0-9]+)*`, compared exactly without case folding or Unicode
normalization. A bare portable Skill root with `SKILL.md` remains installable
without gaining an Alan-specific manifest.

Remote fetching is deferred. A future network-fetch service may produce a
namespace tree and feed the same import contract. `q` does not acquire ambient
Host network or credential authority.

### D4. Package data belongs to the System Store

Package Service owns `services/packages` in the active channel System Store.
Its backing layout and serialization are private implementation details.
Agent-visible identity is a package id, immutable revision, service handle, and
namespace projection—not a Host filesystem path.

Stable and dev stores are independent. Publication is transactional, and store
corruption fails closed rather than being repaired from an ambient source.

### D5. Package is the distribution unit

The term is **distribution package**, or simply **package** in context. The
rejected term “Capability Package” collides with existing Alan capability
vocabulary.

A v0 distribution package is one imported source snapshot that may export
multiple ordinary directory-backed Skill packages. Native `SKILL.md` packages
remain portable. Supported command-style Markdown may be converted with a
versioned Alan adapter preamble. The materialization manifest records every
export and generated file.

V0 distributes Skills only. Package-local scripts and `bin/` files may remain
readable assets but do not become Tools. Tool execution remains owned by the
canonical `/bin/<tool>` Process contract.

### D6. Installation is not Process authority

An installed package is visible in Package Service's catalog, not automatically
in every namespace. A parent or Boot Unit must pass an explicit package
reference when creating a Process. Package Service resolves the reference to an
immutable revision projected read-only at `/lib/pkg/<package-id>`.

Only referenced packages appear in a Process namespace and Agent capability
view. Running Process snapshots do not change when a package is upgraded or
uninstalled. New Processes resolve the new catalog state. Process launch
rejects duplicate runtime Skill ids across all selected package and descriptor
roots before capability assembly.

Explicit Skill and Agent Definition descriptors remain a separate, supported
authority. They do not register their Host directories as package sources.

### D7. First-party Skills use the ordinary package path

Alan's first-party Skill trees seed deterministic preinstalled Package Service
packages. The Root Agent Process Boot Unit references them explicitly. Agent
Execution Engine does not append `builtin_capability_packages()` or use another
compiled-in discovery path.

`builtin` remains useful provenance and precedence vocabulary; it no longer
means privileged injection.

### D8. Lifecycle is immutable and exact

Package revisions are deterministic content fingerprints over normalized input
and materializer version. Install and upgrade validate a complete staged
revision before an atomic catalog switch. Identical input is idempotent.

Uninstall removes future resolution immediately. Revisions with live Process
references enter a retiring state and remain readable until the final reference
is released, after which Package Service deletes all managed content.
Preinstalled first-party packages are not operator-uninstallable in v0.

### D9. Unsupported capabilities fail honestly

Materialization records unsupported Tool or runtime-capability dependencies as
typed availability issues. The existing Skill availability machinery surfaces
them. Package Service does not silently emulate foreign behavior or route
helpers through a Host execution escape hatch.

## Consequences

- Package management extends Alan OS owners instead of creating parallel
  profile, store, permission, or execution systems.
- Operators must explicitly mount and install Host content; old ambient package
  discovery is intentionally not compatible.
- V0 proves ownership and lifecycle with local namespace trees before adding
  remote distribution infrastructure.
- Per-Process references make reproducibility and least authority possible, but
  package changes do not hot-update running Agents.
- Future package types must extend Package Service while respecting their
  existing runtime owners: Tools in `/bin`, services under Service Manager,
  knowledge in Memory/Knowledge Stores, and apps in Alan Apps.
