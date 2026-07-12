# add-alan-package-management

## Why

alan discovers skill capabilities through two host-side paths:
`package_dirs_for_roots` enumerates `AgentRoot/skills/` and `.agents/skills/`,
then `ResolvedCapabilityView::from_package_dirs` appends built-in first-party
packages directly. Those bypasses are the legacy of a world without a
capability manager: no single owner, no lifecycle, no provenance, no
reproducible agent environment. Dogfooding the
first real external workload (ai-berkshire, a 19-skill Claude Code / Codex
investment-research repository whose skills also share repo-root `tools/*.py`)
made the gap concrete: alan cannot adopt an external skill *repository* at all,
and the natural adoption unit is the repository, not the individual skill.

This change establishes **Quartermaster (`q`) as the sole resolution authority
for skill capabilities** (ADR-0030 D6): every skill an agent can reach —
including alan's own built-ins — is a Q package with one owner and one
lifecycle. It is **slice 1** of that authority model, validated by making
ai-berkshire run.

## What Changes

- **Q becomes the single skill-resolution authority.** The legacy
  `package_dirs_for_roots` enumeration and direct
  `builtin_capability_packages()` injection are retired; the engine obtains
  its resolved skill set for an agent from Q. **BREAKING** for the internal
  discovery path (no stable external API depended on it).
- **Distribution packages** are introduced: an external source tree (git repo
  or local directory) pinned to a source revision token, assigned a unique
  package id, and held in a per-install-channel
  **package store** (`~/.alan/pkg/`, dev `~/.alan-dev/pkg/`), projected
  read-only at **`/lib/pkg/<package-id>`** in the Alan OS namespace.
- **All skill sources become Q packages / providers**, physical unification
  (ADR-0030 D6):
  - built-in first-party skills are reseeded as Q **pre-installed packages**;
  - `AgentRoot` and workspace skills are registered as Q **local-source
    packages**;
  - external repositories install as distribution packages.
  No skill reaches an agent through a bypass source.
- **Materialization** turns external content into skill packages inside the
  store: Claude Code command-style `.md` files convert to directory-backed
  packages (verbatim body + versioned **adapter preamble** + emitted tool or
  runtime-capability dependencies); portable `SKILL.md` packages are adopted in
  place. Q resolves only manifest-selected skill roots; the verbatim source
  tree remains package content, not an implicit recursive skill source. Shared
  helpers resolve at `/lib/pkg/<package-id>/...`, never a host path.
- **Lifecycle** is exact: package provenance (source identity, revision token,
  converter version) and a materialization manifest make `q upgrade` idempotent, protect
  local edits (warn, never silently overwrite), and make `q uninstall`
  complete. `q list` reports installed packages and unsatisfied capabilities.
- **Honest failure**: recognized foreign vocabulary with no alan equivalent
  (web search, Team orchestration) becomes an unsatisfied typed
  `runtime_capability` dependency and is surfaced through
  the existing `skill_availability_issues` machinery, never silently degraded.
- **Out of scope** (later slices, ADR-0030 D7): agent runtime self-discovery
  from manifest-selected roots under `/lib/pkg` (gated on
  `refactor-engine-namespace-native`); additional
  package types (MCP, tools/binaries, workflows, models, knowledge packs);
  permission-to-policy wiring; `q` in `/bin`; user-configurable package
  profiles beyond the baseline per-Process resolved-set isolation;
  lockfile/registry/signing; web-capability and multi-agent gaps
  (seeded in the design doc). Core **tools** (`read`/`write`/`edit`/`bash`/…)
  stay compiled-in `Box<dyn Tool>` — kernel, not Q packages.
- Third-party skill content does not enter this repository; CI uses synthetic
  fixture repositories.
- Git clone metadata and clone-local credentials never enter package content;
  `/lib/pkg` exposes an exported working tree without VCS control directories.
- Persisted source identity is credential-free, and helper execution rejects
  symlink targets that escape the canonical package entry.
- Store-backed helpers fail closed as unavailable unless the active sandbox
  backend enforces package-entry-only reads within the channel Alan home.

## Capabilities

### New Capabilities

- `package-management-contract`: Quartermaster as the sole skill-resolution
  authority — distribution packages, the package store and `/lib/pkg`
  projection, the provider model (pre-installed / local-source / distribution),
  materialization rules (conversion, adoption, adapter preamble,
  typed dependency emission, `/lib/pkg` helper addressing), provenance,
  manifest, install/list/upgrade/uninstall, and honest failure.

### Modified Capabilities

- `skill-system-contract`: discovery moves from multi-source host enumeration
  to Q resolution; built-in and agent-root/workspace sources become Q
  providers; adds `provenance` as a stable `package.yaml` sidecar. Modifies the
  discovery, first-party, channel-scoped-source, and workspace-source
  requirements accordingly.

## Impact

- `crates/agent-engine`: `package_dirs_for_roots` (`agent_definition.rs`) and
  the later `builtin_capability_packages()` injection (`capability_view.rs`)
  are retired; the resolved capability view is fed only by Q resolution.
  Built-in skill distribution reseeds into the store. Reuses existing frontmatter validation
  (`parse_skill_metadata`, `validate_capabilities`) and availability reporting
  (`skill_availability_issues`).
- `crates/alan` CLI: new Quartermaster (`q`) command family; store backing under
  the channel alan home; materialization logic as library code so tests drive
  it without the CLI.
- Execution backend: resolve tool-execution paths under `/lib/pkg` through the
  store projection (deterministic prefix mapping), with one narrow guard
  exception for runtime-resolved store paths; direct agent references to the
  backing stay denied.
- `openspec/specs/skill-system-contract/spec.md`: MODIFY discovery / first-party
  / channel-source / workspace-source requirements; ADD provenance sidecar.
- `CONTEXT.md`: glossary entries for *Quartermaster*, *distribution package*,
  *package store*, *materialization*, *skill provider*, *adapter preamble*,
  *package provenance*.
- `docs/skill_authoring.md` / `docs/skills_and_tools.md`: operator-guide
  pointers (non-normative).
