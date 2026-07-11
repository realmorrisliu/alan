# add-alan-package-management

## Why

alan's skill system has never carried a real external workload. The first
dogfooding target — ai-berkshire (`github.com/xbtlin/ai-berkshire`), a 19-skill
Claude Code / Codex investment-research repository — exposed two gaps at once.
First, alan has no supported way to bring external skill content into a skill
source at all: Claude Code command-style `.md` files cannot be discovered, and
even portable `SKILL.md` packages can only be hand-copied with no provenance, no
update path, and no protection against silent capability degradation. Second,
and more fundamental, the workload's natural unit of distribution is the
**repository** — its 19 skills share repo-root helper tools (`tools/*.py`) —
while alan's skill package contract deliberately exports exactly one skill per
directory. Installing skill-by-skill severs those shared dependencies; no
per-skill import tool can fix a wrong distribution unit. alan needs a package
layer above skill packages.

## What Changes

- Introduce **distribution packages** and a package management contract: a
  distribution package is an external source tree (git repository or local
  directory) pinned to a commit, held in a per-install-channel **package
  store**, from which alan **materializes** skill packages into existing skill
  sources.
- Add the **Quartermaster** command family (`q`, per
  [ADR-0029](../../../docs/adr/0029-quartermaster-package-management.md); v0
  hosts it in the alan CLI until the shell command namespace lands):
  - `q install <git-url|path>` — fetch to the store, materialize skills;
  - `q list` — installed packages with provenance;
  - `q upgrade` — idempotent re-install against the moved upstream;
  - `q uninstall` — remove materialized files per manifest plus the store
    entry.
- Materialization reuses the conversion primitives designed for this workload:
  - Claude Code command-style single `.md` files convert to directory-backed
    alan skill packages (verbatim body, generated frontmatter, standard
    **adapter preamble**);
  - existing portable `SKILL.md` packages are adopted via validate+copy;
  - recognized foreign tool vocabulary (`$ARGUMENTS`, `WebSearch`,
    `TeamCreate`/`TaskCreate`) is mapped or explicitly declared unavailable,
    and emitted as `capabilities.required_tools` so missing host capabilities
    surface through the existing `skill_availability_issues` mechanism instead
    of silently degrading.
- The store is projected read-only at **`/lib/pkg/<name>`** in the Alan OS
  namespace (alan-kernel aP namespace, host-agnostic; existing
  host-directory-mounts machinery). Agent Processes see only the Alan OS file
  system, so `/lib/pkg` is the canonical — and only — address for package
  content in contracts and generated skill content.
- Shared in-repo helpers (e.g. `tools/*.py`) stay resolvable after install:
  materialized skills reference them at `/lib/pkg/<name>/...`, dissolving the
  out-of-package-reference problem that per-skill import could only warn
  about; the execution backend resolves `/lib/pkg` paths to store backing when
  spawning helper processes.
- Package-level provenance (source repository, commit, converter version) and a
  materialization manifest make upgrade idempotent, protect locally modified
  files (warn, never silently overwrite), and make uninstall exact.
- **Deliberately out of scope**: registry, semantic version resolution,
  dependency graphs, lockfiles, signing — a package's "version" is its source
  commit. Web-search capability and multi-agent orchestration remain follow-up
  changes; this change's bar for such gaps is honest, user-visible
  unavailability, with the ai-berkshire dogfooding findings recorded in the
  design doc as seeds.
- Third-party skill content does not enter this repository; CI coverage uses
  synthetic fixture repositories that mimic the external shapes.

## Capabilities

### New Capabilities

- `package-management-contract`: the durable contract for distribution
  packages — package store, install/list/upgrade/uninstall semantics,
  materialization rules (conversion, adoption, adapter preamble,
  `required_tools` emission, the `/lib/pkg` namespace projection for shared helpers),
  provenance, manifest, idempotence, and local-modification protection.

### Modified Capabilities

- `skill-system-contract`: add `provenance` as a stable `package.yaml` sidecar
  block on materialized skill packages (currently only `skill_defaults.runtime`
  keys are stable), identifying the owning distribution package; field
  semantics owned by `package-management-contract`.

## Impact

- `crates/agent-engine` skills module: no discovery/loader changes; skills
  materialize into existing skill sources and reuse existing frontmatter
  validation (`parse_skill_metadata`, `validate_capabilities`) and availability
  reporting (`skill_availability_issues`).
- `crates/alan` CLI: new Quartermaster (`q`) command family; store *backing*
  under the channel alan home (`~/.alan/pkg/`, dev `~/.alan-dev/pkg/`) — an
  implementation detail; the alan home's sensitive-read denylist keeps the
  backing agent-opaque, so the `/lib/pkg` projection is the only working
  address; materialization logic as library code so tests drive it without
  the CLI.
- Execution backend: resolve tool-execution paths under `/lib/pkg` through the
  store projection (deterministic prefix mapping for one declared mount), with
  one narrow guard exception for runtime-resolved store paths; direct
  agent references to the backing stay denied.
- `openspec/specs/skill-system-contract/spec.md`: delta for the `package.yaml`
  `provenance` block. The one-skill-per-package rule is untouched —
  distribution packages sit above it, not inside it.
- `CONTEXT.md`: glossary entries for *distribution package*, *package store*,
  *materialization*, *adapter preamble*, *package provenance*.
- `docs/skill_authoring.md` / `docs/skills_and_tools.md`: operator-guide
  pointers to the new surface (non-normative).
