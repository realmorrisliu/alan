# add-alan-package-management

## Why

Alan OS can discover installed Skills, but it does not yet have a system owner
that installs, inspects, updates, or removes the packages that contain them.
The previous proposal filled that gap with a Host-side Quartermaster resolver,
Alan-home stores, and implicit workspace or AgentRoot sources. Those concepts
conflict with the system-level Host, Service Manager, Standard Namespace, and
System Store that now exist.

Alan OS needs one package lifecycle owner inside the OS. Host files should enter
only through an explicit mount, installed state should belong to one durable
service, and Process access should be expressed through namespace projections
and descriptors.

## What Changes

- Add **Package Service**, a required File-Server Service started and supervised
  by Service Manager.
- Make Package Service the sole owner of installed package identity, content,
  provenance, lifecycle, and catalog state.
- Persist package state only in Package Service's channel-isolated System Store
  subtree. Raw Host backing paths are not package identity or a client API.
- Publish the service at `/srv/packages` and mount its management tree at
  `/mnt/packages`.
- Add the Alan Shell Tool `/bin/pkg` for `install`, `list`, `show`, `update`,
  and `remove`. It operates through the mounted service tree; no Host-side
  package command or background management API is introduced.
- Install only explicit, namespace-readable Alan package trees with a
  declarative `alan-package.yaml`, or one bare portable Skill root containing
  `SKILL.md`. Alan OS does not scan workspace, AgentRoot, `.agents`, Alan home,
  or arbitrary Host directories for packages.
- Stage, validate, and atomically activate package content. Updates replace one
  installed revision; removal deletes only Package Service-owned state.
- Project explicitly selected package content read-only at
  `/lib/pkg/<package-id>`. Explicit Tool exports may be bound into `/bin`; files
  under a package's `bin/` or `scripts/` do not become Tools implicitly.
- Resolve installed Skills through Package Service, then pass selected Skills
  to Agent Processes by descriptor. Installing a package does not expose all of
  its Skills or Tools to every Process.
- Install first-party Skill packages through the same manifest and lifecycle
  rules as third-party packages.
- Supersede ADR-0030's Quartermaster, `q`, Host-store, local-provider, and
  engine-resolver decisions with the Package Service model accepted by
  ADR-0052.

## Capabilities

### New Capabilities

- `package-management-contract`: Package Service ownership, package artifact
  validation, transactional lifecycle, management tree, namespace projections,
  descriptor resolution, and Alan Shell operations.

### Modified Capabilities

- `alan-os-system-store`: Package Service owns all durable installed-package
  state inside its service subtree.
- `service-manager`: Package Service is a required supervised boot unit that
  publishes `/srv/packages` before package-dependent Processes start.
- `skill-system-contract`: installed Skill discovery resolves through Package
  Service while explicit Skill and Agent Definition descriptors remain valid;
  first-party packages use the ordinary installed-package path.

## Impact

- New `alan-package-service` crate implementing the aP file tree and durable
  package transactions.
- `alan-service-manager` starts Package Service, supplies its System Store
  binding, and composes its `/srv`, `/mnt`, `/lib/pkg`, and `/bin` surfaces.
- `alan-os-host` passes only the Package Service backing binding; it does not
  own package semantics.
- `alan-shell` provides the `pkg` Tool against `/mnt/packages`.
- `alan-agent-engine` stops resolving package content from Host roots and
  accepts only Package Service-backed or explicitly passed Skill descriptors.
- `AGENTS.md` and `CONTEXT.md` define Package Service as the canonical component
  name.
- Legacy Quartermaster/Q code and implicit Host-directory package readers are
  deleted rather than retained behind compatibility adapters.

## Out of Scope

- Registries, dependency solving, lockfiles, signatures, and remote fetching.
- Git-specific lifecycle or credentials.
- Automatic conversion of Claude Code, Codex, or other foreign package formats.
- Dynamic installation of boot units, Agent Executables, apps, models, MCP
  servers, or knowledge packs.
- A Host CLI package-management surface or Alan for macOS package UI.
