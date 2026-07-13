# add-alan-package-management

## Why

Alan OS has no system owner for installed packages. First-party Skills are still
injected directly by Agent Execution Engine, while authored Skills can only
enter through an Agent Definition descriptor. That leaves package lifecycle,
provenance, namespace projection, and repeatable installation undefined.

The previous draft filled that gap with Host-side source scanning, per-user
Alan home directories, workspace overlays, and a Host execution resolver.
Those mechanisms contradict the now-landed system-level Alan OS, explicit Host
Mount, System Store, Service Manager, and descriptor-only Skill contracts.

This change replaces that draft completely. It introduces a Package Service as
an Alan OS system service and makes installed package content enter a Process
only through an explicit Alan OS package reference.

## What Changes

- Add **Package Service (Quartermaster)** as a required File-Server Service,
  started and supervised by Service Manager and published at `/srv/package`.
- Give Package Service its own install-channel subtree in the System Store. The
  service owns the catalog, immutable package revisions, materialized Skill
  roots, provenance, and transactional lifecycle state.
- Bind **`q`** into `/bin` as the Alan OS package command. `q` is an ordinary
  Process launched from Alan Shell; it reaches package management through the
  Package Service file surface. Package management is not an Alan OS boot
  option and does not introduce a second Host-side authority.
- Accept install and upgrade sources only as explicit readable paths in the
  invoking Process namespace, normally beneath `/mnt` after Host Mount
  authorization. Package Service imports the bytes and never persists a raw
  Host path or scans ambient Host directories.
- Materialize v0 Skill distribution packages into immutable revisions. Native
  `SKILL.md` package roots are preserved; supported command-style Markdown is
  converted with a versioned Alan adapter preamble. Unsupported runtime
  capabilities remain explicit availability failures.
- Project explicitly referenced installed package revisions read-only beneath
  `/lib/pkg/<package-id>`. The backing System Store path and format are not
  Agent-visible identity.
- Resolve Skills from only two authorities: explicit installed package
  references and explicit Skill/Agent Definition descriptors. Remove the
  direct `builtin_capability_packages()` injection. First-party Skills are
  seeded as ordinary preinstalled Package Service packages and referenced
  explicitly by the Root Agent Process boot context.
- Define exact `q install`, `q list`, `q upgrade`, and `q uninstall` lifecycle
  behavior, including collision handling, content fingerprints, atomic
  publication, live-Process snapshot semantics, and complete deletion after
  references are released.
- Keep v0 deliberately narrow: Skill packages only. Remote source fetching,
  registries, signing, dependency solving, Tool package execution, live
  namespace mutation, and foreign-agent export are separate future changes.

## Capabilities

### New Capabilities

- `package-management-contract`: Package Service ownership, its `/srv/package`
  interface, System Store data, namespace source intake, immutable revisions,
  `/lib/pkg` projection, explicit package references, `q` lifecycle, and
  failure semantics.

### Modified Capabilities

- `alan-os-system-store`: Package Service owns all durable package data in its
  channel service subtree and never exposes the backing path as identity.
- `service-manager`: Package Service becomes a required supervised service;
  Service Manager publishes `/srv/package`, binds `/bin/q`, and projects only
  explicit immutable package references.
- `skill-system-contract`: installed package resolution becomes explicit,
  first-party Skills become preinstalled packages, and the direct built-in
  injection path is removed.

## Impact

- `crates/service-manager` and Alan OS boot units: Package Service lifecycle,
  publication, readiness, and package command execution.
- `crates/os-host`: Package Service System Store binding and native backing
  adapter only; no package policy or source discovery.
- `crates/agent-engine`: explicit installed-package inputs replace built-in
  injection while descriptor-passed Skills remain supported.
- `crates/shell`: generic command execution is completed so `/bin/q` can run as
  an ordinary Process without adding package-specific shell builtins.
- `docs/adr/0030-quartermaster-package-management.md`: rewritten to the landed
  system-service and explicit-reference model; ADR-0052 remains authoritative
  over the rejected Host-directory source model.

## Compatibility

This is an internal breaking change. Alan is in early development and no
compatibility scan of `~/.alan`, `~/.alan-dev`, `~/.agents`, AgentRoot, or
workspace directories is retained. Existing authored Skills continue to work
when passed by an explicit Skill or Agent Definition descriptor.
