# add-alan-package-management — design

## Context

ai-berkshire (`github.com/xbtlin/ai-berkshire`) is the first real external
workload pointed at alan's skill system. It ships the same content in two
forms:

- `skills/*.md` — canonical Claude Code command-style single files: body text
  with `$ARGUMENTS`, no portable frontmatter, references to Claude-only
  surfaces (`WebSearch`, `TeamCreate`/`TaskCreate`, background subagents,
  permission whitelists).
- `codex-skills/<name>/SKILL.md` — generated portable packages with
  `name`/`description` frontmatter plus a hand-written "Codex adapter note"
  preamble mapping Claude-only vocabulary onto Codex capabilities.

Critically, all 19 skills invoke shared repo-root helpers
(`python3 tools/financial_rigor.py ...`). The repository, not the individual
skill, is the unit of distribution.

Current alan state (verified in-code):

- The loader (`crates/agent-engine/src/skills/loader.rs`) accepts any
  `SKILL.md` with `name` + `description` frontmatter, so the generated packages
  are already discoverable if hand-copied. Command-style single files are not.
- `capabilities.required_tools` and typed
  `compatibility.dependencies.runtime_capability` entries flow into
  `skill_availability_issues` (`skills/types.rs`), so missing-capability
  detection needs no new runtime mechanism. Required tools may fall back to a
  same-named PATH executable; unsupported Alan surfaces therefore must use a
  runtime-capability dependency, whose unknown names remain unsatisfied.
- There is no `$ARGUMENTS` substitution in the engine; alan's model is implicit
  invocation / force-select, not slash-commands-with-args.
- skill-system-contract fixes one exported skill per skill package and already
  speaks install vocabulary (install sources, install channels).
- Discovery today has two bypass paths: `package_dirs_for_roots`
  (`agent_definition.rs`) builds a `Vec<ScopedPackageDir>` from AgentRoot and
  workspace/user `.agents/skills/`, then
  `ResolvedCapabilityView::from_package_dirs` appends
  `builtin_capability_packages()` directly. There is no single owner of an
  agent's capability set — both paths are retired by this change.
- Built-in skills are distributed by compiled-in registration, not as
  installable packages; making them Q packages is a reseed, not a rewrite of
  the skill package contract (skill-system-contract already says built-in
  distribution is "a packaging detail, not a different contract").
- The execution guard's sensitive-read denylist covers the channel alan home
  directories (`~/.alan`, `~/.alan-dev`; `sandbox.rs`
  `sensitive_read_denylist_for_home`) because they hold operator configuration
  and managed credentials. Under canonical `/lib/pkg` addressing this is a
  feature, not a blocker: agents reach package content only through the projection and the
  exec resolver, and any host path that leaks into content fails fast at the
  guard.
- The two upstream source forms overlap completely: all 19 command-style
  skills also exist as generated `codex-skills/*/SKILL.md` packages (whose
  hand-written preamble targets *Codex* vocabulary, not alan's). Naive
  convention-scan would materialize every skill twice under colliding ids.
- The upstream package's documented worst failure (issue #58) is a background
  agent silently losing web access and producing training-data pseudo-research.
  This contract treats prevention of that failure class as first-order.

An earlier draft of this change scoped the surface to per-skill import
(`alan skill install`). It could only warn about the shared `tools/*.py`
dependency, because the distribution unit was wrong. This design supersedes it
with a package layer; the per-skill conversion/adoption primitives survive
unchanged as materialization rules.

## Goals / Non-Goals

**Goals:**

- Make Quartermaster the sole resolution authority for skill capabilities:
  retire multi-source enumeration, route every skill through Q as a provider
  package (pre-installed built-in / local-source agent-root / distribution).
- A supported, repeatable way to adopt an external repository of skill content
  as one **distribution package**, preserving upstream as the source of truth.
- Conversion of Claude Code command-style `.md` files into directory-backed
  alan skill packages during materialization.
- Shared in-repo helpers remain resolvable after install via the store's
  canonical `/lib/pkg` projection in the Alan OS namespace.
- Honest failure: foreign capability requirements become machine-readable
  declarations surfacing through existing availability reporting.
- Exact lifecycle: provenance and a materialization manifest make upgrade
  idempotent and uninstall complete.

**Non-Goals:**

- Registry, semantic version resolution, dependency graphs, lockfiles,
  signing. A package's version is its source revision token.
- Web-search capability (follow-up change; see Gap findings).
- Multi-agent orchestration parity with Claude Code Team tools (follow-up).
- Emulating foreign tools or rewriting prompt semantics.
- Vendoring ai-berkshire content into this repository.
- Codex prompt formats (`codex-prompts/`) — marginal value over command `.md`.
- Changing the one-skill-per-package rule, loading, or exposure — only the
  discovery *source* changes (to Q resolution).
- Agent runtime self-discovery from manifest-selected roots under `/lib/pkg` —
  gated on Ring 2; slice 1 feeds the engine through Q's host-side resolution
  interface.
- Force-migrating agent-root/workspace skills into the global store — they
  stay local-source providers at their existing location.
- Tool resolution. Core tools stay compiled-in `Box<dyn Tool>`; `tools=/bin`
  is `refactor-engine-namespace-native`'s concern.
- Per-agent package visibility via namespace binds (the real "profiles") —
  a later slice.

## Decisions

### D1: Q is the sole skill-resolution authority; providers replace enumerated sources

The engine no longer enumerates host directories for skills. Today
`package_dirs_for_roots` (`agent_definition.rs`) scans AgentRoot and
`.agents/skills/` sources, while `ResolvedCapabilityView::from_package_dirs`
injects `builtin_capability_packages()` afterward; slice 1 retires both paths
and makes **Quartermaster the one authority that resolves an agent's skill
set** (ADR-0030 D6). Every skill reaches an agent as a **Q package**, through
one of three provider kinds:

- **Pre-installed provider** — alan's first-party built-in skills, reseeded
  into the store (seeded on first run) and projected like any other package.
- **Local-source provider** — `AgentRoot`, workspace, and user
  `.agents/skills/` skills, *registered* with Q at their existing location
  (not force-migrated into the global store — that would break AgentRoot
  encapsulation) and folded into the agent's resolved set and `/lib/pkg` view.
- **Distribution provider** — external repositories installed into the store.

Physical unification means **no bypass source**: the engine sees skills only
through Q's resolution, and every skill is reachable at one `/lib/pkg`
namespace view. Two things this must not conflate (ADR-0030 D6):

- **Managed by Q** (install/register/upgrade/uninstall all flow through Q, no
  escape) — done in slice 1, not gated on anything.
- **Discovered by the agent via `ls /lib/pkg`** (the agent reading its own
  capabilities as files) — gated on `refactor-engine-namespace-native`
  (Ring 2). Until it lands, the engine takes its resolved set from Q's
  host-side resolution interface — the "use Q to find skills" path the user
  named. When Ring 2 lands, that interface degrades into walking the
  manifest-selected roots under `/lib/pkg`, never recursively scanning the
  whole projection.

Alternative rejected: **interface-only unification** (Q as a façade over
untouched enumerated sources). A façade is one reader over many owners, not one
owner; it leaves the fragmentation this change exists to remove.

### D1a: A distribution-package layer above skill packages, not inside them

**Distribution package** (the Quartermaster `q` unit) ⊇ N **skill packages**
(the existing contract's unit). Materialization emits ordinary single-skill
packages, so loading, exposure, and the one-skill-per-package rule are
untouched; only the *discovery source* changes (D1). Alternative rejected:
multi-skill skill packages — that reopens a rule skill-system-contract
deliberately closed and forces loader changes for one workload.

### D2: The unit of distribution is an identified source tree pinned to a revision

`q install <git-url|path>` places a checkout in the package store. Q derives a
package id by stripping a terminal `.git` suffix from the source basename,
lowercasing it, replacing each run outside `[A-Za-z0-9]` with `-`, and trimming
edge hyphens. The result must match `[a-z0-9]+(?:-[a-z0-9]+)*`.
`--name <package-id>` overrides the default but is validated, not normalized,
against that same path-safe slug grammar; separators, `.`, `..`, and empty
values are rejected.
The id MUST be unique within the active channel. Source identity is the
canonical git URL with an optional terminal `.git` suffix normalized away for
git sources, or canonical absolute path for local sources.
If an existing package with that id has different source identity, install
fails before any write and tells the user to choose another `--name`; neither
the store nor the projection is reused implicitly. The store's **canonical address is
`/lib/pkg/<package-id>` in the Alan OS namespace** — the
alan-kernel (aP) namespace, a host-agnostic userspace construct, not a host-OS
mount namespace — projected read-only through the existing
host-directory-mounts machinery. Its host backing lives in the
per-install-channel alan home (`~/.alan/pkg/<package-id>/` stable,
`~/.alan-dev/pkg/` dev): alan-owned state belongs under alan-controlled
directories, and channel isolation is inherited for free. Backing is an
implementation detail that never appears in contracts or generated content —
self-enforced by the alan home's presence on the execution guard's
sensitive-read denylist: only the runtime's projection and exec resolver
reach the backing, so a host path leaking into agent-visible content fails at
the guard instead of silently working (see Context). "Version" = a source
revision token (git commit, or a content fingerprint for non-git local
sources); upgrade = fetch + re-materialize. Alternatives rejected: per-skill import (wrong unit —
severs shared helpers; superseded draft) and registry-based packaging (no
registry exists and v0 needs none).

Git fetch uses a private staging clone only to resolve the revision and export
its tracked working tree. The stored `source/` tree never contains `.git` or
other VCS control metadata; clone-local config and credentialed remote URLs
therefore cannot enter `/lib/pkg`. Upgrade repeats the staged fetch/export
rather than retaining repository metadata in the projected store entry.

### D3: Skills are discovered in place from `/lib/pkg`, never copied out

Materialization does **not** copy skill packages into the shared
`~/.agents/skills` source. Skills live inside the distribution package's store
entry and are discovered in place through the `/lib/pkg` projection (D3a).
Conversion and adoption produce the skill package content; the store, not a
public directory, is where it lives:

- **Convert**: command-style `.md` without portable frontmatter → generated
  skill package (derived `name`/`description` frontmatter, versioned adapter
  preamble, body preserved byte-for-byte) written into the store entry's
  materialized layer.
- **Adopt**: directory with a valid `SKILL.md` → validated with existing
  loader rules and left in place; no copy, since it is already discoverable
  once the store is a skill source.

This is the core correction over the earlier draft, which copied conversion
output into `~/.agents/skills`. That copy was a double-write (content in
`/lib/pkg` *and* in the public source), it leaked q's output into a directory
shared with other agent ecosystems, and — because the public source is a
single stable path — it broke install-channel isolation. Discovering in place
removes all three: `/lib/pkg` is the single home for distribution content, and
channel isolation is inherited from the channel-scoped store backing
(`~/.alan/pkg` vs `~/.alan-dev/pkg`) for free. Hand-authored `~/.agents/skills`
content is not a bypass source under D1 — Q registers it as a **local-source
provider**, so it still works but flows through the one authority.

Q records the accepted skill package roots in the materialization manifest and
passes only those roots to the loader. The verbatim source tree is package
content, never an implicit recursive skill source. When one package yields the
same skill id from both source forms (ai-berkshire
ships all 19 skills both ways), **conversion from the canonical command file
wins** and the duplicate portable package is skipped with a report entry. The
counterintuitive-looking choice is deliberate: the portable duplicates carry a
preamble hand-written for *Codex* surfaces, while conversion applies alan's
own adapter preamble — the canonical body plus our adapter beats someone
else's adapter. The include/exclude scan flags remain the escape hatch.

### D3a: The store entry is layered; `/lib/pkg` projects a merged view

`/lib/pkg` is a read-only projection and the backing holds a verbatim upstream
checkout, so conversion output cannot be written back into the checkout. The
store entry therefore has two layers under its channel-scoped backing
(`~/.alan/pkg/<package-id>/`): `source/` (the exported upstream working tree,
byte-for-byte except excluded VCS control metadata) and
`materialized/` (generated skill packages plus the manifest).
`/lib/pkg/<package-id>/` projects a merged content view — original helpers
(`tools/*.py`) from `source/`, skill packages from `materialized/` — so a
converted skill and the helpers it calls share one namespace path prefix. Q's
distribution provider resolves only the skill roots selected by the manifest;
it never asks the loader to recursively scan the merged view. A skipped
portable duplicate therefore remains readable source content but cannot become
a second discovered skill. This confines the only real added complexity
of in-place discovery to one place (the store entry), instead of spreading a
double-write across the public skill source.

### D4: Adapter preamble + typed dependencies, never mechanical rewrite

The source body stays verbatim. The injected preamble (one standard, versioned
block after frontmatter):

- defines `$ARGUMENTS` as the user's current request;
- maps known foreign tool vocabulary to alan surfaces where an equivalent
  exists;
- explicitly declares vocabulary with no alan equivalent (web search, Team
  orchestration) **unavailable**, instructing the skill to state the limitation
  instead of improvising;
- resolves upstream-relative helper invocations (e.g. `tools/*.py`) to the
  package's canonical namespace path under `/lib/pkg/<package-id>/` — never to a
  host path.

Known vocabulary is converter data, versioned with the converter. Vocabulary
with an Alan tool equivalent emits `capabilities.required_tools`; vocabulary
for an unsupported Alan surface emits a typed
`compatibility.dependencies.runtime_capability` entry (for example
`web_access` or `multi_agent_orchestration`), which cannot be satisfied by an
unrelated same-named PATH executable. Unknown tool-like tokens are never
silently mapped; they go to the install report.
Alternatives rejected: mechanical token rewriting (prompt surgery, untraceable
upstream diffs) and verbatim import (reproduces the silent-degradation
failure). Mirrors the adapter-note pattern upstream already validated.

### D5: Missing capabilities fail honestly through existing machinery

Conversion emits the existing typed dependency appropriate to each recognized
surface: `capabilities.required_tools` for real tool/executable requirements,
and `compatibility.dependencies.runtime_capability` for Alan runtime surfaces.
Unsupported surfaces such as web access therefore remain unavailable until the
host explicitly implements that runtime capability. Missing dependencies
surface as `skill_availability_issues`, which the runtime already computes; the
contract requires visibility at install time (report) and at exposure time
(existing inspection surfaces). No new enforcement mechanism.

### D6: The `/lib/pkg` projection is the fix for shared helpers

Materialized skills reference in-package helpers at the canonical namespace
path (`/lib/pkg/<package-id>/tools/...`), which is stable for the life of the
installation and identical on every host. Agent Processes see only the Alan OS
file system; the store projection is how package content enters it. Helper
*execution* (spawning an interpreter on a store file) is the one place the
runtime must bridge: the execution backend resolves `/lib/pkg/<package-id>/...`
through the mount table to the store backing when spawning host processes — a
deterministic prefix resolution, since the store is one declared
host-directory mount. The guard gets exactly one narrow exception for this:
paths the runtime itself resolved through the store mount are permitted for
spawn, while agent-authored commands referencing the backing directly remain
denied by the sensitive-read denylist — which is what keeps `/lib/pkg` the
only working address. That resolution is runtime machinery, invisible to
contracts and content. Uninstall removes the store entry *and* the
materialized skills together (manifest-driven), so no skill outlives the
helpers it points at. Helpers are not copied per-skill (duplication) and not
left pointing at the user's original clone (breaks when the clone moves).

### D7: Provenance and manifest make the lifecycle exact

The store keeps, per package: provenance (source repository, commit, source
path, converter version) and a manifest of every materialized file with a
content hash. Ownership is decided by Q's provider registry and this manifest,
not by a back-pointer inside each skill — the earlier draft's per-skill
`package.yaml` provenance block is now optional metadata, not a discovery
requirement, since Q already knows which provider owns what. Semantics:

- **upgrade**, unchanged source revision token (commit, or content fingerprint
  for non-git sources) + converter → no-op; changed → re-materialize;
- materialized files diverging from manifest hashes (local edits) → warn and
  skip unless `--force`;
- **uninstall** → delete exactly the manifest's files plus the store entry;
- **list** → packages with provenance and materialized-skill summary.

### D8: The surface is Quartermaster's `q` command family, implemented in Rust

Identity and naming are settled by
[ADR-0030](../../../docs/adr/0030-quartermaster-package-management.md):
package management is an Alan OS organ named **Quartermaster**, command `q`
("Q equips agents"), verbs deliberately boring (`q install|list|upgrade|
uninstall`). In v0, before the shell command namespace lands, the family is
hosted by the alan CLI; materialization logic is library code in the existing
skills module space so tests drive it without the CLI, per the skill-authoring
tooling preference.

### D9: This change is slice 1 of the ADR-0030 authority model

Slice 1 (ADR-0030 D7) establishes Q as the sole skill-resolution authority:
providers replace enumerated sources, built-ins reseed as pre-installed
packages, agent-root/workspace become local-source providers, distribution
packages install from git/local, and the `/lib/pkg` projection is the single
physical home. Deferred to later slices: agent runtime self-discovery from
manifest-selected roots under `/lib/pkg` (gated on Ring 2), further package types, permission-to-
policy wiring, per-agent namespace binds (the real "profiles"),
reproducibility, registry/signing, `q` in `/bin`. The prior non-goal warning
against turning `/mnt` into a package manager
(define-alan-app-service-integration) is respected — the projection lives
under `/lib`, and Plan 9's `/lib` is precisely where data files belong.

## Risks / Trade-offs

- [Vocabulary table drift — ecosystems invent new tool names] → table is
  converter data with a conservative default: unknown tokens are reported,
  never mapped; a table update is a converter-version bump, which triggers
  re-materialization on next upgrade.
- [Adapter preamble subtly changes prompt behavior] → preamble is standard,
  minimal, versioned; body stays verbatim so upstream diffs remain meaningful.
- [Foreign vocabulary names collide with unrelated PATH executables] → only
  actual tool requirements use `required_tools`; unsupported Alan surfaces use
  typed runtime-capability dependencies, which PATH cannot satisfy.
- [Install report warnings ignored, silent degradation returns at runtime] →
  availability issues also surface through existing exposure/inspection paths,
  not only at install time.
- [Skill-id collision between packages or with hand-authored skills] → warn
  and skip; `--force` never steals another provider's ownership. The operator
  must uninstall the owner or choose a non-colliding package/skill id, keeping
  upgrade and uninstall exact without cross-manifest mutation.
- [Two unrelated sources normalize to the same package id] → fail before any
  write and require an explicit alternate `--name`; never alias store entries.
- [User edits a materialized skill, upgrade wants to replace it] → manifest
  hashes detect divergence; warn-and-skip unless forced; the report tells the
  user to upstream their edit or fork the package.
- [Helper execution needs namespace→backing resolution the bash path may not
  have yet] → the store is a single declared host-directory mount, so
  resolution is a deterministic prefix mapping via the mount table; it is a
  scoped v0 task, not an open-ended exec-through-namespace program. Preambles
  carry only `/lib/pkg` paths, so nothing re-materializes when the backing or
  resolution mechanism changes.
- [Q resolution becomes a single point for all skill discovery] → it replaces
  an enumeration that was already the sole discovery path; in slice 1 Q is a
  host-side library, so its failure surface equals the enumeration it replaces,
  not a new network/service dependency.
- [Reseeding built-ins changes first-party load timing or first-run behavior] →
  seeding is idempotent into the store on first run; the built-in skill
  contract is unchanged (skill-system-contract already treats built-in
  distribution as a packaging detail), so only the *source* of the same skills
  moves.
- [Large blast radius: slice 1 MODIFYs core discovery contract] → accepted per
  ADR-0030 D6 (physical unification cannot be a façade); mitigated by keeping
  loading/exposure/one-skill-per-package untouched and changing only the
  discovery source.

## Gap findings from the ai-berkshire dogfooding run (seeds for follow-up changes)

Recorded here per the change scope; none are implemented in this change.

1. **Web access capability.** alan has no web search/fetch tool; every
   research-grade skill in the workload requires it. Recommended shape per the
   alan worldview: a mountable search/fetch file server (a `searchfs` analog
   to `llmfs`), not a bolted-on host tool. Blocking for layer-2 dogfooding
   (actually producing a research report).
2. **Multi-agent orchestration.** `/investment-team` spawns four parallel
   background analysts plus a coordinating lead. alan's natural expression is
   child Agent Processes with file-based handoff (`child_runs` / delegated
   launch targets exist), but there is no contract for parallel fan-out, join,
   and result aggregation equivalent to Team/TaskCreate.
3. **Background escalation surfacing.** The workload's documented failure mode
   (background agent cannot prompt for permission, degrades silently) maps to
   alan's PolicyEngine `escalate` → Yield path for child processes; how child
   Yields bubble to the user is untested and needs a dedicated pass once
   multi-agent lands.
4. **`$ARGUMENTS` / parameterized invocation.** alan has no argument
   substitution for force-selected skills. The adapter preamble works for
   implicit invocation; whether alan wants first-class skill arguments is an
   open product question, deliberately not answered here.

## Migration Plan

This is a non-additive discovery cutover. Before replacing
`package_dirs_for_roots`, seed built-ins and register the existing AgentRoot,
workspace, and channel-scoped public sources with Q; the parity regression in
task 1.6 must show the same resolved skills. Then switch the engine to Q and
remove both the legacy directory enumeration and the
`builtin_capability_packages()` injection in one change, without shipping a
dual-resolver compatibility path.

Rollback reverts that cutover and restores `package_dirs_for_roots` plus the
`builtin_capability_packages()` injection, together with the AgentRoot,
workspace, and channel-scoped source inputs. Only after legacy discovery is restored may the `q` command/provider wiring be
removed. Existing store entries can remain inert on disk during rollback; they
must not be treated as discovered skills by the restored legacy resolver.

## Open Questions

- Exact wording of the standard adapter preamble (finalized during
  implementation; versioned so it can evolve).
- How a package names its materializable skills in v0: convention-scan
  (`skills/*.md` command-style, `*/SKILL.md` portable) versus an optional
  package manifest file in the source repo. Default assumption:
  convention-scan with explicit include/exclude flags; a source-side manifest
  can come later without breaking the contract.
